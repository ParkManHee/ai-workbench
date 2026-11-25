use awb_core::transcript::SessionInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

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
    // path() 가 '.' 을 '_' 로 바꾸므로 `<proj>.meta.json` 은 어떤 프로젝트의 세션 파일과도 겹치지 않는다.
    fn meta_path(&self, project: &str) -> String { format!("{}.meta.json", self.path(project).trim_end_matches(".json")) }
    pub fn metas(&self, project: &str) -> HashMap<String, SessionMeta> {
        fs::read_to_string(self.meta_path(project)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    /// name: None=그대로, Some(None)·공백뿐=이름 제거. archived: None=그대로.
    pub fn set_meta(&self, project: &str, sid: &str, name: Option<Option<String>>, archived: Option<bool>) -> std::io::Result<SessionMeta> {
        // 읽고-고쳐-쓰기 사이에 다른 요청이 끼면 한쪽 변경이 사라진다
        static LOCK: Mutex<()> = Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = self.metas(project);
        let mut m = all.remove(sid).unwrap_or_default();
        if let Some(n) = name {
            m.name = n.map(|s| s.trim().chars().take(NAME_MAX).collect::<String>().trim_end().to_string()).filter(|s| !s.is_empty());
        }
        if let Some(a) = archived { m.archived = a; }
        if m != SessionMeta::default() { all.insert(sid.to_string(), m.clone()); }
        // 원자적 교체(temp+rename) — 쓰기 중 크래시로 전 세션의 이름·보관이 날아가지 않게
        let path = self.meta_path(project);
        let tmp = format!("{path}.tmp");
        fs::write(&tmp, serde_json::to_string(&all)?)?;
        fs::rename(&tmp, &path)?;
        Ok(m)
    }
}

pub const NAME_MAX: usize = 80;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct SessionMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
}

/// `"name": null`(제거)과 키 없음(그대로)을 가르기 위해 — 기본 Option 은 둘 다 None 으로 읽는다.
pub fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Serialize)]
pub struct SessionRow {
    #[serde(flatten)]
    pub info: SessionInfo,
    pub name: Option<String>,
    pub archived: bool,
    /// 격리 트리에서 돈 세션이면 그 트리(앱이 변경 보기·커밋·이어받기에 쓴다). 옛 앱은 무시한다
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<crate::worktree::Worktree>,
}

/// 메타를 붙이고 보관(기본 제외 / archived=true 면 보관만)·검색어로 거른다.
/// 검색은 이름·질문/답 미리보기만 본다 — 원문 전문 검색은 하지 않는다.
pub fn filter_rows(list: Vec<SessionInfo>, metas: &HashMap<String, SessionMeta>, q: Option<&str>, archived: bool) -> Vec<SessionRow> {
    let q = q.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    list.into_iter().filter_map(|info| {
        let m = metas.get(&info.session_id).cloned().unwrap_or_default();
        if m.archived != archived { return None; }
        if let Some(q) = &q {
            let hit = [m.name.as_deref().unwrap_or(""), info.preview.as_str(), info.answer_preview.as_str()]
                .iter().any(|f| f.to_lowercase().contains(q.as_str()));
            if !hit { return None; }
        }
        Some(SessionRow { info, name: m.name, archived: m.archived, worktree: None })
    }).collect()
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
    fn session_meta_roundtrip_and_name_normalized() {
        let d = std::env::temp_dir().join("awb_session_meta_test"); let _ = std::fs::remove_dir_all(&d);
        let s = SessionStore::load(d.to_str().unwrap());
        s.record("proj", "sid-9", None);
        let some = |x: &str| Some(Some(x.to_string()));
        s.set_meta("proj", "a", some("  배포 수정  "), None).unwrap();
        s.set_meta("proj", "b", some(&"가".repeat(90)), Some(true)).unwrap();
        let m = SessionStore::load(d.to_str().unwrap()).metas("proj");
        assert_eq!(m["a"], SessionMeta { name: Some("배포 수정".into()), archived: false });
        assert_eq!(m["b"].name.as_deref().map(|n| n.chars().count()), Some(NAME_MAX));
        assert!(m["b"].archived);
        // 보관만 바꾸면 이름은 그대로, 공백뿐·null 은 이름 제거
        s.set_meta("proj", "a", None, Some(true)).unwrap();
        assert_eq!(s.metas("proj")["a"].name.as_deref(), Some("배포 수정"));
        s.set_meta("proj", "a", some("   "), None).unwrap();
        assert_eq!(s.metas("proj")["a"].name, None);
        s.set_meta("proj", "b", Some(None), Some(false)).unwrap();
        assert!(!s.metas("proj").contains_key("b"));
        // 같은 디렉터리의 세션 파일을 덮지 않는다
        assert_eq!(s.get("proj"), Some("sid-9".to_string()));
    }

    #[test]
    fn filter_rows_searches_name_and_previews_and_hides_archived() {
        let info = |sid: &str, preview: &str, answer: &str| SessionInfo {
            session_id: sid.into(), updated: 0, preview: preview.into(), answer_preview: answer.into(),
            count: 1, active: false, waiting: false, live: false, app: true,
        };
        let list = || vec![info("a", "질문", "답"), info("b", "Rust 빌드 오류", ""), info("c", "x", "Deploy 끝"), info("z", "Deploy 보관", "")];
        let metas: HashMap<String, SessionMeta> = [
            ("a".to_string(), SessionMeta { name: Some("Deploy Fix".into()), archived: false }),
            ("z".to_string(), SessionMeta { name: None, archived: true }),
        ].into();
        let ids = |rows: Vec<SessionRow>| rows.into_iter().map(|r| r.info.session_id).collect::<Vec<_>>();
        assert_eq!(ids(filter_rows(list(), &metas, None, false)), ["a", "b", "c"]);
        assert_eq!(ids(filter_rows(list(), &metas, None, true)), ["z"]);
        // 대소문자 무시: 이름(a)·답 미리보기(c) 에 걸리고 보관(z)은 빠진다
        assert_eq!(ids(filter_rows(list(), &metas, Some(" dEPLOY "), false)), ["a", "c"]);
        assert_eq!(ids(filter_rows(list(), &metas, Some("rust"), false)), ["b"]);
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
