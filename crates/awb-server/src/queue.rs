// 실행 중 도착한 후속 지시(턴)를 프로젝트별로 큐잉했다가, 현재 턴이 끝나면 자동 실행한다.
// (이전에는 409 거부 — 폰에서 "개입"이 취소밖에 없던 문제의 해소)
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct QueuedTurn {
    pub prompt: String,
    pub plan: bool,
    /// 큐잉 시점에 폰이 지정한 세션(없으면 실행 시점의 프로젝트 최신 세션으로 resume)
    pub resume_session_id: Option<String>,
}

#[derive(Clone, Default)]
pub struct TurnQueue {
    inner: Arc<Mutex<HashMap<String, VecDeque<QueuedTurn>>>>,
}

impl TurnQueue {
    pub fn new() -> Self { Self::default() }
    /// 적재 후 대기 순번(1부터)을 반환.
    pub fn enqueue(&self, project: &str, t: QueuedTurn) -> usize {
        let mut m = self.inner.lock().unwrap();
        let q = m.entry(project.to_string()).or_default();
        q.push_back(t);
        q.len()
    }
    pub fn pop(&self, project: &str) -> Option<QueuedTurn> {
        self.inner.lock().unwrap().get_mut(project).and_then(|q| q.pop_front())
    }
    /// 실행 실패(락 경합 등) 시 순서 보존을 위해 앞에 되돌린다.
    pub fn push_front(&self, project: &str, t: QueuedTurn) {
        self.inner.lock().unwrap().entry(project.to_string()).or_default().push_front(t);
    }
    pub fn len(&self, project: &str) -> usize {
        self.inner.lock().unwrap().get(project).map(|q| q.len()).unwrap_or(0)
    }
}

/// run 완료 후 호출: 큐에 다음 턴이 있으면 실행한다. 락 경합이면 5초 간격 재시도(최대 120회).
/// 성공적으로 시작되면 그 run의 워처가 done에서 다시 drain을 이어가므로 체인이 유지된다.
pub fn spawn_drain(st: crate::routes::AppState, project: String, workdir: String) {
    tokio::spawn(async move {
        for _ in 0..120 {
            let Some(t) = st.queue.pop(&project) else { return };
            let resume = t.resume_session_id.clone().or_else(|| st.sessions.get(&project));
            match awb_core::runner::start_stream_run(
                &st.claude_bin, &workdir, &st.settings_path, t.plan, &t.prompt, resume.as_deref(), &st.runs_dir,
            ) {
                Ok(h) => {
                    let run_id = crate::routes::run_id_of(&h.log);
                    st.runs.insert(&run_id, crate::runreg::RunMeta {
                        log: h.log.clone(), pgid: h.pgid, workdir: workdir.clone(), project: project.clone(), notified: false,
                    });
                    crate::push::spawn_watch(st.clone(), run_id);
                    return;
                }
                Err(_) => {
                    st.queue.push_front(&project, t);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
        eprintln!("queue::spawn_drain {project}: 재시도 한도 초과 — 큐 잔류(다음 완료 시 재시도)");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifo_with_push_front_recovery() {
        let q = TurnQueue::new();
        assert_eq!(q.enqueue("p", QueuedTurn { prompt: "a".into(), plan: false, resume_session_id: None }), 1);
        assert_eq!(q.enqueue("p", QueuedTurn { prompt: "b".into(), plan: false, resume_session_id: None }), 2);
        let a = q.pop("p").unwrap();
        assert_eq!(a.prompt, "a");
        q.push_front("p", a); // 실행 실패 → 순서 보존 복귀
        assert_eq!(q.pop("p").unwrap().prompt, "a");
        assert_eq!(q.pop("p").unwrap().prompt, "b");
        assert!(q.pop("p").is_none());
        assert_eq!(q.len("other"), 0);
    }
}
