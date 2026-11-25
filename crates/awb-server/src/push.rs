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
        if awb_core::transcript::is_question(m) {
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
    post(&build_messages(tokens, title, body, data));
}

/// title/body 없는 data-only 푸시 — 안드로이드에서 FCM data 메시지로 가야 앱 백그라운드 태스크가 깨어난다.
/// high: Doze 중에도 지연 없이 배달되게.
pub fn send_data(tokens: &[String], data: &serde_json::Value) {
    if tokens.is_empty() {
        return;
    }
    let msgs: Vec<_> = tokens.iter().map(|t| serde_json::json!({"to": t, "data": data, "priority": "high"})).collect();
    post(&msgs);
}

/// 진행 푸시 최소 간격 — 단계마다 보내면 FCM·알림 갱신이 과해 마지막 단계만 묶어 보낸다.
const PROGRESS_MIN_GAP: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
pub struct ProgressThrottle {
    seen: Option<crate::streamevt::Progress>,
    pending: Option<crate::streamevt::Progress>,
    last_sent: Option<std::time::Instant>,
}

impl ProgressThrottle {
    /// 단계가 바뀌면 보류에 올리고, 마지막 전송 후 PROGRESS_MIN_GAP 이 지났으면 보류분(마지막 것)을 내준다.
    pub fn tick(&mut self, latest: Option<crate::streamevt::Progress>, now: std::time::Instant) -> Option<crate::streamevt::Progress> {
        if latest.is_some() && latest != self.seen {
            self.pending = latest.clone();
            self.seen = latest;
        }
        let due = self.last_sent.is_none_or(|t| now.duration_since(t) >= PROGRESS_MIN_GAP);
        if self.pending.is_some() && due {
            self.last_sent = Some(now);
            return self.pending.take();
        }
        None
    }
    pub fn sent_any(&self) -> bool {
        self.last_sent.is_some()
    }
}

/// 폰이 WS 로 붙어 있으면 보내지 않는다. 그동안 tick 을 건너뛰어, 끊긴 뒤 첫 틱에 최신 단계가 나간다.
pub fn progress_to_send(
    reg: &crate::runreg::RunRegistry,
    run_id: &str,
    th: &mut ProgressThrottle,
    latest: Option<crate::streamevt::Progress>,
    now: std::time::Instant,
) -> Option<crate::streamevt::Progress> {
    if reg.is_attached(run_id) {
        return None;
    }
    th.tick(latest, now)
}

fn post(msgs: &[serde_json::Value]) {
    let payload = serde_json::to_string(msgs).unwrap_or_default();
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

/// run 로그 전체에서 마지막 한도 이벤트를 /info 상태에 반영한다 — 워처와 WS done 경로 공용
pub fn record_rate_limit(st: &crate::routes::AppState, project: &str, log_text: &str) {
    if let Some(mut rl) = crate::streamevt::last_rate_limit(log_text) {
        rl.project = project.to_string();
        *st.rate_limit.lock().unwrap() = Some(rl);
    }
}

/// run 완료를 폴링하다 완료되면 run_status로 락을 해제하고, notified 게이트를 통과할 때만 푸시를 보낸다.
/// 순서: done 감지 → run_status(락 해제, WS 미접속으로 인한 락 잔류 방지) → should_push 게이트 → (필요시) send.
/// hung run 상한(초). claude가 멈춰도 락이 무기한 잡히지 않게 워처가 강제 종료한다.
fn run_timeout_secs() -> u64 {
    std::env::var("AWB_RUN_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(3600)
}

pub fn spawn_watch(st: crate::routes::AppState, run_id: String) {
    tokio::spawn(async move {
        let meta = match st.runs.get(&run_id) {
            Some(m) => m,
            None => return,
        };
        let timeout = run_timeout_secs();
        let started = std::time::Instant::now();
        let mut progress = ProgressThrottle::default();
        loop {
            let chunk = awb_core::runlog::read_log(&meta.log, 0);
            record_rate_limit(&st, &meta.project, &chunk.text);
            if !chunk.done {
                let latest = crate::streamevt::latest_progress(&chunk.text);
                if let Some(p) = progress_to_send(&st.runs, &run_id, &mut progress, latest, std::time::Instant::now()) {
                    // 탭 시 완료 푸시와 같은 딥링크(hostname/path/session)로 열리게 같은 키를 싣는다
                    let data = serde_json::json!({
                        "kind": "progress",
                        "project": meta.project,
                        "run_id": run_id,
                        "step": p.step,
                        "todo": p.todo.map(|(done, total)| serde_json::json!({"done": done, "total": total})),
                        "hostname": crate::routes::resolve_hostname(),
                        "path": meta.workdir,
                        "session": st.sessions.get(&meta.project),
                        "branch": meta.worktree.as_ref().map(|w| w.branch.clone()),
                    });
                    send_data(&st.push.list(), &data);
                }
            }
            if !chunk.done && started.elapsed().as_secs() >= timeout {
                // hung run: 그룹 종료 + 타임아웃 코드 기록 → 다음 틱에 done 경로(락 해제·푸시)로 수렴
                eprintln!("run {run_id} 타임아웃({timeout}s) — 프로세스 그룹 강제 종료");
                awb_core::runner::cancel_run(meta.pgid, &meta.workdir);
                awb_core::runlog::mark_done_if_absent(&meta.log, 124);
            }
            if chunk.done {
                let status = awb_core::runlog::run_status(&meta.log, &meta.workdir);
                // WS가 붙어있지 않은 push-only 완료 경로에서도 --resume용 session_id를 캡처한다.
                // (WS 루프는 라인 단위로 파싱하지만, 여기선 WS가 없었을 수 있으므로 로그 전체를 읽어 찾는다.)
                if let Some(sid) = crate::sessions::capture_session_id_from_log(&meta.log) {
                    st.sessions.record(&meta.project, &sid, meta.fork_of.as_deref());
                    crate::worktree::note(&st, &meta, &sid);
                } else if let Some(wt) = &meta.worktree {
                    // 세션을 잡기 전에 끝난 격리 실행(시작 실패)은 끝내기·보관할 세션이 없다 — 빈 트리를 여기서 정리한다
                    eprintln!("격리 실행 {run_id}: 세션 없이 종료 — 트리 정리 {}", crate::worktree::cleanup_json(&st, wt));
                }
                // shortcut: 데몬이 run 도중 재시작되면 progress_end 가 안 가 고정 알림이 남는다(탭하면 지워짐), 기기에서 겪으면 앱 시작 시 정리로 올린다
                if progress.sent_any() {
                    // 앱은 identifier=progress-<project> 로 지우므로 project 가 필요하다
                    send_data(&st.push.list(), &serde_json::json!({"kind": "progress_end", "project": meta.project, "run_id": run_id}));
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
                        "branch": meta.worktree.as_ref().map(|w| w.branch.clone()),
                    });
                    send(&tokens, &title, &body, &data);
                }
                st.runs.remove(&run_id); // 완료된 run은 레지스트리에서 제거(취소/완료 후 무한 누적 방지)
                // 대기 중인 후속 턴이 있으면 이어서 실행(락이 방금 해제됨)
                crate::queue::spawn_drain(st.clone(), meta.project.clone(), meta.worktree.as_ref().map_or_else(|| meta.workdir.clone(), |w| w.repo.clone()));
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
            RunMeta { log: "l".into(), pgid: 1, workdir: "w".into(), project: "p".into(), notified: false, fork_of: None, worktree: None },
        );
        // WS가 먼저 전달한 경우
        assert!(r.mark_notified("x")); // WS가 소비
        assert!(!should_push(&r, "x")); // 워처는 스킵

        // WS가 없던 경우
        r.insert(
            "y",
            RunMeta { log: "l".into(), pgid: 1, workdir: "w".into(), project: "p".into(), notified: false, fork_of: None, worktree: None },
        );
        assert!(should_push(&r, "y")); // 워처가 발송
    }

    fn step(s: &str) -> Option<crate::streamevt::Progress> {
        Some(crate::streamevt::Progress { step: s.into(), todo: None })
    }

    #[test]
    fn progress_throttle_holds_14s_and_sends_last_at_15s() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut th = ProgressThrottle::default();
        assert_eq!(th.tick(step("Read"), t0), step("Read")); // 첫 단계는 바로
        assert_eq!(th.tick(step("Bash"), t0 + Duration::from_secs(10)), None);
        assert_eq!(th.tick(step("Edit"), t0 + Duration::from_secs(14)), None); // 14초: 보류
        assert_eq!(th.tick(step("Edit"), t0 + Duration::from_secs(15)), step("Edit")); // 15초: 마지막 것만
        assert_eq!(th.tick(step("Edit"), t0 + Duration::from_secs(40)), None); // 안 바뀌면 다시 안 보냄
    }

    #[test]
    fn progress_skipped_while_ws_attached_then_sent_after_detach() {
        let r = RunRegistry::new();
        let mut th = ProgressThrottle::default();
        let now = std::time::Instant::now();
        r.attach("x");
        assert_eq!(progress_to_send(&r, "x", &mut th, step("Bash"), now), None);
        r.detach("x");
        assert_eq!(progress_to_send(&r, "x", &mut th, step("Bash"), now), step("Bash"));
    }

    #[test]
    fn push_content_distinguishes_question_from_done() {
        use awb_core::transcript::TranscriptMsg;
        let q = TranscriptMsg { role: "assistant".into(), text: "어느 방식으로 진행할까요?".into(), tools: vec![], tool_details: vec![], options: vec![], todos: vec![] };
        let (t, b) = push_content("proj", "success", Some(&q));
        assert!(t.contains("질문 대기"), "{t}");
        assert!(b.contains("어느 방식"));
        // 선택지가 있으면 '?'로 안 끝나도 질문
        let opt = TranscriptMsg { role: "assistant".into(), text: "방식을 골라주세요".into(), tools: vec![], tool_details: vec![], options: vec!["A".into()], todos: vec![] };
        assert!(push_content("proj", "success", Some(&opt)).0.contains("질문 대기"));
        // 평서문 완료는 verdict 푸시
        let done = TranscriptMsg { role: "assistant".into(), text: "완료했습니다.".into(), tools: vec![], tool_details: vec![], options: vec![], todos: vec![] };
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
