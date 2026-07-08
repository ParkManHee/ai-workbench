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
pub struct SessionInfo { pub session_id: String, pub updated: u64, pub preview: String, pub count: u32, pub active: bool, pub waiting: bool }
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
    let mut lines: Vec<&str> = content.lines().collect();
    // 쓰기 중인(개행 미완) 마지막 줄은 제외 — next에 포함되면 완성된 뒤 건너뛰어 메시지가 유실된다.
    if !content.ends_with('\n') && !lines.is_empty() { lines.pop(); }
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

/// epoch초 → "YYYY-MM-DDTHH:MM:SS"(UTC) — 트랜스크립트 timestamp(ISO8601)와 사전순 비교용.
fn epoch_to_iso(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}")
}

/// 페이지 응답: 메시지들 + prev(더 이전 페이지 요청용 line idx, 없으면 None) + next(라이브 폴 offset) + active.
#[derive(Serialize)]
pub struct Page { pub messages: Vec<TranscriptMsg>, pub prev: Option<usize>, pub next: usize, pub active: bool }

/// 페이지 조회. until=Some(L)이면 L 이전 메시지 중 마지막 limit개("위로 스크롤"),
/// until=None이면 최근 tail_secs(기본 1시간) 메시지(최대 limit개, 없으면 마지막 20개 폴백).
pub fn read_transcript_page(path: &str, until: Option<usize>, limit: usize, tail_secs: u64) -> Page {
    let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return Page { messages: vec![], prev: None, next: 0, active: false } };
    let mut lines: Vec<&str> = content.lines().collect();
    // 쓰기 중인(개행 미완) 마지막 줄 제외 — read_transcript와 동일한 유실 방지
    if !content.ends_with('\n') && !lines.is_empty() { lines.pop(); }
    let total = lines.len();
    // (line_idx, timestamp, msg) 전체 파싱
    let mut all: Vec<(usize, String, TranscriptMsg)> = vec![];
    for (i, line) in lines.iter().enumerate() {
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        if let Some(r @ ("user" | "assistant")) = v.get("type").and_then(|t| t.as_str()) {
            if let Some(c) = v.get("message").and_then(|m| m.get("content")) {
                let (text, tools, tool_details) = parse_content(c);
                if !text.is_empty() || !tools.is_empty() {
                    let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    all.push((i, ts, TranscriptMsg { role: r.to_string(), text, tools, tool_details }));
                }
            }
        }
    }
    let active = fs::metadata(path).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| now().saturating_sub(d.as_secs()) <= 90).unwrap_or(false);
    let taken: Vec<&(usize, String, TranscriptMsg)> = match until {
        Some(u) => {
            let cands: Vec<&_> = all.iter().filter(|(i, _, _)| *i < u).collect();
            let start = cands.len().saturating_sub(limit);
            cands[start..].to_vec()
        }
        None => {
            let cutoff = epoch_to_iso(now().saturating_sub(tail_secs));
            let cands: Vec<&_> = all.iter().filter(|(_, ts, _)| ts.as_str() >= cutoff.as_str()).collect();
            let cands = if cands.is_empty() {
                let start = all.len().saturating_sub(20);
                all[start..].iter().collect()
            } else { cands };
            let start = cands.len().saturating_sub(limit);
            cands[start..].to_vec()
        }
    };
    let first_idx = taken.first().map(|(i, _, _)| *i);
    let prev = first_idx.filter(|fi| all.iter().any(|(i, _, _)| i < fi)).map(|fi| fi);
    Page { messages: taken.into_iter().map(|(_, _, m)| m.clone()).collect(), prev, next: total, active }
}

/// 마지막 메시지가 "사용자 답을 기다리는 질문"인가 — AskUserQuestion 툴 또는 '?'로 끝나는 assistant 텍스트.
fn is_waiting_msg(m: &TranscriptMsg) -> bool {
    m.role == "assistant"
        && (m.tools.iter().any(|t| t == "AskUserQuestion") || m.text.trim_end().ends_with('?'))
}

/// 파일 끝부분(최대 64KB)만 읽어 마지막 user/assistant 메시지를 파싱 — /projects 상태 판정용 저비용 경로.
fn last_msg_from_tail(path: &str) -> Option<TranscriptMsg> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(64 * 1024);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = s.lines().collect();
    if start > 0 && !lines.is_empty() { lines.remove(0); } // 잘린 첫 줄 버림
    for line in lines.iter().rev() {
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        if let Some(r @ ("user" | "assistant")) = v.get("type").and_then(|t| t.as_str()) {
            if let Some(c) = v.get("message").and_then(|m| m.get("content")) {
                let (text, tools, tool_details) = parse_content(c);
                if !text.is_empty() || !tools.is_empty() {
                    return Some(TranscriptMsg { role: r.to_string(), text, tools, tool_details });
                }
            }
        }
    }
    None
}

/// 프로젝트 상태: 최신 세션이 활발히 갱신 중이면 "working"(🟢), 아니고 마지막 메시지가 질문 대기면 "waiting"(🔴).
pub fn project_status(slug: &str) -> Option<String> {
    project_status_in(&format!("{}/{}", projects_root(), slug))
}
pub fn project_status_in(dir: &str) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    let mut latest: Option<(u64, std::path::PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let m = e.metadata().and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        if latest.as_ref().map(|(lm, _)| m > *lm).unwrap_or(true) { latest = Some((m, p)); }
    }
    let (mtime, path) = latest?;
    if now().saturating_sub(mtime) <= 90 { return Some("working".to_string()); }
    let last = last_msg_from_tail(path.to_str()?)?;
    if is_waiting_msg(&last) { Some("waiting".to_string()) } else { None }
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
        let waiting = !active && msgs.last().map(is_waiting_msg).unwrap_or(false);
        out.push(SessionInfo { session_id: sid, updated, preview, count: msgs.len() as u32, active, waiting });
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
    fn partial_last_line_not_counted_then_recovered() {
        let dir = std::env::temp_dir().join("awb_tx_partial"); std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.jsonl");
        // 완성 1줄 + 쓰기 중(개행 없음) 1줄
        std::fs::write(&f, concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"질문\"}}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"쓰는중")).unwrap();
        let (msgs, next, _) = read_transcript(f.to_str().unwrap(), 0);
        assert_eq!(msgs.len(), 1);
        assert_eq!(next, 1, "미완성 줄은 next에 포함되면 안 됨");
        // 줄 완성 후 next부터 다시 읽으면 유실 없이 잡혀야 함
        std::fs::write(&f, concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"질문\"}}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"쓰는중이던말\"}}\n")).unwrap();
        let (msgs2, next2, _) = read_transcript(f.to_str().unwrap(), next);
        assert_eq!(msgs2.len(), 1);
        assert_eq!(msgs2[0].text, "쓰는중이던말");
        assert_eq!(next2, 2);
    }
    #[test]
    fn epoch_to_iso_known_values() {
        assert_eq!(epoch_to_iso(0), "1970-01-01T00:00:00");
        assert_eq!(epoch_to_iso(86_400 + 3661), "1970-01-02T01:01:01");
    }
    #[test]
    fn page_tail_and_scrollback() {
        let dir = std::env::temp_dir().join("awb_tx_page"); std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("p1.jsonl");
        let recent = epoch_to_iso(now() - 10);
        std::fs::write(&f, format!(concat!(
            "{{\"type\":\"user\",\"timestamp\":\"2020-01-01T00:00:00.000Z\",\"message\":{{\"role\":\"user\",\"content\":\"옛날1\"}}}}\n",
            "{{\"type\":\"assistant\",\"timestamp\":\"2020-01-01T00:00:01.000Z\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"옛답1\"}}]}}}}\n",
            "{{\"type\":\"user\",\"timestamp\":\"{r}.000Z\",\"message\":{{\"role\":\"user\",\"content\":\"최근1\"}}}}\n",
            "{{\"type\":\"assistant\",\"timestamp\":\"{r}.100Z\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"최근답1\"}}]}}}}\n"
        ), r = recent)).unwrap();
        // tail(1시간): 최근 2개만, prev는 최근1의 라인(2) — 그 앞에 옛 메시지 존재
        let p = read_transcript_page(f.to_str().unwrap(), None, 100, 3600);
        assert_eq!(p.messages.len(), 2);
        assert_eq!(p.messages[0].text, "최근1");
        assert_eq!(p.prev, Some(2));
        assert_eq!(p.next, 4);
        // 위로 스크롤: until=2 이전 → 옛 2개, 더 이전 없음 → prev=None
        let older = read_transcript_page(f.to_str().unwrap(), Some(2), 50, 3600);
        assert_eq!(older.messages.len(), 2);
        assert_eq!(older.messages[0].text, "옛날1");
        assert_eq!(older.prev, None);
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
