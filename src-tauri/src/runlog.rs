use serde::Serialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::process::Command as PCmd;

#[derive(Serialize, Clone)]
pub struct LogChunk {
    pub text: String,
    pub offset: u64,
    pub done: bool,
    pub exit_code: Option<i32>,
}

pub fn read_log(log: &str, offset: u64) -> LogChunk {
    let mut text = String::new();
    let mut new_off = offset;
    if let Ok(mut f) = fs::File::open(log) {
        if f.seek(SeekFrom::Start(offset)).is_ok() {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                new_off = offset + buf.len() as u64;
                text = String::from_utf8_lossy(&buf).into_owned();
            }
        }
    }
    let done_path = format!("{log}.done");
    let (done, exit_code) = match fs::read_to_string(&done_path) {
        Ok(s) => (true, s.trim().parse::<i32>().ok()),
        Err(_) => (false, None),
    };
    LogChunk {
        text,
        offset: new_off,
        done,
        exit_code,
    }
}

#[derive(Serialize, Clone)]
pub struct RunStatus {
    pub done: bool,
    pub exit_code: Option<i32>,
    pub changed_files: u32,
    pub verdict: String,
}

fn changed_files(workdir: &str) -> u32 {
    let out = PCmd::new("git").args(["-C", workdir, "status", "--porcelain"]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).count() as u32).unwrap_or(0)
}

pub fn run_status(log: &str, workdir: &str) -> RunStatus {
    let chunk = read_log(log, 0);
    if !chunk.done {
        return RunStatus { done: false, exit_code: None, changed_files: 0, verdict: "running".into() };
    }
    // 락 해제 전에 소유자 pid를 읽어, 디태치된 자식을 좀비로 남기지 않도록 non-blocking으로 회수한다.
    // 앱 재시작 후에는 자식이 이미 init에 reparent되었거나 회수되었을 수 있으니 ECHILD 등은 무시한다.
    if let Some(owner) = crate::lock::status(workdir) {
        unsafe { libc::waitpid(owner.pid as i32, std::ptr::null_mut(), libc::WNOHANG); }
    }
    crate::lock::release(workdir);
    let cf = changed_files(workdir);
    let verdict = match chunk.exit_code {
        Some(0) => if cf == 0 { "success(무변경)".into() } else { "success".into() },
        Some(_) => "failed".into(),
        None => "failed".into(),
    };
    RunStatus { done: true, exit_code: chunk.exit_code, changed_files: cf, verdict }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_and_done() {
        let log = std::env::temp_dir().join("awb_tail.log");
        // Cleanup from previous runs
        let _ = std::fs::remove_file(&log);
        let done_file = format!("{}.done", log.to_str().unwrap());
        let _ = std::fs::remove_file(&done_file);

        std::fs::write(&log, "line1\n").unwrap();
        let c1 = read_log(log.to_str().unwrap(), 0);
        assert_eq!(c1.text, "line1\n");
        assert!(!c1.done);
        std::fs::write(&log, "line1\nline2\n").unwrap();
        let c2 = read_log(log.to_str().unwrap(), c1.offset);
        assert_eq!(c2.text, "line2\n");
        std::fs::write(&done_file, "0\n").unwrap();
        let c3 = read_log(log.to_str().unwrap(), c2.offset);
        assert!(c3.done);
        assert_eq!(c3.exit_code, Some(0));

        // Cleanup
        let _ = std::fs::remove_file(&log);
        let _ = std::fs::remove_file(&done_file);
    }

    #[test]
    fn verdicts() {
        let log = std::env::temp_dir().join("awb_status.log");
        std::fs::write(&log, "x").unwrap();
        let wd = std::env::temp_dir().join("awb_status_wd"); std::fs::create_dir_all(&wd).unwrap();
        // 미완료
        let _ = std::fs::remove_file(format!("{}.done", log.to_str().unwrap()));
        assert_eq!(run_status(log.to_str().unwrap(), wd.to_str().unwrap()).verdict, "running");
        // 실패
        std::fs::write(format!("{}.done", log.to_str().unwrap()), "1").unwrap();
        assert_eq!(run_status(log.to_str().unwrap(), wd.to_str().unwrap()).verdict, "failed");
    }
}
