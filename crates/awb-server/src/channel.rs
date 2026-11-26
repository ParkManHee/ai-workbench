//! 터미널 claude 에 붙은 폰 채널(plugins/phone/server.cjs)로 메시지 보내기.
//! 우편함: `~/.claude/awb-channels/<claude pid>/in/*.json` — 채널 서버가 읽고 unlink 한 뒤 세션에 주입한다.
//! 전달 확정은 「먼저 unlink 한 쪽이 갖는다」: 시한 안에 파일이 사라지면 전달, 아니면 데몬이 지워 회수한다.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub fn base_dir() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude/awb-channels") }

/// 채널 서버가 남긴 server.pid 의 프로세스가 살아 있어야 붙은 것 — claude 만 살고 서버가 죽은 경우를 거른다
pub fn attached_in(base: &Path, claude_pid: u32, alive: impl Fn(u32) -> bool) -> bool {
    std::fs::read_to_string(base.join(claude_pid.to_string()).join("server.pid")).ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .is_some_and(alive)
}

pub fn pid_alive(pid: u32) -> bool {
    // kill(pid, 0): 신호 없이 존재·권한만 본다
    i32::try_from(pid).is_ok_and(|p| p > 1 && unsafe { libc::kill(p, 0) } == 0)
}

pub fn attached(claude_pid: u32) -> bool { attached_in(&base_dir(), claude_pid, pid_alive) }

/// 채널 서버 폴링 주기(500ms)의 여러 배 — 그 안에 안 가져가면 서버가 멈춘 것으로 본다
pub const DELIVER_TIMEOUT: Duration = Duration::from_secs(3);

pub fn send_in(base: &Path, claude_pid: u32, content: &str, timeout: Duration) -> Result<(), String> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let inbox = base.join(claude_pid.to_string()).join("in");
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{nanos}-{}", SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let (tmp, msg) = (inbox.join(format!(".{name}.tmp")), inbox.join(format!("{name}.json")));
    // 서버는 *.json 만 읽는다 — 다 쓴 뒤 rename 해 반쯤 쓴 파일을 읽지 않게
    std::fs::write(&tmp, serde_json::json!({ "content": content }).to_string()).map_err(|e| format!("채널 우편함 쓰기 실패: {e}"))?;
    std::fs::rename(&tmp, &msg).map_err(|e| { let _ = std::fs::remove_file(&tmp); format!("채널 우편함 쓰기 실패: {e}") })?;
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !msg.exists() { return Ok(()); }
        std::thread::sleep(Duration::from_millis(50));
    }
    match std::fs::remove_file(&msg) {
        Ok(()) => Err("터미널 채널이 메시지를 가져가지 않았습니다(채널 서버 멈춤 의심)".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // 회수 직전에 서버가 가져갔다
        Err(e) => Err(format!("채널 우편함 회수 실패: {e}")),
    }
}

pub fn send(claude_pid: u32, content: &str) -> Result<(), String> { send_in(&base_dir(), claude_pid, content, DELIVER_TIMEOUT) }

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_base(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("42/in")).unwrap();
        d
    }

    #[test]
    fn attached_needs_live_server_pid() {
        let b = tmp_base("awb_chan_attached");
        assert!(!attached_in(&b, 42, |_| true), "server.pid 없음");
        std::fs::write(b.join("42/server.pid"), "777\n").unwrap();
        assert!(attached_in(&b, 42, |p| p == 777));
        assert!(!attached_in(&b, 42, |_| false), "서버 프로세스 죽음");
        assert!(!attached_in(&b, 43, |_| true), "다른 claude pid");
    }

    #[test]
    fn send_ok_when_server_takes_file() {
        let b = tmp_base("awb_chan_take");
        let inbox = b.join("42/in");
        // 채널 서버 흉내: *.json 을 읽고 unlink
        let taker = std::thread::spawn({
            let inbox = inbox.clone();
            move || {
                for _ in 0..100 {
                    for e in std::fs::read_dir(&inbox).unwrap().flatten() {
                        let p = e.path();
                        if p.extension().and_then(|x| x.to_str()) == Some("json") {
                            let body = std::fs::read_to_string(&p).unwrap();
                            std::fs::remove_file(&p).unwrap();
                            return body;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                String::new()
            }
        });
        send_in(&b, 42, "안녕 \"폰\"", Duration::from_secs(2)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&taker.join().unwrap()).unwrap();
        assert_eq!(v["content"], "안녕 \"폰\"");
    }

    #[test]
    fn send_reclaims_and_errs_when_nobody_takes() {
        let b = tmp_base("awb_chan_reclaim");
        assert!(send_in(&b, 42, "x", Duration::from_millis(200)).is_err());
        assert_eq!(std::fs::read_dir(b.join("42/in")).unwrap().count(), 0, "회수된 파일이 남으면 서버가 늦게 주입한다");
    }
}
