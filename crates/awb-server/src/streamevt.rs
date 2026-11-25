// crates/awb-server/src/streamevt.rs (Task 3) — claude stream-json JSONL 한 줄을 Event로 파싱
use serde::Serialize;
use serde_json::Value;

const SUMMARY_MAX_CHARS: usize = 200;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Token { text: String },
    ToolUse { name: String, summary: String },
    /// session_id: 이 실행이 쓴 세션(fork 되면 새 ID) — 앱이 이후 전송·트랜스크립트를 이 ID 로 바꾼다
    Done { exit: Option<i32>, verdict: String, changed_files: u32, session_id: Option<String> },
    Error { message: String },
}

/// 문자(char) 경계를 지켜 최대 `max_chars`개 문자로 잘라 UTF-8 패닉을 방지한다.
/// (바이트 인덱스로 자르면 한글/이모지 등 멀티바이트 문자 중간에서 패닉 가능)
pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> (String, bool) {
    let mut it = s.chars();
    let truncated: String = it.by_ref().take(max_chars).collect();
    let truncated_flag = it.next().is_some();
    (truncated, truncated_flag)
}

/// 사용량 한도 상태 — claude 스트림의 rate_limit_event 그대로(남은 %는 스트림에 없다).
/// status: "allowed" | "allowed_warning" | "rejected"(차단). project 는 워처가 채운다.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct RateLimit {
    pub status: String,
    pub resets_at: Option<u64>,
    pub rate_limit_type: Option<String>,
    pub session_id: Option<String>,
    pub project: String,
}

/// 로그 텍스트에서 마지막 한도 정보 — CLI 는 바뀐 상태만 다시 내보내므로 마지막 것이 현재 상태다.
/// 차단 중에는 rate_limit_event 대신 assistant 응답의 api_error_params 에만 나올 수 있다(t3-code ClaudeAdapterV2.ts:5017).
pub fn last_rate_limit(text: &str) -> Option<RateLimit> {
    text.lines()
        .filter(|l| l.contains("\"rate_limit_event\"") || l.contains("\"api_error_params\""))
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            let i = match v.get("type")?.as_str()? {
                "rate_limit_event" => v.get("rate_limit_info")?,
                "assistant" => v.get("api_error_params")?.get("rate_limit_info")?,
                _ => return None,
            };
            Some(RateLimit {
                status: i.get("status")?.as_str()?.to_string(),
                resets_at: i.get("resetsAt").and_then(|x| x.as_u64()),
                rate_limit_type: i.get("rateLimitType").and_then(|x| x.as_str()).map(String::from),
                session_id: v.get("session_id").and_then(|x| x.as_str()).map(String::from),
                project: String::new(),
            })
        })
        .last()
}

/// 진행 알림에 싣는 현재 단계 — 마지막 tool_use 도구 이름과 마지막 TodoWrite 의 (완료, 전체).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub step: String,
    pub todo: Option<(usize, usize)>,
}

/// 로그 텍스트에서 현재 단계. parse_line 의 ToolUse 는 input 을 200자로 잘라 todos 를 못 읽으므로 따로 본다.
pub fn latest_progress(text: &str) -> Option<Progress> {
    let mut step = None;
    let mut todo = None;
    for l in text.lines().filter(|l| l.contains("\"tool_use\"")) {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) else { continue };
        for item in content.iter().filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("tool_use")) {
            let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("tool");
            if name == "TodoWrite" {
                if let Some(todos) = item.get("input").and_then(|i| i.get("todos")).and_then(|t| t.as_array()) {
                    let done = todos.iter().filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed")).count();
                    todo = Some((done, todos.len()));
                }
            }
            step = Some(name.to_string());
        }
    }
    step.map(|step| Progress { step, todo })
}

pub fn parse_line(line: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(line).ok()?;
    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => {
            let content = v.get("message")?.get("content")?.as_array()?;
            for item in content {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                            return Some(Event::Token { text: t.to_string() });
                        }
                    }
                    Some("tool_use") => {
                        let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("tool").to_string();
                        let raw_summary = item.get("input").map(|i| i.to_string()).unwrap_or_default();
                        let (mut summary, was_truncated) = truncate_chars(&raw_summary, SUMMARY_MAX_CHARS);
                        if was_truncated {
                            summary.push('…');
                        }
                        return Some(Event::ToolUse { name, summary });
                    }
                    _ => {}
                }
            }
            None
        }
        Some("result") => v.get("subtype").and_then(|s| s.as_str())
            .filter(|s| s.contains("error"))
            .map(|_| Event::Error { message: v.get("error").map(|e| e.to_string()).unwrap_or_else(|| "error".into()) }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_assistant_text_to_token() {
        // claude stream-json assistant 이벤트(간략화): content[].text
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"안녕"}]}}"#;
        match parse_line(line) {
            Some(Event::Token { text }) => assert_eq!(text, "안녕"),
            other => panic!("expected Token, got {other:?}"),
        }
    }
    #[test]
    fn parses_tool_use() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        match parse_line(line) {
            Some(Event::ToolUse { name, .. }) => assert_eq!(name, "Bash"),
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
    #[test]
    fn last_rate_limit_picks_last_event() {
        // 실측 줄(~/.claude/.awb-runs/1783505085-7679-0.log) 그대로 + 뒤에 차단 이벤트
        let real = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1783508400,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false},"uuid":"06880897-b148-4535-ab2d-7d64eb56fd8c","session_id":"0504bb6f-da3c-4c2d-a7ae-7eb686c54928"}"#;
        let first = last_rate_limit(real).unwrap();
        assert_eq!((first.status.as_str(), first.resets_at, first.rate_limit_type.as_deref()), ("allowed", Some(1783508400), Some("five_hour")));
        assert_eq!(first.session_id.as_deref(), Some("0504bb6f-da3c-4c2d-a7ae-7eb686c54928"));
        let blocked = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1783512000,"rateLimitType":"five_hour"},"session_id":"s2"}"#;
        let text = format!("{real}\n{{\"type\":\"assistant\",\"message\":{{}}}}\n{blocked}\n");
        let last = last_rate_limit(&text).unwrap();
        assert_eq!((last.status.as_str(), last.resets_at), ("rejected", Some(1783512000)));
    }
    #[test]
    fn last_rate_limit_includes_blocked_assistant_error() {
        // assistant 차단 응답 구조는 t3-code ClaudeAdapterV2.test.ts:3695-3716·9196 픽스처를 따른다
        let event = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1783508400,"rateLimitType":"five_hour"},"session_id":"s1"}"#;
        let blocked = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"limit"}]},"parent_tool_use_id":null,"error":"rate_limit","api_error_params":{"rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","resetsAt":1783512000}},"uuid":"u","session_id":"s1"}"#;
        let last = last_rate_limit(&format!("{event}\n{blocked}\n")).unwrap();
        assert_eq!((last.status.as_str(), last.resets_at, last.session_id.as_deref()), ("rejected", Some(1783512000), Some("s1")));
    }
    #[test]
    fn ignores_unparseable() { assert!(parse_line("garbage").is_none()); }

    #[test]
    fn truncates_multibyte_summary_without_panic() {
        // 한글 문자는 UTF-8로 3바이트 — 순진하게 &s[..200] 바이트 슬라이스를 쓰면
        // 문자 중간에서 잘려 패닉이 난다. 300자(900바이트) 한글 문자열로 검증.
        let long_korean = "안".repeat(300);
        let input = serde_json::json!({"command": long_korean});
        let line = serde_json::json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "name": "Bash", "input": input}]}
        }).to_string();
        match parse_line(&line) {
            Some(Event::ToolUse { name, summary }) => {
                assert_eq!(name, "Bash");
                // 잘렸다는 표시(줄임표)가 붙고, 문자 경계에서 잘려 유효 UTF-8이어야 한다(패닉 없이 여기 도달하면 이미 증명됨).
                assert!(summary.ends_with('…'));
                assert!(summary.chars().count() <= SUMMARY_MAX_CHARS + 1);
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
