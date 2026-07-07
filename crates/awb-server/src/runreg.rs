// crates/awb-server/src/runreg.rs (Task 4) — 실행 중인 run 메타데이터 레지스트리
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RunMeta {
    pub log: String,
    pub pgid: i32,
    pub workdir: String,
    pub project: String,
    pub notified: bool,
}

#[derive(Clone)]
pub struct RunRegistry {
    inner: Arc<Mutex<HashMap<String, RunMeta>>>,
}

impl RunRegistry {
    pub fn new() -> RunRegistry {
        RunRegistry { inner: Arc::new(Mutex::new(HashMap::new())) }
    }
    pub fn insert(&self, run_id: &str, meta: RunMeta) {
        self.inner.lock().unwrap().insert(run_id.to_string(), meta);
    }
    pub fn get(&self, run_id: &str) -> Option<RunMeta> {
        self.inner.lock().unwrap().get(run_id).cloned()
    }
    /// notified가 false→true로 전이할 때만 true 반환(중복 알림 방지, 1회 보장).
    pub fn mark_notified(&self, run_id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.get_mut(run_id) {
            Some(m) if !m.notified => { m.notified = true; true }
            _ => false,
        }
    }
    pub fn remove(&self, run_id: &str) {
        self.inner.lock().unwrap().remove(run_id);
    }
}

impl Default for RunRegistry {
    fn default() -> Self { RunRegistry::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mark_notified_transitions_once() {
        let r = RunRegistry::new();
        r.insert("run1", RunMeta { log: "l".into(), pgid: 10, workdir: "w".into(), project: "p".into(), notified: false });
        assert!(r.mark_notified("run1"));   // 첫 호출 true
        assert!(!r.mark_notified("run1"));   // 두번째 false(중복 방지)
        assert!(!r.mark_notified("absent")); // 없는 run false
    }
}
