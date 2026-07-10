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

/// 푸시 제목/본문(순수): 마지막 메시지가 질문(선택지 있음 또는 '?'로 끝남)이면 "질문 대기"로,
/// 아니면 완료/실패 verdict로 — 사용자가 알림만 보고 "답하러 가야 하는지"를 구분할 수 있게.
pub fn push_content(project: &str, verdict: &str, last: Option<&awb_core::transcript::TranscriptMsg>) -> (String, String) {
    if let Some(m) = last {
        let is_question = m.role == "assistant" && (!m.options.is_empty() || m.text.trim_end().ends_with('?'));
        if is_question {
            return (format!("🔴 질문 대기 — {project}"), awb_core::transcript::snippet(&m.text));
        }
    }
    let emoji = if verdict.starts_with("success") { "✅" } else { "❌" };
    (format!("{emoji} {project}"), verdict.to_string())
}

/// 발송 페이로드(순수) — data는 앱이 알림 탭 시 해당 대화방으로 딥링크하는 데 쓴다.
pub fn build_messages(tokens: &[String], title: &str, body: &str, data: &serde_json::Value) -> Vec<serde_json::Value> {
    tokens
        .iter()
        .map(|t| serde_json::json!({"to": t, "title": title, "body": body, "data": data}))
        .collect()
}

/// Expo 푸시 발송(curl 서브프로세스). 실패는 로그만 남기고 패닉하지 않는다.
pub fn send(tokens: &[String], title: &str, body: &str, data: &serde_json::Value) {
    if tokens.is_empty() {
        return;
    }
    let msgs = build_messages(tokens, title, body, data);
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
                // WS가 붙어있지 않은 push-only 완료 경로에서도 --resume용 session_id를 캡처한다.
                // (WS 루프는 라인 단위로 파싱하지만, 여기선 WS가 없었을 수 있으므로 로그 전체를 읽어 찾는다.)
                if let Some(sid) = crate::sessions::capture_session_id_from_log(&meta.log) {
                    st.sessions.set(&meta.project, &sid);
                }
                if should_push(&st.runs, &run_id) {
                    let tokens = st.push.list();
                    // 마지막 메시지가 질문이면 "질문 대기" 푸시로 구분(답하러 들어갈 신호)
                    let last = st.sessions.get(&meta.project).and_then(|sid| {
                        let slug = awb_core::transcript::project_slug(&meta.workdir);
                        awb_core::transcript::last_message(&awb_core::transcript::transcript_path(&slug, &sid))
                    });
                    let (title, body) = push_content(&meta.project, &status.verdict, last.as_ref());
                    // 딥링크 데이터: 앱이 hostname으로 PC를 찾고 해당 프로젝트 대화방을 연다
                    let data = serde_json::json!({
                        "hostname": crate::routes::resolve_hostname(),
                        "project": meta.project,
                        "path": meta.workdir,
                        "session": st.sessions.get(&meta.project),
                    });
                    send(&tokens, &title, &body, &data);
                }
                st.runs.remove(&run_id); // 완료된 run은 레지스트리에서 제거(취소/완료 후 무한 누적 방지)
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
    fn push_content_distinguishes_question_from_done() {
        use awb_core::transcript::TranscriptMsg;
        let q = TranscriptMsg { role: "assistant".into(), text: "어느 방식으로 진행할까요?".into(), tools: vec![], tool_details: vec![], options: vec![] };
        let (t, b) = push_content("proj", "success", Some(&q));
        assert!(t.contains("질문 대기"), "{t}");
        assert!(b.contains("어느 방식"));
        // 선택지가 있으면 '?'로 안 끝나도 질문
        let opt = TranscriptMsg { role: "assistant".into(), text: "방식을 골라주세요".into(), tools: vec![], tool_details: vec![], options: vec!["A".into()] };
        assert!(push_content("proj", "success", Some(&opt)).0.contains("질문 대기"));
        // 평서문 완료는 verdict 푸시
        let done = TranscriptMsg { role: "assistant".into(), text: "완료했습니다.".into(), tools: vec![], tool_details: vec![], options: vec![] };
        let (t2, b2) = push_content("proj", "success(변경 3)", Some(&done));
        assert!(t2.starts_with("✅"), "{t2}");
        assert_eq!(b2, "success(변경 3)");
        // 마지막 메시지 없으면 verdict 폴백
        assert!(push_content("proj", "failed", None).0.starts_with("❌"));
    }

    #[test]
    fn build_messages_includes_deeplink_data() {
        let data = serde_json::json!({"hostname": "mac", "project": "p1", "path": "/x", "session": "s1"});
        let msgs = build_messages(&["ExponentPushToken[a]".into()], "✅ p1", "success", &data);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["to"], "ExponentPushToken[a]");
        assert_eq!(msgs[0]["data"]["project"], "p1");
        assert_eq!(msgs[0]["data"]["session"], "s1");
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
