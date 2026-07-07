// crates/awb-server/src/push.rs (Task 6) — Expo 푸시 + 완료 워처
// notified 게이트(runreg::RunRegistry::mark_notified)로 WS/워처 이중 발송을 막는다:
// WS(ws.rs)가 done을 먼저 전달하면 이미 mark_notified(true)를 호출했으므로 워처는 스킵,
// WS가 붙어있지 않았던 경우엔 워처가 최초로 mark_notified하여 발송한다.
use std::fs;

#[derive(Clone)]
pub struct PushStore {
    pub path: String,
}

impl PushStore {
    pub fn load(path: &str) -> PushStore {
        PushStore { path: path.to_string() }
    }
    fn read(&self) -> Vec<String> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn write(&self, v: &[String]) {
        if let Ok(s) = serde_json::to_string_pretty(v) {
            let _ = fs::write(&self.path, s);
        }
    }
    /// 중복 토큰은 무시한다.
    pub fn add(&self, token: &str) {
        let mut v = self.read();
        if !v.iter().any(|t| t == token) {
            v.push(token.to_string());
            self.write(&v);
        }
    }
    pub fn list(&self) -> Vec<String> {
        self.read()
    }
}

/// 워처가 발송해도 되는지: WS가 이미 소비(mark_notified)했으면 false, 아직이면 true(그리고 워처가 소비 처리).
pub fn should_push(reg: &crate::runreg::RunRegistry, run_id: &str) -> bool {
    reg.mark_notified(run_id)
}

/// Expo 푸시 발송(curl 서브프로세스). 실패는 로그만 남기고 패닉하지 않는다.
pub fn send(tokens: &[String], title: &str, body: &str) {
    if tokens.is_empty() {
        return;
    }
    let msgs: Vec<_> = tokens
        .iter()
        .map(|t| serde_json::json!({"to": t, "title": title, "body": body}))
        .collect();
    let payload = serde_json::to_string(&msgs).unwrap_or_default();
    match std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://exp.host/--/api/v2/push/send",
            "-H",
            "Content-Type: application/json",
            "-d",
            &payload,
        ])
        .output()
    {
        Ok(out) if !out.status.success() => {
            eprintln!("push::send 실패(exit={:?}): {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
        }
        Err(e) => eprintln!("push::send curl 실행 실패: {e}"),
        _ => {}
    }
}

/// run 완료를 폴링하다 완료되면 run_status로 락을 해제하고, notified 게이트를 통과할 때만 푸시를 보낸다.
/// 순서: done 감지 → run_status(락 해제, WS 미접속으로 인한 락 잔류 방지) → should_push 게이트 → (필요시) send.
pub fn spawn_watch(st: crate::routes::AppState, run_id: String) {
    tokio::spawn(async move {
        let meta = match st.runs.get(&run_id) {
            Some(m) => m,
            None => return,
        };
        loop {
            let chunk = awb_core::runlog::read_log(&meta.log, 0);
            if chunk.done {
                let status = awb_core::runlog::run_status(&meta.log, &meta.workdir);
                if should_push(&st.runs, &run_id) {
                    let tokens = st.push.list();
                    let title = format!(
                        "{} {}",
                        if status.verdict.starts_with("success") { "✅" } else { "❌" },
                        meta.project
                    );
                    send(&tokens, &title, &status.verdict);
                }
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runreg::{RunMeta, RunRegistry};

    #[test]
    fn pushes_only_if_ws_did_not_consume() {
        let r = RunRegistry::new();
        r.insert(
            "x",
            RunMeta { log: "l".into(), pgid: 1, workdir: "w".into(), project: "p".into(), notified: false },
        );
        // WS가 먼저 전달한 경우
        assert!(r.mark_notified("x")); // WS가 소비
        assert!(!should_push(&r, "x")); // 워처는 스킵

        // WS가 없던 경우
        r.insert(
            "y",
            RunMeta { log: "l".into(), pgid: 1, workdir: "w".into(), project: "p".into(), notified: false },
        );
        assert!(should_push(&r, "y")); // 워처가 발송
    }

    #[test]
    fn add_ignores_duplicates_and_lists() {
        let path = std::env::temp_dir().join("awb_push_tokens_test.json").to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);
        let store = PushStore::load(&path);
        assert!(store.list().is_empty());
        store.add("ExponentPushToken[aaa]");
        store.add("ExponentPushToken[aaa]"); // 중복
        store.add("ExponentPushToken[bbb]");
        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&"ExponentPushToken[aaa]".to_string()));
        assert!(listed.contains(&"ExponentPushToken[bbb]".to_string()));
        let _ = std::fs::remove_file(&path);
    }
}
