// crates/awb-server/src/streamevt.rs (Task 3) — claude stream-json JSONL 한 줄을 Event로 파싱
use serde::Serialize;
use serde_json::Value;

const SUMMARY_MAX_CHARS: usize = 200;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Token { text: String },
    ToolUse { name: String, summary: String },
    Done { exit: Option<i32>, verdict: String, changed_files: u32 },
    Error { message: String },
}

/// 문자(char) 경계를 지켜 최대 `max_chars`개 문자로 잘라 UTF-8 패닉을 방지한다.
/// (바이트 인덱스로 자르면 한글/이모지 등 멀티바이트 문자 중간에서 패닉 가능)
fn truncate_chars(s: &str, max_chars: usize) -> (String, bool) {
    let mut it = s.chars();
    let truncated: String = it.by_ref().take(max_chars).collect();
    let truncated_flag = it.next().is_some();
    (truncated, truncated_flag)
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
