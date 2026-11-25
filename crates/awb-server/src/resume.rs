// 사용량 한도로 멈춘 세션을 리셋 시각에 이어 실행하는 예약(프로젝트당 1개).
// 데몬이 재시작돼도 남도록 파일에 둔다.
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

/// 리셋 직후에는 아직 거절될 수 있어 여유를 둔다.
const AFTER_RESET_SECS: u64 = 60;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Reservation {
    pub session_id: String,
    pub resets_at: u64,
    #[serde(default = "crate::routes::default_mode")]
    pub mode: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
}

impl Reservation {
    pub fn due(&self, now: u64) -> bool { now >= self.resets_at.saturating_add(AFTER_RESET_SECS) }
}

#[derive(Clone)]
pub struct ResumeStore {
    path: String,
    // 핸들러와 스케줄러가 같은 파일을 읽고-고치고-쓴다 — 겹치면 한쪽 예약이 사라진다
    lock: Arc<Mutex<()>>,
}

impl ResumeStore {
    pub fn load(path: &str) -> ResumeStore { ResumeStore { path: path.to_string(), lock: Arc::new(Mutex::new(())) } }
    fn read(&self) -> HashMap<String, Reservation> {
        fs::read_to_string(&self.path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn write(&self, m: &HashMap<String, Reservation>) {
        if let Ok(s) = serde_json::to_string_pretty(m) {
            if let Err(e) = fs::write(&self.path, s) { eprintln!("resume 예약 저장 실패({}): {e}", self.path); }
        }
    }
    pub fn get(&self, project: &str) -> Option<Reservation> {
        let _g = self.lock.lock().unwrap();
        self.read().remove(project)
    }
    pub fn set(&self, project: &str, r: Reservation) {
        let _g = self.lock.lock().unwrap();
        let mut m = self.read();
        m.insert(project.to_string(), r);
        self.write(&m);
    }
    pub fn remove(&self, project: &str) -> bool {
        let _g = self.lock.lock().unwrap();
        let mut m = self.read();
        let had = m.remove(project).is_some();
        if had { self.write(&m); }
        had
    }
    /// 시각이 된 예약을 파일에서 먼저 빼고 돌려준다 — 실행 도중 데몬이 죽어도 두 번 돌지 않게.
    pub fn take_due(&self, now: u64) -> Vec<(String, Reservation)> {
        let _g = self.lock.lock().unwrap();
        let (due, keep): (HashMap<_, _>, HashMap<_, _>) = self.read().into_iter().partition(|(_, r)| r.due(now));
        if !due.is_empty() { self.write(&keep); }
        due.into_iter().collect()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn spawn_scheduler(st: crate::routes::AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            // 첫 틱은 즉시 — 데몬이 꺼져 있던 사이 지난 예약은 시작 때 한 번 실행된다
            tick.tick().await;
            for (project, r) in st.resume.take_due(now()) {
                fire(&st, project, r).await;
            }
        }
    });
}

/// /chat 과 같은 경로(잠금·큐 포함)로 "계속" 을 보낸다.
async fn fire(st: &crate::routes::AppState, project: String, r: Reservation) {
    let body = crate::routes::ChatBody {
        prompt: "계속".into(), plan: false, resume_session_id: Some(r.session_id.clone()), mode: r.mode, model: r.model, effort: r.effort, new_session: false,
    };
    let res = crate::routes::chat_handler(axum::extract::State(st.clone()), axum::extract::Path(project.clone()), axum::Json(body)).await;
    if let Err((code, e)) = res {
        eprintln!("resume-at-reset {project}: 실행 실패({code}): {e}");
        return;
    }
    let path = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).map(|p| p.path).unwrap_or_default();
    let data = serde_json::json!({
        "hostname": crate::routes::resolve_hostname(), "project": project, "path": path, "session": r.session_id,
    });
    crate::push::send(&st.push.list(), &format!("⏰ 이어가기 시작 — {project}"), "한도 리셋 — 세션을 이어갑니다", &data);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(sid: &str, resets_at: u64) -> Reservation {
        Reservation { session_id: sid.into(), resets_at, mode: "auto".into(), model: None, effort: None }
    }

    #[test]
    fn reload_takes_only_due_and_keeps_one_per_project() {
        let path = std::env::temp_dir().join("awb_resume_test.json").to_string_lossy().to_string();
        let _ = fs::remove_file(&path);
        let now = 1_000_000;
        let s = ResumeStore::load(&path);
        s.set("past", r("old", now - 3600));
        s.set("past", r("s-past", now - AFTER_RESET_SECS)); // 같은 프로젝트 → 덮어쓰기
        s.set("future", r("s-future", now));
        // 데몬 재시작 = 같은 파일을 새로 로드
        let s2 = ResumeStore::load(&path);
        let due = s2.take_due(now);
        assert_eq!(due, vec![("past".to_string(), r("s-past", now - AFTER_RESET_SECS))]);
        assert_eq!(s2.get("past"), None);
        assert_eq!(ResumeStore::load(&path).get("future"), Some(r("s-future", now)));
        let _ = fs::remove_file(&path);
    }
}
