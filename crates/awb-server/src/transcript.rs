use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn project_slug(workdir: &str) -> String {
    workdir.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}
pub fn safe_session_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }
fn home() -> String { std::env::var("HOME").unwrap_or_default() }
fn projects_root() -> String { format!("{}/.claude/projects", home()) }

#[derive(Serialize, Clone)]
pub struct SessionInfo { pub session_id: String, pub updated: u64, pub preview: String, pub count: u32, pub active: bool }
#[derive(Serialize, Clone)]
pub struct TranscriptMsg { pub role: String, pub text: String, pub tools: Vec<String>, pub tool_details: Vec<String> }

/// UTF-8 안전 절단(문자 기준) — 바이트 슬라이스는 멀티바이트 경계에서 패닉.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars { return s.to_string(); }
    let t: String = s.chars().take(max_chars).collect();
    format!("{t}…")
}

fn parse_content(v: &Value) -> (String, Vec<String>, Vec<String>) {
    match v {
        Value::String(s) => (s.clone(), vec![], vec![]),
        Value::Array(arr) => {
            let mut text = String::new(); let mut tools = vec![]; let mut details = vec![];
            for it in arr {
                match it.get("type").and_then(|t| t.as_str()) {
                    Some("text") => if let Some(t) = it.get("text").and_then(|x| x.as_str()) { text.push_str(t); },
                    Some("tool_use") => if let Some(n) = it.get("name").and_then(|x| x.as_str()) {
                        tools.push(n.to_string());
                        // 상세(입력 요약): 앱에서 "작업" 버튼 펼침 시 표시
                        let d = it.get("input").map(|i| truncate_chars(&i.to_string(), 300)).unwrap_or_default();
                        details.push(if d.is_empty() { n.to_string() } else { format!("{n}: {d}") });
                    },
                    _ => {}
                }
            }
            (text, tools, details)
        }
        _ => (String::new(), vec![], vec![]),
    }
}

pub fn read_transcript(path: &str, from_line: usize) -> (Vec<TranscriptMsg>, usize, bool) {
    let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return (vec![], from_line, false) };
    let lines: Vec<&str> = content.lines().collect();
    let mut msgs = vec![];
    for line in lines.iter().skip(from_line) {
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        match v.get("type").and_then(|t| t.as_str()) {
            Some(r @ ("user" | "assistant")) => {
                if let Some(c) = v.get("message").and_then(|m| m.get("content")) {
                    let (text, tools, tool_details) = parse_content(c);
                    if !text.is_empty() || !tools.is_empty() {
                        msgs.push(TranscriptMsg { role: r.to_string(), text, tools, tool_details });
                    }
                }
            }
            _ => {}
        }
    }
    let active = fs::metadata(path).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| now().saturating_sub(d.as_secs()) <= 90).unwrap_or(false);
    (msgs, lines.len(), active)
}

pub fn list_sessions(slug: &str) -> Vec<SessionInfo> {
    let dir = format!("{}/{}", projects_root(), slug);
    let mut out = vec![];
    let entries = match fs::read_dir(&dir) { Ok(e) => e, Err(_) => return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let sid = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
        if !safe_session_id(&sid) { continue; }
        let updated = e.metadata().and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        let (msgs, _n, active) = read_transcript(p.to_str().unwrap_or(""), 0);
        let preview = msgs.iter().find(|m| m.role == "user").map(|m| {
            let t: String = m.text.chars().take(60).collect(); t
        }).unwrap_or_default();
        out.push(SessionInfo { session_id: sid, updated, preview, count: msgs.len() as u32, active });
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slug_matches_claude_convention() {
        assert_eq!(project_slug("/Users/mh/github/ai-workbench"), "-Users-mh-github-ai-workbench");
        assert_eq!(project_slug("/a/b.c_d"), "-a-b-c-d");
    }
    #[test]
    fn safe_session_id_rejects_traversal() {
        assert!(safe_session_id("0504bb6f-da3c-4c2d"));
        assert!(!safe_session_id("../etc/passwd"));
        assert!(!safe_session_id("a/b"));
    }
    #[test]
    fn parses_user_and_assistant_lines() {
        let dir = std::env::temp_dir().join("awb_tx"); std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s1.jsonl");
        std::fs::write(&f, concat!(
            "{\"type\":\"mode\",\"sessionId\":\"s1\"}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"안녕\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"반가워\"},{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\n"
        )).unwrap();
        let (msgs, next, _active) = read_transcript(f.to_str().unwrap(), 0);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user"); assert_eq!(msgs[0].text, "안녕");
        assert_eq!(msgs[1].role, "assistant"); assert_eq!(msgs[1].text, "반가워");
        assert_eq!(msgs[1].tools, vec!["Bash".to_string()]);
        assert_eq!(msgs[1].tool_details, vec!["Bash".to_string()]); // input 없으면 이름만
        assert_eq!(next, 3); // 3 lines consumed
    }
}
