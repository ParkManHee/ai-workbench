//! 터미널(대화형 claude)에서 지금 열려 있는 세션 판정 — Claude Code 가 남기는 `~/.claude/sessions/<pid>.json` 레지스트리.
//! 내부 형식이라 바뀌면 빈 집합이 된다: 파일은 있는데 하나도 못 읽으면 경고를 남겨 조용히 틀리지 않게 한다.
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

/// 레지스트리 한 항목. cwd 는 없을 수도 있다(형식 변경 대비).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveEntry { pub pid: u32, pub session_id: String, pub proc_start: String, pub cwd: Option<String> }

/// 레지스트리 한 파일 → 항목. 대화형(kind=interactive)만.
pub fn parse_entry(json: &str) -> Option<LiveEntry> {
    let v: Value = serde_json::from_str(json).ok()?;
    if v.get("kind")?.as_str()? != "interactive" { return None; }
    // 데몬이 띄운 claude -p 도 kind=interactive 로 등록된다(2026-11-26 실측) — entrypoint 가 sdk-cli 면 터미널이 아니다
    if v.get("entrypoint").and_then(|e| e.as_str()) == Some("sdk-cli") { return None; }
    let pid = u32::try_from(v.get("pid")?.as_u64()?).ok()?;
    Some(LiveEntry {
        pid,
        session_id: v.get("sessionId")?.as_str()?.to_string(),
        proc_start: v.get("procStart")?.as_str()?.to_string(),
        cwd: v.get("cwd").and_then(|c| c.as_str()).map(str::to_string),
    })
}

/// pid 가 살아 있고 시작시각이 레지스트리와 같을 때만 live — 죽은 프로세스의 남은 파일·pid 재사용을 거른다.
/// `want` 로 먼저 거르는 건 조회 하나에 항목마다 ps 를 띄우지 않기 위해서다.
fn live_entries_in(dir: &str, lstart: impl Fn(u32) -> Option<String>, want: impl Fn(&LiveEntry) -> bool) -> Vec<LiveEntry> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let (mut files, mut parsed) = (0, 0);
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        files += 1;
        let Some(ent) = std::fs::read_to_string(&p).ok().as_deref().and_then(parse_entry) else { continue };
        parsed += 1;
        if want(&ent) && lstart(ent.pid).as_deref() == Some(ent.proc_start.as_str()) { out.push(ent); }
    }
    if files > 0 && parsed == 0 {
        eprintln!("live: {dir} 에 레지스트리 {files}개가 있지만 대화형 항목을 하나도 못 읽음 — 형식 변경 의심(터미널 세션 판정 꺼짐)");
    }
    out
}

pub fn live_ids_in(dir: &str, lstart: impl Fn(u32) -> Option<String>) -> HashSet<String> {
    live_entries_in(dir, lstart, |_| true).into_iter().map(|e| e.session_id).collect()
}

/// session_id 의 살아 있는 대화형 항목. 판정은 live_ids_in 과 같은 경로다.
pub fn find_live_in(dir: &str, session_id: &str, lstart: impl Fn(u32) -> Option<String>) -> Option<LiveEntry> {
    live_entries_in(dir, lstart, |e| e.session_id == session_id).into_iter().next()
}

/// procStart 는 UTC 의 `ps -o lstart` 형식("Wed Sep 23 03:09:45 2026")과 같다(2026-09-23 실측).
pub fn proc_lstart(pid: u32) -> Option<String> {
    let out = Command::new("ps").env("TZ", "UTC").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn sessions_dir() -> String { format!("{}/.claude/sessions", std::env::var("HOME").unwrap_or_default()) }

pub fn live_interactive_ids() -> HashSet<String> {
    live_ids_in(&sessions_dir(), proc_lstart)
}

pub fn find_live(session_id: &str) -> Option<LiveEntry> {
    find_live_in(&sessions_dir(), session_id, proc_lstart)
}

/// 살아 있는 앱 실행(데몬이 띄운 claude -p)의 세션 id. 항목은 실행 0.5초 안에 생겨 트랜스크립트보다 먼저고(--resume 이면 그 id),
/// 정상 종료·SIGTERM 엔 지워지지만 SIGKILL 엔 남아서 시작시각 대조로 거른다(2026-11-26 실측).
pub fn live_app_ids_in(dir: &str, lstart: impl Fn(u32) -> Option<String>) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashSet::new() };
    entries.flatten().filter_map(|e| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
        if v.get("entrypoint")?.as_str()? != "sdk-cli" { return None; }
        let pid = u32::try_from(v.get("pid")?.as_u64()?).ok()?;
        if lstart(pid).as_deref() != Some(v.get("procStart")?.as_str()?) { return None; }
        v.get("sessionId")?.as_str().map(str::to_string)
    }).collect()
}

pub fn live_app_ids() -> HashSet<String> {
    live_app_ids_in(&sessions_dir(), proc_lstart)
}

/// 레지스트리 항목과 그 파일의 수정시각(초) — 읽기만 하고 ps 는 띄우지 않는다. 살아 있는지는 live_interactive_ids 로 거른다.
pub fn registry() -> Vec<(LiveEntry, u64)> {
    let Ok(entries) = std::fs::read_dir(sessions_dir()) else { return vec![] };
    entries.flatten().filter_map(|e| {
        let ent = parse_entry(&std::fs::read_to_string(e.path()).ok()?)?;
        let t = e.metadata().and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        Some((ent, t))
    }).collect()
}

/// 레지스트리 cwd 와 프로젝트 경로 비교 — `~`·끝 슬래시·심링크(프로젝트 경로는 realpath) 차이를 흡수한다.
pub fn same_dir(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let p = crate::paths::expand_tilde(p);
        let p = if p.len() > 1 { p.trim_end_matches('/').to_string() } else { p };
        std::fs::canonicalize(&p).map(|x| x.to_string_lossy().to_string()).unwrap_or(p)
    };
    norm(a) == norm(b)
}

fn alive(pid: u32) -> bool {
    // EPERM 은 존재하지만 권한 없음 — 살아 있는 것으로 본다
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

fn wait_exit(pid: u32, limit: Duration) -> bool {
    let step = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    loop {
        if !alive(pid) { return true; }
        if waited >= limit { return false; }
        std::thread::sleep(step);
        waited += step;
    }
}

/// 대화형 claude 는 SIGTERM 에 정리 후 바로 끝나므로 짧은 유예(3초) 뒤 SIGKILL, SIGKILL 은 즉시라 1초만 더 본다.
/// 앱 요청이 오래 걸리지 않게 총 4초 남짓으로 묶는다.
const TERM_GRACE: Duration = Duration::from_secs(3);
const KILL_GRACE: Duration = Duration::from_secs(1);

/// pid 를 종료한다(블로킹 — 비동기 쪽은 spawn_blocking 으로 부른다).
/// 신호마다 직전에 시작시각을 다시 대조한다: pid 재사용으로 남의 프로세스를 죽이지 않기 위해서다.
/// None = 대조 실패로 신호를 하나도 안 보냄, Some(ended) = SIGTERM 을 보냈고 제한 안에 끝났는지.
pub fn terminate(pid: u32, proc_start: &str, lstart: impl Fn(u32) -> Option<String>) -> Option<bool> {
    let same = || lstart(pid).as_deref() == Some(proc_start);
    if pid <= 1 || !same() { return None; }
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
    if wait_exit(pid, TERM_GRACE) { return Some(true); }
    if !same() { return Some(true); } // 사이에 끝나고 pid 가 재사용됐다 — 그 프로세스는 건드리지 않는다
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL); }
    Some(wait_exit(pid, KILL_GRACE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_dir(name: &str, files: &[(&str, &str)]) -> String {
        let d = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (f, body) in files { std::fs::write(d.join(f), body).unwrap(); }
        d.to_string_lossy().to_string()
    }

    const A: &str = r#"{"pid":111,"sessionId":"sid-a","cwd":"/p/a","procStart":"Wed Oct  7 10:00:00 2026","kind":"interactive"}"#;
    const B: &str = r#"{"pid":222,"sessionId":"sid-b","cwd":"/p/b","procStart":"Wed Oct  7 11:00:00 2026","kind":"interactive"}"#;

    // 222 는 pid 재사용: 살아 있지만 시작시각이 레지스트리와 다르다
    fn fake_lstart(pid: u32) -> Option<String> {
        match pid { 111 => Some("Wed Oct  7 10:00:00 2026".into()), 222 => Some("Thu Oct  8 09:00:00 2026".into()), _ => None }
    }

    #[test]
    fn find_live_rejects_proc_start_mismatch() {
        let d = reg_dir("awb_live_find_mismatch", &[("111.json", A), ("222.json", B)]);
        let a = find_live_in(&d, "sid-a", fake_lstart).unwrap();
        assert_eq!((a.pid, a.cwd.as_deref()), (111, Some("/p/a")));
        assert_eq!(find_live_in(&d, "sid-b", fake_lstart), None);
        assert_eq!(live_ids_in(&d, fake_lstart), HashSet::from(["sid-a".to_string()]));
    }

    // 실측한 레지스트리(claude 2.1.332): 터미널은 entrypoint=cli, 데몬이 띄운 claude -p 는 sdk-cli
    const TERM: &str = r#"{"pid":111,"sessionId":"sid-a","cwd":"/p/a","procStart":"Wed Oct  7 10:00:00 2026","kind":"interactive","entrypoint":"cli","status":"idle"}"#;
    const DAEMON: &str = r#"{"pid":222,"sessionId":"sid-b","cwd":"/p/b","procStart":"Wed Oct  7 11:00:00 2026","kind":"interactive","entrypoint":"sdk-cli","status":"busy"}"#;

    #[test]
    fn daemon_run_registry_entry_is_not_live_but_terminal_is() {
        assert_eq!(parse_entry(DAEMON), None);
        assert_eq!(parse_entry(TERM).map(|e| e.pid), Some(111));
        // entrypoint 가 없는 항목(옛 형식)은 지금처럼 터미널로 본다
        assert_eq!(parse_entry(A).map(|e| e.pid), Some(111));
    }

    #[test]
    fn find_live_none_for_unregistered_session() {
        let d = reg_dir("awb_live_find_unreg", &[("111.json", A)]);
        assert_eq!(find_live_in(&d, "sid-zzz", fake_lstart), None);
    }

    #[test]
    fn same_dir_ignores_trailing_slash_and_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert!(same_dir("~/", &home));
        assert!(same_dir("/nonexistent/x/", "/nonexistent/x"));
        assert!(!same_dir("/nonexistent/x", "/nonexistent/y"));
    }

    fn spawn_sleep() -> (u32, std::thread::JoinHandle<()>) {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        // 회수해 주지 않으면 좀비로 남아 kill(pid,0) 이 계속 성공한다
        let h = std::thread::spawn(move || { let _ = child.wait(); });
        (pid, h)
    }

    #[test]
    fn terminate_ends_matching_process() {
        let (pid, h) = spawn_sleep();
        let start = proc_lstart(pid).unwrap();
        assert_eq!(terminate(pid, &start, proc_lstart), Some(true));
        h.join().unwrap();
    }

    #[test]
    fn terminate_sends_nothing_on_proc_start_mismatch() {
        let (pid, h) = spawn_sleep();
        assert_eq!(terminate(pid, "Mon Jan  1 00:00:00 2001", proc_lstart), None);
        assert!(alive(pid), "시작시각이 다른 pid 에 신호를 보냈다");
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL); }
        h.join().unwrap();
    }
}
