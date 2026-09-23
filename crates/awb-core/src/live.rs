//! 터미널(대화형 claude)에서 지금 열려 있는 세션 판정 — Claude Code 가 남기는 `~/.claude/sessions/<pid>.json` 레지스트리.
//! 내부 형식이라 바뀌면 빈 집합이 된다: 파일은 있는데 하나도 못 읽으면 경고를 남겨 조용히 틀리지 않게 한다.
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;

/// 레지스트리 한 파일 → (pid, session_id, procStart). 대화형(kind=interactive)만.
pub fn parse_entry(json: &str) -> Option<(u32, String, String)> {
    let v: Value = serde_json::from_str(json).ok()?;
    if v.get("kind")?.as_str()? != "interactive" { return None; }
    let pid = u32::try_from(v.get("pid")?.as_u64()?).ok()?;
    Some((pid, v.get("sessionId")?.as_str()?.to_string(), v.get("procStart")?.as_str()?.to_string()))
}

/// pid 가 살아 있고 시작시각이 레지스트리와 같을 때만 live — 죽은 프로세스의 남은 파일·pid 재사용을 거른다.
pub fn live_ids_in(dir: &str, lstart: impl Fn(u32) -> Option<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let (mut files, mut parsed) = (0, 0);
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        files += 1;
        let Some((pid, sid, start)) = std::fs::read_to_string(&p).ok().as_deref().and_then(parse_entry) else { continue };
        parsed += 1;
        if lstart(pid).as_deref() == Some(start.as_str()) { out.insert(sid); }
    }
    if files > 0 && parsed == 0 {
        eprintln!("live: {dir} 에 레지스트리 {files}개가 있지만 대화형 항목을 하나도 못 읽음 — 형식 변경 의심(터미널 세션 판정 꺼짐)");
    }
    out
}

/// procStart 는 UTC 의 `ps -o lstart` 형식("Wed Sep 23 03:09:45 2026")과 같다(2026-09-23 실측).
fn proc_lstart(pid: u32) -> Option<String> {
    let out = Command::new("ps").env("TZ", "UTC").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

pub fn live_interactive_ids() -> HashSet<String> {
    live_ids_in(&format!("{}/.claude/sessions", std::env::var("HOME").unwrap_or_default()), proc_lstart)
}
