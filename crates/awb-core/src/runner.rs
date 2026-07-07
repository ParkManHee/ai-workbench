use serde::Serialize;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::lock::{acquire, LockInfo};

#[derive(Serialize, Clone)]
pub struct RunHandle { pub log: String, pub pgid: i32 }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() }

/// 동일 초·동일 프로세스 내 다중 실행이 같은 로그 파일명을 갖지 않도록
/// epoch초 + pid + 프로세스-로컬 원자 카운터를 조합한 충돌 방지 로그 경로 생성.
/// (awb-server는 로그 파일명에서 run_id를 파생하므로, 동일 파일명 충돌 시
/// 로그 truncation·잘못된 취소·락 누수로 이어질 수 있었음.)
static LOG_SEQ: AtomicU64 = AtomicU64::new(0);
fn unique_log_path(runs_dir: &str) -> String {
    let secs = now();
    let pid = std::process::id();
    let seq = LOG_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{}/{}-{}-{}.log", runs_dir, secs, pid, seq)
}
fn app_dir() -> String {
    // awb-run.sh 위치: 앱 리소스. 개발 중엔 리포 scripts/. 배포 시 Tauri resource 경로로 교체(TODO).
    std::env::var("AWB_SCRIPTS_DIR").unwrap_or_else(|_| format!("{}/github/ai-workbench/scripts", std::env::var("HOME").unwrap_or_default()))
}

pub fn start_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, runs_dir: &str) -> Result<RunHandle, String> {
    let settings = crate::paths::expand_tilde(settings);
    let runs_dir = crate::paths::expand_tilde(runs_dir);
    let workdir_e = crate::paths::expand_tilde(workdir);
    // 락 선점
    let placeholder = LockInfo { pid: std::process::id(), pgid: 0, start_ts: now(), source: "app".into() };
    if let Err(cur) = acquire(&workdir_e, &placeholder) {
        return Err(format!("이미 실행 중: {} (pid {})", cur.source, cur.pid));
    }
    std::fs::create_dir_all(&runs_dir).map_err(|e| { crate::lock::release(&workdir_e); format!("runs_dir 생성 실패: {e}") })?;
    let log = unique_log_path(&runs_dir);
    let wrapper = format!("{}/awb-run.sh", app_dir());
    let plan_flag = if plan { "1" } else { "0" };
    let child = unsafe {
        Command::new("sh")
            .args([&wrapper, claude_bin, &workdir_e, &log, &settings, plan_flag, prompt])
            .pre_exec(|| { libc::setsid(); Ok(()) })  // 자체 세션/PGID
            .stdin(std::process::Stdio::null())
            .spawn()
    }.map_err(|e| { crate::lock::release(&workdir_e); format!("spawn 실패: {e}") })?;
    let pgid = child.id() as i32; // setsid 후 자식 pid == pgid
    // 락 메타 pgid 갱신
    let info = LockInfo { pid: child.id(), pgid, start_ts: now(), source: "app".into() };
    let _ = std::fs::write(crate::lock::lock_dir(&workdir_e).join("meta.json"), serde_json::to_string(&info).unwrap());
    Ok(RunHandle { log, pgid })
}

pub fn start_stream_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, resume: Option<&str>, runs_dir: &str) -> Result<RunHandle, String> {
    let settings = crate::paths::expand_tilde(settings);
    let runs_dir = crate::paths::expand_tilde(runs_dir);
    let workdir_e = crate::paths::expand_tilde(workdir);
    let placeholder = LockInfo { pid: std::process::id(), pgid: 0, start_ts: now(), source: "daemon".into() };
    if let Err(cur) = acquire(&workdir_e, &placeholder) {
        return Err(format!("이미 실행 중: {} (pid {})", cur.source, cur.pid));
    }
    std::fs::create_dir_all(&runs_dir).map_err(|e| { crate::lock::release(&workdir_e); format!("runs_dir 생성 실패: {e}") })?;
    let log = unique_log_path(&runs_dir);
    let wrapper = format!("{}/awb-run-stream.sh", app_dir());
    let plan_flag = if plan { "1" } else { "0" };
    let resume_arg = resume.unwrap_or("");
    let child = unsafe {
        Command::new("sh")
            .args([&wrapper, claude_bin, &workdir_e, &log, &settings, plan_flag, resume_arg, prompt])
            .pre_exec(|| { libc::setsid(); Ok(()) })
            .stdin(std::process::Stdio::null())
            .spawn()
    }.map_err(|e| { crate::lock::release(&workdir_e); format!("spawn 실패: {e}") })?;
    let pgid = child.id() as i32;
    let info = LockInfo { pid: child.id(), pgid, start_ts: now(), source: "daemon".into() };
    let _ = std::fs::write(crate::lock::lock_dir(&workdir_e).join("meta.json"), serde_json::to_string(&info).unwrap());
    Ok(RunHandle { log, pgid })
}

pub fn cancel_run(pgid: i32, workdir: &str) -> bool {
    // pgid<=1: 유효하지 않은 그룹 — 락은 여기서 해제하지 않음(호출자는 RunHandle.pgid=live child.id()만 전달)
    if pgid <= 1 { return false; }
    unsafe { libc::killpg(pgid, libc::SIGTERM); }
    std::thread::sleep(std::time::Duration::from_millis(300));
    if unsafe { libc::killpg(pgid, 0) } == 0 {
        unsafe { libc::killpg(pgid, libc::SIGKILL); }
    }
    crate::lock::release(workdir);
    let dead = unsafe { libc::killpg(pgid, 0) } != 0;
    dead
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_log_path_differs_on_consecutive_calls() {
        let runs = std::env::temp_dir().join("awb_runs_unique");
        std::fs::create_dir_all(&runs).unwrap();
        let a = unique_log_path(runs.to_str().unwrap());
        let b = unique_log_path(runs.to_str().unwrap());
        assert_ne!(a, b, "동일 초·동일 프로세스라도 카운터로 경로가 달라야 함");
    }

    #[test]
    fn cancel_kills_group() {
        // sleep 30 을 setsid로 띄우고 pgid 취소
        use std::os::unix::process::CommandExt;
        let child = unsafe { std::process::Command::new("sh").args(["-c","sleep 30"]).pre_exec(||{libc::setsid();Ok(())}).spawn().unwrap() };
        let pgid = child.id() as i32;
        let wd = std::env::temp_dir().join("awb_cancel_wd"); std::fs::create_dir_all(&wd).unwrap();
        assert!(cancel_run(pgid, wd.to_str().unwrap()));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(unsafe { libc::killpg(pgid, 0) } != 0); // 그룹 사라짐
    }
    #[test]
    fn start_run_spawns_and_locks() {
        let dir = std::env::temp_dir().join("awb_runner_proj"); std::fs::create_dir_all(&dir).unwrap();
        let runs = std::env::temp_dir().join("awb_runs"); std::fs::create_dir_all(&runs).unwrap();
        crate::lock::release(dir.to_str().unwrap());
        // 가짜 claude: 0.5초 자고 종료
        let fake = std::env::temp_dir().join("fakeclaude2");
        std::fs::write(&fake, "#!/bin/sh\nsleep 0.3\necho done\n").unwrap();
        #[cfg(unix)]{ use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        let h = start_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", runs.to_str().unwrap()).unwrap();
        assert!(std::path::Path::new(&h.log).exists() || h.pgid > 0);
        // 실행 중 재요청 → 락 거부
        assert!(start_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", runs.to_str().unwrap()).is_err());
        // 완료 대기 후 .done 확인
        std::thread::sleep(std::time::Duration::from_millis(700));
        assert!(std::path::Path::new(&format!("{}.done", h.log)).exists());
        crate::lock::release(dir.to_str().unwrap());
    }

    #[test]
    fn start_stream_run_spawns_with_resume_arg() {
        std::env::set_var("AWB_SCRIPTS_DIR", format!("{}/../../scripts", env!("CARGO_MANIFEST_DIR")));
        let dir = std::env::temp_dir().join("awb_stream_proj"); std::fs::create_dir_all(&dir).unwrap();
        let runs = std::env::temp_dir().join("awb_stream_runs"); std::fs::create_dir_all(&runs).unwrap();
        crate::lock::release(dir.to_str().unwrap());
        // 가짜 claude: 인자를 그대로 에코(우리가 --resume/--output-format 전달했는지 확인용) 후 종료
        let fake = std::env::temp_dir().join("fakeclaude_stream");
        std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$*\"\n").unwrap();
        #[cfg(unix)]{ use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        let h = start_stream_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", Some("sess-1"), runs.to_str().unwrap()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let log = std::fs::read_to_string(&h.log).unwrap_or_default();
        assert!(log.contains("--output-format stream-json"));
        assert!(log.contains("--resume sess-1"));
        assert!(std::path::Path::new(&format!("{}.done", h.log)).exists());
        crate::lock::release(dir.to_str().unwrap());
    }
}
