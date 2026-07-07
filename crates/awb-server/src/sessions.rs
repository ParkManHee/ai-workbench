use serde_json::Value;
use std::fs;

pub fn parse_session_id(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    v.get("session_id")?.as_str().map(|s| s.to_string())
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
    fn session_store_roundtrip() {
        let d = std::env::temp_dir().join("awb_sessions_test"); let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let s = SessionStore::load(d.to_str().unwrap());
        assert_eq!(s.get("proj"), None);
        s.set("proj", "sid-9");
        assert_eq!(s.get("proj"), Some("sid-9".to_string()));
    }
}
