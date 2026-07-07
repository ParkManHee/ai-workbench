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
    let mut pending = String::new();
    loop {
        let chunk = awb_core::runlog::read_log(&meta.log, offset);
        offset = chunk.offset;
        if !chunk.text.is_empty() {
            pending.push_str(&chunk.text);
            let (lines, consumed) = split_new_lines(&pending);
            pending = pending[consumed..].to_string();
            for line in lines {
                if let Some(sid) = crate::sessions::parse_session_id(&line) {
                    st.sessions.set(&meta.project, &sid);
                }
                if let Some(ev) = crate::streamevt::parse_line(&line) {
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
            let status = awb_core::runlog::run_status(&meta.log, &meta.workdir); // 락 해제 포함
            let done = crate::streamevt::Event::Done {
                exit: status.exit_code,
                verdict: status.verdict.clone(),
                changed_files: status.changed_files,
            };
            let _ = socket
                .send(Message::Text(serde_json::to_string(&done).unwrap().into()))
                .await;
            st.runs.mark_notified(&run_id); // WS로 전달됨 → 이후 푸시 스킵
            let _ = socket.send(Message::Close(None)).await; // 정상 종료 프레임(클라이언트 측 abnormal-closure 방지)
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
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
