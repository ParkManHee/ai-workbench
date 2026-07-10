// 툴 권한 원격 승인 — 승인 모드 실행에서 claude의 permission prompt를 폰으로 릴레이한다.
// 흐름: claude(--permission-prompt-tool) → MCP 스크립트 → POST /permission/request(대기)
//      → 폰이 /permission/pending 폴 + 푸시 수신 → POST /permission/answer → MCP에 allow/deny 반환.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub struct PendingPerm {
    pub project: String,
    pub tool_name: String,
    pub input_summary: String,
    /// answer 시 1회 소비 — request 핸들러의 대기(rx)를 깨운다.
    pub tx: Option<oneshot::Sender<bool>>,
}

#[derive(Clone, Default)]
pub struct PermStore {
    inner: Arc<Mutex<HashMap<String, PendingPerm>>>,
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

    /// 응답 전달(1회). 대상이 없거나 이미 응답됐으면 false.
    pub fn answer(&self, id: &str, allow: bool) -> bool {
        let mut g = self.inner.lock().unwrap();
        if let Some(p) = g.get_mut(id) {
            if let Some(tx) = p.tx.take() {
                let _ = tx.send(allow);
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
        store.insert("p1", PendingPerm { project: "proj".into(), tool_name: "Bash".into(), input_summary: "ls".into(), tx: Some(tx) });
        assert_eq!(store.pending_for("proj").len(), 1);
        assert_eq!(store.pending_for("other").len(), 0);
        assert!(store.answer("p1", true));
        assert_eq!(rx.await.unwrap(), true);
        assert!(!store.answer("p1", false)); // 이미 소비됨
        assert_eq!(store.pending_for("proj").len(), 0);
    }
}
