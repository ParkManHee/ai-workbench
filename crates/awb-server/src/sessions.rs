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
    pub fn set(&self, project: &str, sid: &str) {
        let _ = fs::write(self.path(project), format!("{{\"session_id\":{}}}", serde_json::to_string(sid).unwrap()));
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
        s.set("proj", "sid-9");
        assert_eq!(s.get("proj"), Some("sid-9".to_string()));
    }
}
