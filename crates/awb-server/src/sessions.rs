use serde_json::Value;
use std::fs;

pub fn parse_session_id(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    v.get("session_id")?.as_str().map(|s| s.to_string())
}

/// 로그 파일 전체를 읽어 session_id를 담은 첫 줄을 찾아 반환한다.
/// WS가 붙어있지 않은(push-only) 완료 경로에서도 --resume용 세션을 캡처하기 위함
/// (WS 루프의 라인 단위 parse_session_id 호출과 동일한 로직을 파일 전체에 대해 적용).
pub fn capture_session_id_from_log(log_path: &str) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    content.lines().find_map(parse_session_id)
}

#[derive(Clone)]
pub struct SessionStore { pub dir: String }
impl SessionStore {
    pub fn load(dir: &str) -> SessionStore { let _ = fs::create_dir_all(dir); SessionStore { dir: dir.to_string() } }
    fn path(&self, project: &str) -> String {
        let safe: String = project.chars().map(|c| if c.is_alphanumeric() || c=='-' || c=='_' { c } else { '_' }).collect();
        format!("{}/{}.json", self.dir, safe)
    }
    pub fn get(&self, project: &str) -> Option<String> {
        let s = fs::read_to_string(self.path(project)).ok()?;
        let v: Value = serde_json::from_str(&s).ok()?;
        v.get("session_id")?.as_str().map(|x| x.to_string())
    }
    /// 이 세션이 터미널 세션을 갈라 만든 것이면 그 원본 ID.
    pub fn forked_from(&self, project: &str) -> Option<String> {
        let v: Value = serde_json::from_str(&fs::read_to_string(self.path(project)).ok()?).ok()?;
        v.get("forked_from")?.as_str().map(|x| x.to_string())
    }
    /// 실행이 잡은 session_id 저장. fork 실행이면 원본을 남기고, 같은 세션을 잇는 동안은 원본을 유지한다.
    pub fn record(&self, project: &str, sid: &str, fork_of: Option<&str>) {
        let origin = fork_of.map(|x| x.to_string())
            .or_else(|| if self.get(project).as_deref() == Some(sid) { self.forked_from(project) } else { None });
        let v = serde_json::json!({ "session_id": sid, "forked_from": origin });
        let _ = fs::write(self.path(project), v.to_string());
    }
}

/// 이어갈 세션과 fork 원본. 터미널에 열린 세션을 그대로 --resume 하면 두 프로세스가 한 대화 파일에 번갈아 써서
/// 서로의 턴을 모른다(2026-09-23 실측) — 그때는 갈라(--fork-session) 새 ID 로 쓴다.
/// 이미 갈라 둔 원본이 다시 오면(앱의 옛 파라미터·큐 적재분) 갈라 둔 쪽을 잇는다.
pub fn decide_resume(want: Option<String>, stored: Option<String>, forked_from: Option<String>, live: &std::collections::HashSet<String>) -> (Option<String>, Option<String>) {
    let want = want.or_else(|| stored.clone());
    match want {
        Some(w) if forked_from.as_deref() == Some(w.as_str()) && stored.is_some() => (stored, None),
        Some(w) if live.contains(&w) => (Some(w.clone()), Some(w)),
        other => (other, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_session_id_from_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc-123","tools":[]}"#;
        assert_eq!(parse_session_id(line), Some("abc-123".to_string()));
        assert_eq!(parse_session_id(r#"{"type":"assistant","message":{}}"#), None);
        assert_eq!(parse_session_id("not json"), None);
    }
    #[test]
    fn capture_session_id_from_log_finds_init_line() {
        let path = std::env::temp_dir().join("awb_capture_sid_test.log").to_string_lossy().to_string();
        let content = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sid-abc\",\"tools\":[]}\n",
            "{\"type\":\"assistant\",\"message\":{}}\n",
        );
        std::fs::write(&path, content).unwrap();
        assert_eq!(capture_session_id_from_log(&path), Some("sid-abc".to_string()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn capture_session_id_from_log_none_when_absent() {
        let path = std::env::temp_dir().join("awb_capture_sid_test_none.log").to_string_lossy().to_string();
        let content = "{\"type\":\"assistant\",\"message\":{}}\nnot json\n";
        std::fs::write(&path, content).unwrap();
        assert_eq!(capture_session_id_from_log(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn session_store_roundtrip() {
        let d = std::env::temp_dir().join("awb_sessions_test"); let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let s = SessionStore::load(d.to_str().unwrap());
        assert_eq!(s.get("proj"), None);
        s.record("proj", "sid-9", None);
        assert_eq!(s.get("proj"), Some("sid-9".to_string()));
    }

    #[test]
    fn decide_resume_forks_live_terminal_and_reuses_fork() {
        let live: std::collections::HashSet<String> = ["term".to_string()].into();
        let s = |x: &str| Some(x.to_string());
        // 터미널에 열린 세션 → 갈라서 실행
        assert_eq!(decide_resume(s("term"), None, None, &live), (s("term"), s("term")));
        // 저장된 세션이 터미널 세션이어도 갈라서 실행
        assert_eq!(decide_resume(None, s("term"), None, &live), (s("term"), s("term")));
        // 이미 갈라 둔 원본이 다시 오면 갈라 둔 쪽을 잇는다(재fork 금지)
        assert_eq!(decide_resume(s("term"), s("fork1"), s("term"), &live), (s("fork1"), None));
        // 닫힌 세션은 그대로 resume
        assert_eq!(decide_resume(s("old"), None, None, &live), (s("old"), None));
    }
}
