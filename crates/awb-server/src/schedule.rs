// 요일·시각 반복 실행 예약(프로젝트당 여러 개). 매 회차 새 세션으로 /chat 경로를 탄다.
// 리셋 이어가기(resume.rs)와 반대로, PC 잠자기·데몬 꺼짐으로 놓친 회차는 실행하지 않고 다음 회차로 넘긴다.
use std::fs;
use std::sync::{Arc, Mutex};

/// 틱(30초)보다 넉넉히. 회차 시각에서 이만큼 넘겨 발견했으면 놓친 회차로 본다
const GRACE_SECS: u64 = 120;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Schedule {
    pub id: String,
    pub project: String,
    /// 0=일 … 6=토 (JS Date.getDay 와 같다)
    pub days: Vec<u8>,
    /// Mac 로컬 시각 "HH:MM"
    pub time: String,
    pub prompt: String,
    pub mode: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// 다음 회차(epoch초)
    pub next_at: u64,
}

pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 { return None; }
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// now 보다 뒤의 첫 회차. offset_at(t) = t 시점의 로컬 UTC 오프셋(초) — 테스트는 고정값을 넣는다.
pub fn next_after(days: &[u8], (h, m): (u32, u32), now: u64, offset_at: impl Fn(i64) -> i64) -> Option<u64> {
    let now = now as i64;
    let today = (now + offset_at(now)).div_euclid(86400); // 로컬 날짜(1970-01-01 부터 일수)
    (0..=7).find_map(|d| {
        let day = today + d;
        let wday = (day + 4).rem_euclid(7) as u8; // 1970-01-01 은 목요일
        if !days.contains(&wday) { return None; }
        let local = day * 86400 + (h * 3600 + m * 60) as i64;
        // 서머타임 경계에서도 맞게 그 시각의 오프셋으로 되돌린다
        let at = local - offset_at(local - offset_at(now));
        (at > now).then_some(at as u64)
    })
}

/// Mac 로컬 시간대(launchd 데몬은 TZ 없이 /etc/localtime)
fn local_offset(t: i64) -> i64 {
    let tt = t as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&tt, &mut tm) }.is_null() { return 0; }
    tm.tm_gmtoff as i64
}

pub fn next_local(days: &[u8], time: &str, now: u64) -> Option<u64> {
    next_after(days, parse_hhmm(time)?, now, local_offset)
}

#[derive(Clone)]
pub struct ScheduleStore {
    path: String,
    // 핸들러와 틱이 같은 파일을 읽고-고치고-쓴다 — 겹치면 한쪽 변경이 사라진다
    lock: Arc<Mutex<()>>,
}

impl ScheduleStore {
    pub fn load(path: &str) -> ScheduleStore { ScheduleStore { path: path.to_string(), lock: Arc::new(Mutex::new(())) } }
    fn read(&self) -> Vec<Schedule> {
        fs::read_to_string(&self.path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn write(&self, v: &[Schedule]) {
        if let Ok(s) = serde_json::to_string_pretty(v) {
            if let Err(e) = fs::write(&self.path, s) { eprintln!("반복 예약 저장 실패({}): {e}", self.path); }
        }
    }
    pub fn list(&self, project: &str) -> Vec<Schedule> {
        let _g = self.lock.lock().unwrap();
        self.read().into_iter().filter(|s| s.project == project).collect()
    }
    pub fn add(&self, s: Schedule) {
        let _g = self.lock.lock().unwrap();
        let mut v = self.read();
        v.push(s);
        self.write(&v);
    }
    pub fn remove(&self, project: &str, id: &str) -> bool {
        let _g = self.lock.lock().unwrap();
        let mut v = self.read();
        let n = v.len();
        v.retain(|s| !(s.project == project && s.id == id));
        let had = v.len() != n;
        if had { self.write(&v); }
        had
    }
    /// 시각이 지난 예약의 다음 회차를 파일에 먼저 쓰고, 유예 안에 발견한 것만 돌려준다 — 실행 도중 데몬이 죽어도 두 번 돌지 않게.
    pub fn take_due(&self, now: u64, next: impl Fn(&Schedule, u64) -> Option<u64>) -> Vec<Schedule> {
        let _g = self.lock.lock().unwrap();
        let mut v = self.read();
        let mut fire = vec![];
        let mut changed = false;
        for s in v.iter_mut().filter(|s| now >= s.next_at) {
            if now - s.next_at <= GRACE_SECS {
                fire.push(s.clone());
            } else {
                eprintln!("반복 예약 {} {}: {}초 늦게 발견(잠자기·데몬 꺼짐) — 이번 회차는 건너뜀", s.project, s.id, now - s.next_at);
            }
            s.next_at = next(s, now).unwrap_or(u64::MAX);
            changed = true;
        }
        if changed { self.write(&v); }
        fire
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// resume.rs 의 30초 틱이 부른다.
pub async fn run_due(st: &crate::routes::AppState) {
    for s in st.schedules.take_due(now(), |s, now| next_local(&s.days, &s.time, now)) {
        fire(st, s).await;
    }
}

/// /chat 과 같은 경로(잠금·큐·완료 푸시 포함)로 새 세션을 연다. 시작 실패는 완료 푸시가 없으니 따로 알린다.
async fn fire(st: &crate::routes::AppState, s: Schedule) {
    let body = crate::routes::ChatBody {
        prompt: s.prompt.clone(), plan: false, resume_session_id: None, mode: s.mode, model: s.model, effort: s.effort, new_session: true, isolate: false,
    };
    let res = crate::routes::chat_handler(axum::extract::State(st.clone()), axum::extract::Path(s.project.clone()), axum::Json(body)).await;
    match res {
        Ok(axum::Json(r)) => eprintln!("반복 예약 {} {}: 실행 run={:?} queued={}", s.project, s.id, r.run_id, r.queued),
        Err((code, e)) => {
            eprintln!("반복 예약 {} {}: 실행 실패({code}): {e}", s.project, s.id);
            let data = serde_json::json!({ "hostname": crate::routes::resolve_hostname(), "project": s.project });
            crate::push::send(&st.push.list(), &format!("❌ 예약 실행 실패 — {}", s.project), &e, &data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KST: i64 = 9 * 3600;
    // 2026-11-25(수) 00:00 KST
    const WED_KST: u64 = 1_795_532_400;

    fn at(day_off: u64, h: u64, m: u64) -> u64 { WED_KST + day_off * 86400 + h * 3600 + m * 60 }

    #[test]
    fn next_after_picks_weekday_time_and_crosses_midnight() {
        let kst = |_| KST;
        assert_eq!(next_after(&[3], (9, 0), at(0, 8, 59), kst), Some(at(0, 9, 0))); // 같은 날
        assert_eq!(next_after(&[3], (9, 0), at(0, 9, 0), kst), Some(at(7, 9, 0))); // 그 시각 정각이면 다음 주
        assert_eq!(next_after(&[4, 1], (9, 0), at(0, 10, 0), kst), Some(at(1, 9, 0))); // 수 → 목
        assert_eq!(next_after(&[1], (9, 0), at(0, 10, 0), kst), Some(at(5, 9, 0))); // 수 → 월
        assert_eq!(next_after(&[4], (0, 30), at(0, 23, 50), kst), Some(at(1, 0, 30))); // 자정 넘김
        // KST 수 07:00 은 UTC 로는 화요일 — 요일은 로컬 날짜로 판정한다
        assert_eq!(next_after(&[3], (8, 0), at(0, 7, 0), kst), Some(at(0, 8, 0)));
        assert_eq!(next_after(&[3], (8, 0), at(0, 7, 0), |_| 0), Some(at(0, 17, 0))); // UTC 로 보면 수 08:00 = KST 17:00
        // UTC 서쪽: EST 수 21:00 은 UTC 로는 이미 목요일 — 같은 날 22:00 이 다음 회차
        let est = |_| -5 * 3600;
        assert_eq!(next_after(&[3], (22, 0), WED_KST + 35 * 3600, est), Some(WED_KST + 36 * 3600));
        assert_eq!(next_after(&[], (9, 0), at(0, 0, 0), kst), None);
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("9:00"), None);
        assert_eq!(parse_hhmm("07:05"), Some((7, 5)));
    }

    fn sched(id: &str, next_at: u64) -> Schedule {
        Schedule { id: id.into(), project: "p".into(), days: vec![3], time: "09:00".into(), prompt: "x".into(), mode: "auto".into(), model: None, effort: None, next_at }
    }

    #[test]
    fn missed_runs_are_skipped_and_rescheduled_not_fired() {
        let path = std::env::temp_dir().join("awb_schedule_test_skip.json").to_string_lossy().to_string();
        let _ = fs::remove_file(&path);
        let s = ScheduleStore::load(&path);
        let due = at(0, 9, 0);
        s.add(sched("on-time", due));
        s.add(sched("slept", due - 3600)); // 한 시간 전 회차 — 잠자기 동안 지나갔다
        s.add(sched("later", due + 60));
        let next = |_: &Schedule, now: u64| next_after(&[3], (9, 0), now, |_| KST);
        // 데몬 재시작 = 같은 파일을 새로 로드, 첫 틱
        let fired = ScheduleStore::load(&path).take_due(due + 30, next);
        assert_eq!(fired.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["on-time"]);
        let left = s.list("p");
        assert_eq!(left.iter().map(|s| (s.id.as_str(), s.next_at)).collect::<Vec<_>>(),
            [("on-time", at(7, 9, 0)), ("slept", at(7, 9, 0)), ("later", due + 60)]);
        // 같은 회차를 다시 틱해도 두 번 돌지 않는다
        assert!(s.take_due(due + 60, next).iter().all(|x| x.id == "later"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn add_list_remove_by_project_and_id() {
        let path = std::env::temp_dir().join("awb_schedule_test_crud.json").to_string_lossy().to_string();
        let _ = fs::remove_file(&path);
        let s = ScheduleStore::load(&path);
        s.add(sched("a", 1));
        s.add(Schedule { project: "q".into(), ..sched("a", 1) });
        s.add(sched("b", 2));
        assert!(!s.remove("p", "zz"));
        assert!(s.remove("p", "a"));
        assert_eq!(ScheduleStore::load(&path).list("p"), vec![sched("b", 2)]);
        assert_eq!(s.list("q").len(), 1); // 다른 프로젝트의 같은 id 는 남는다
        let _ = fs::remove_file(&path);
    }
}
