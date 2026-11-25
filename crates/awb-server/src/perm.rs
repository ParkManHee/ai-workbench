// 툴 권한 원격 승인 — 승인 모드 실행에서 claude의 permission prompt를 폰으로 릴레이한다.
// 흐름: claude(--permission-prompt-tool) → MCP 스크립트 → POST /permission/request(대기)
//      → 폰이 /permission/pending 폴 + 푸시 수신 → POST /permission/answer → MCP에 allow/deny 반환.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub struct PendingPerm {
    pub project: String,
    pub tool_name: String,
    pub input_summary: String,
    /// "세션 동안 허용" 답을 기억할 claude 세션(요청 시점에 run 로그에서 못 찾았으면 None — 기억하지 않는다)
    pub session_id: Option<String>,
    /// answer 시 1회 소비 — request 핸들러의 대기(rx)를 깨운다.
    pub tx: Option<oneshot::Sender<Decision>>,
}

pub struct Decision {
    pub allow: bool,
    /// 거절 사유 — claude 에 그대로 전달된다
    pub message: Option<String>,
}

#[derive(Clone, Default)]
pub struct PermStore {
    inner: Arc<Mutex<HashMap<String, PendingPerm>>>,
    /// (session_id, tool_name) — 데몬 메모리에만 둔다. 재시작하면 다시 묻는다
    session_allowed: Arc<Mutex<HashSet<(String, String)>>>,
}

impl PermStore {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&self, id: &str, p: PendingPerm) {
        self.inner.lock().unwrap().insert(id.to_string(), p);
    }

    /// 폰 폴링용: (id, tool_name, input_summary) 목록.
    pub fn pending_for(&self, project: &str) -> Vec<(String, String, String)> {
        self.inner.lock().unwrap().iter()
            .filter(|(_, p)| p.project == project)
            .map(|(id, p)| (id.clone(), p.tool_name.clone(), p.input_summary.clone()))
            .collect()
    }

    pub fn is_session_allowed(&self, session_id: &str, tool_name: &str) -> bool {
        self.session_allowed.lock().unwrap().contains(&(session_id.to_string(), tool_name.to_string()))
    }

    /// 응답 전달(1회). 대상이 없거나 이미 응답됐으면 false.
    pub fn answer(&self, id: &str, allow: bool, for_session: bool, message: Option<String>) -> bool {
        let mut g = self.inner.lock().unwrap();
        if let Some(p) = g.get_mut(id) {
            if let Some(tx) = p.tx.take() {
                if allow && for_session {
                    if let Some(sid) = &p.session_id {
                        self.session_allowed.lock().unwrap().insert((sid.clone(), p.tool_name.clone()));
                    }
                }
                let _ = tx.send(Decision { allow, message });
                g.remove(id);
                return true;
            }
        }
        false
    }

    /// 타임아웃 등으로 대기가 끝난 항목 정리.
    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answer_resolves_waiting_request_once() {
        let store = PermStore::new();
        let (tx, rx) = oneshot::channel();
        store.insert("p1", PendingPerm { project: "proj".into(), tool_name: "Bash".into(), input_summary: "ls".into(), session_id: None, tx: Some(tx) });
        assert_eq!(store.pending_for("proj").len(), 1);
        assert_eq!(store.pending_for("other").len(), 0);
        assert!(store.answer("p1", false, false, Some("rm 말고 trash".into())));
        let d = rx.await.unwrap();
        assert!(!d.allow);
        assert_eq!(d.message.as_deref(), Some("rm 말고 trash"));
        assert!(!store.answer("p1", true, false, None)); // 이미 소비됨
        assert_eq!(store.pending_for("proj").len(), 0);
    }

    #[test]
    fn session_allow_is_keyed_by_session_and_tool() {
        let store = PermStore::new();
        let (tx, _rx) = oneshot::channel();
        store.insert("p1", PendingPerm { project: "proj".into(), tool_name: "Bash".into(), input_summary: "ls".into(), session_id: Some("s1".into()), tx: Some(tx) });
        assert!(store.answer("p1", true, true, None));
        assert!(store.is_session_allowed("s1", "Bash"));
        assert!(!store.is_session_allowed("s1", "Write"), "다른 툴은 다시 물어야 한다");
        assert!(!store.is_session_allowed("s2", "Bash"), "다른 세션은 다시 물어야 한다");
    }
}
