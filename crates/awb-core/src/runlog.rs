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

/// `<log>.done`이 없을 때만 `code`를 기록한다(이미 존재하면 실제 종료코드를 덮어쓰지 않음).
/// 그룹 SIGTERM/SIGKILL로 래퍼 sh가 `.done`을 못 쓴 경우의 보정용(취소=143, 타임아웃=124).
pub fn mark_done_if_absent(log: &str, code: i32) {
    let done_path = format!("{log}.done");
    if !std::path::Path::new(&done_path).exists() {
        let _ = fs::write(&done_path, code.to_string());
    }
}

/// 작업트리 스냅샷 트리 해시 — 임시 인덱스에 `add -A` 후 `write-tree`. 실제 인덱스·작업트리는 건드리지 않는다.
/// 실제 인덱스 사본에서 시작한다: 빈 인덱스에서 시작하면 gitignore 에 걸린 추적 파일이 스냅샷에서 빠지고,
/// 사본은 stat 캐시로 바뀌지 않은 파일의 재해시도 건너뛴다. git 저장소가 아니면 None.
pub fn snapshot_tree(workdir: &str) -> Option<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let git = |args: &[&str], idx: Option<&str>| -> Option<String> {
        let mut c = PCmd::new("git");
        c.arg("-C").arg(workdir).args(args);
        if let Some(i) = idx { c.env("GIT_INDEX_FILE", i); }
        let o = c.output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let real = std::path::Path::new(workdir).join(git(&["rev-parse", "--git-path", "index"], None)?);
    let tmp = std::env::temp_dir().join(format!("awb-idx-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let tmp_s = tmp.to_string_lossy().into_owned();
    let _ = fs::copy(&real, &tmp); // 실제 인덱스가 없으면(첫 add 전) 빈 인덱스에서 시작
    let tree = git(&["add", "-A"], Some(&tmp_s)).and_then(|_| git(&["write-tree"], Some(&tmp_s)));
    let _ = fs::remove_file(&tmp);
    tree
}

/// 스냅샷 해시를 `<log>.t0`/`<log>.t1` 에 남긴다 — run_id 로 찾을 수 있게 로그 옆에 둔다.
pub fn record_snapshot(workdir: &str, log: &str, which: &str) {
    if let Some(t) = snapshot_tree(workdir) { let _ = fs::write(format!("{log}.{which}"), t); }
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
    // 끝 스냅샷: 성공·실패·취소·타임아웃 모두 여기(done 이후 첫 호출)를 지난다. 락 해제 전에 찍어 다음 턴과 섞이지 않게
    if std::path::Path::new(&format!("{log}.t0")).exists() && !std::path::Path::new(&format!("{log}.t1")).exists() {
        record_snapshot(workdir, log, "t1");
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
    fn mark_done_if_absent_does_not_overwrite() {
        let dir = std::env::temp_dir().join("awb_markdone"); std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("r.log"); let log = log.to_str().unwrap();
        let done = format!("{log}.done");
        let _ = std::fs::remove_file(&done);
        mark_done_if_absent(log, 124);
        assert_eq!(std::fs::read_to_string(&done).unwrap(), "124");
        mark_done_if_absent(log, 143); // 이미 있으면 덮어쓰지 않음
        assert_eq!(std::fs::read_to_string(&done).unwrap(), "124");
        let _ = std::fs::remove_file(&done);
    }

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
