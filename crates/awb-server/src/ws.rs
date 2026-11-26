// crates/awb-server/src/ws.rs (Task 5) — WS 스트리밍 /stream/:run_id?offset=N&token=<t>
// 재접속 시 offset부터 이어보기. 인증은 미들웨어가 업그레이드에 안 걸리므로 핸들러 내부에서 수동 검증.
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use crate::routes::AppState;

/// 버퍼에서 완결된(개행으로 끝난) 줄들만 뽑아내고, 마지막 미완결 줄은 버퍼에 남긴다.
/// 반환값: (완결된 줄들, 소비한 바이트 수 — 호출자가 `buf[consumed..]`로 잔여분을 유지)
pub fn split_new_lines(buf: &str) -> (Vec<String>, usize) {
    let mut lines = Vec::new();
    let mut consumed = 0;
    for line in buf.split_inclusive('\n') {
        if line.ends_with('\n') {
            lines.push(line.trim_end_matches('\n').to_string());
            consumed += line.len();
        }
    }
    (lines, consumed)
}

/// stream_loop 의 모든 return 경로에서 연결 수를 되돌리려고 Drop 에 맡긴다.
struct Attached(crate::runreg::RunRegistry, String);
impl Drop for Attached {
    fn drop(&mut self) {
        self.0.detach(&self.1);
    }
}

#[derive(serde::Deserialize)]
pub struct StreamQuery {
    #[serde(default)]
    pub offset: u64,
    pub token: Option<String>,
}

/// `GET /stream/:run_id?offset=N&token=<t>` — WS 업그레이드.
/// 무인증 라우트 그룹에 등록되므로(미들웨어가 업그레이드에 안 걸림) 여기서 직접 토큰을 검증한다.
/// 토큰은 로그에 남기지 않는다.
pub async fn stream_handler(
    ws: WebSocketUpgrade,
    State(st): State<AppState>,
    Path(run_id): Path<String>,
    Query(q): Query<StreamQuery>,
) -> Response {
    let authorized = q.token.as_deref().map(|t| st.devices.verify(t)).unwrap_or(false);
    if !authorized {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let meta = st.runs.get(&run_id);
    ws.on_upgrade(move |socket| stream_loop(socket, st, run_id, q.offset, meta))
}

async fn stream_loop(
    mut socket: WebSocket,
    st: AppState,
    run_id: String,
    mut offset: u64,
    meta: Option<crate::runreg::RunMeta>,
) {
    let meta = match meta {
        Some(m) => m,
        None => {
            let _ = socket
                .send(Message::Text("{\"kind\":\"error\",\"message\":\"unknown run\"}".into()))
                .await;
            let _ = socket.send(Message::Close(None)).await; // 정상 종료 프레임
            return;
        }
    };
    // shortcut: 끊긴 연결은 다음 send/ping(최대 20초) 때 감지돼 그동안 진행 푸시가 빠진다, 기기에서 공백이 보이면 올린다
    st.runs.attach(&run_id);
    let _attached = Attached(st.runs.clone(), run_id.clone());
    let mut pending = String::new();
    let mut session_id: Option<String> = None;
    let mut task_ids = Vec::new();
    // keepalive: 폰 절전/NAT가 유휴 연결을 끊지 않도록 주기적으로 ping 프레임 전송
    // (OkHttp/브라우저가 pong을 자동 응답하므로 클라이언트 코드는 무관여).
    const PING_EVERY_SECS: u64 = 20;
    let mut since_traffic = 0u64;
    loop {
        let chunk = awb_core::runlog::read_log(&meta.log, offset);
        offset = chunk.offset;
        if !chunk.text.is_empty() { since_traffic = 0; }
        if !chunk.text.is_empty() {
            pending.push_str(&chunk.text);
            let (lines, consumed) = split_new_lines(&pending);
            pending = pending[consumed..].to_string();
            for line in lines {
                if let Some(sid) = crate::sessions::parse_session_id(&line) {
                    st.sessions.record(&meta.project, &sid, meta.fork_of.as_deref());
                    crate::worktree::note(&st, &meta, &sid);
                    session_id = Some(sid);
                }
                for ev in crate::streamevt::parse_line(&line).into_iter().chain(crate::streamevt::task_todos_event(&line, &mut task_ids)) {
                    if socket
                        .send(Message::Text(serde_json::to_string(&ev).unwrap().into()))
                        .await
                        .is_err()
                    {
                        return; // 클라이언트가 끊음
                    }
                }
            }
        }
        if chunk.done {
            let done = finish(&st, &meta, &run_id, session_id.clone());
            let _ = socket
                .send(Message::Text(serde_json::to_string(&done).unwrap().into()))
                .await;
            let _ = socket.send(Message::Close(None)).await; // 정상 종료 프레임(클라이언트 측 abnormal-closure 방지)
            break;
        }
        // 소켓을 읽지 않으면 클라이언트의 close 를 다음 send(최대 20초 ping)에야 알아 그동안 푸시가 억제된다
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(1000)) => {}
            msg = socket.recv() => match msg {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            },
        }
        since_traffic += 1;
        if since_traffic >= PING_EVERY_SECS {
            since_traffic = 0;
            if socket.send(Message::Ping(vec![].into())).await.is_err() {
                return; // 상대가 죽은 연결 — 정리
            }
        }
    }
}

/// done 이벤트를 만들기 전에 끝내야 할 상태 반영 — 반환값을 보낸 직후 앱이 /info 를 다시 읽으므로
/// 한도 저장은 여기서 해야 한다(1.5초 주기 워처에 맡기면 앱 조회가 먼저 와 배지가 안 뜬다).
pub(crate) fn finish(st: &AppState, meta: &crate::runreg::RunMeta, run_id: &str, session_id: Option<String>) -> crate::streamevt::Event {
    let status = awb_core::runlog::run_status(&meta.log, &meta.workdir); // 락 해제 포함
    crate::push::record_rate_limit(st, &meta.project, &awb_core::runlog::read_log(&meta.log, 0).text);
    // 순서 중요: socket.send(...).await 지점에서 태스크가 잠깐 양보하는 동안
    // 워처(push.rs)가 should_push(=mark_notified)를 먼저 통과해 이중 발송할 수 있다.
    // send 이전에 게이트를 닫아(mark_notified) 워처가 끼어들 여지를 없앤다.
    st.runs.mark_notified(run_id); // WS로 전달 예정 → 이후 푸시 스킵(게이트를 먼저 닫음)
    crate::streamevt::Event::Done {
        exit: status.exit_code,
        verdict: status.verdict,
        changed_files: status.changed_files,
        session_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn splits_only_complete_lines() {
        let (lines, consumed) = split_new_lines("a\nb\npartial");
        assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(consumed, 4); // "a\nb\n"
    }

    #[test]
    fn no_trailing_newline_yields_nothing_consumed() {
        let (lines, consumed) = split_new_lines("no newline here");
        assert!(lines.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn empty_buf_yields_nothing() {
        let (lines, consumed) = split_new_lines("");
        assert!(lines.is_empty());
        assert_eq!(consumed, 0);
    }
}
