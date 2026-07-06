use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct LockInfo { pub pid: u32, pub pgid: i32, pub start_ts: u64, pub source: String }

fn home() -> String { std::env::var("HOME").unwrap_or_default() }
fn realpath(p: &str) -> String { fs::canonicalize(p).map(|x| x.to_string_lossy().into()).unwrap_or_else(|_| p.into()) }

fn sha1_hex(s: &str) -> String {
    // 의존성 최소화: 간단한 FNV-1a 64bit(충돌 실질 무시). 파일명 안전.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); }
    format!("{:016x}", h)
}

pub fn lock_dir(workdir: &str) -> PathBuf {
    PathBuf::from(home()).join(".claude/.run-locks").join(sha1_hex(&realpath(workdir)))
}

fn pgid_alive(pgid: i32) -> bool {
    // killpg(pgid, 0): 존재하면 Ok
    unsafe { libc::killpg(pgid, 0) == 0 }
}

pub fn status(workdir: &str) -> Option<LockInfo> {
    let meta = lock_dir(workdir).join("meta.json");
    let s = fs::read_to_string(meta).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn acquire(workdir: &str, info: &LockInfo) -> Result<(), LockInfo> {
    let dir = lock_dir(workdir);
    if let Some(parent) = dir.parent() { let _ = fs::create_dir_all(parent); }
    match fs::create_dir(&dir) {
        Ok(_) => { let _ = fs::write(dir.join("meta.json"), serde_json::to_string(info).unwrap()); Ok(()) }
        Err(_) => {
            if let Some(cur) = status(workdir) {
                if pgid_alive(cur.pgid) { return Err(cur); }
            }
            // stale(소유자 죽음/메타 없음) → 탈취
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir(&dir).map_err(|_| info.clone())?;
            let _ = fs::write(dir.join("meta.json"), serde_json::to_string(info).unwrap());
            Ok(())
        }
    }
}

pub fn release(workdir: &str) { let _ = fs::remove_dir_all(lock_dir(workdir)); }

#[cfg(test)]
mod tests {
    use super::*;
    fn info(pgid: i32) -> LockInfo { LockInfo{ pid: std::process::id(), pgid, start_ts: 1, source: "test".into() } }
    #[test]
    fn acquire_is_exclusive_then_releasable() {
        let d = std::env::temp_dir().join("awb_lock_proj"); let _ = std::fs::create_dir_all(&d);
        let w = d.to_str().unwrap();
        release(w); // 청소
        let my_pgid = unsafe { libc::getpgrp() };
        assert!(acquire(w, &info(my_pgid)).is_ok());
        // 살아있는 소유자(pgid=현재 프로세스 그룹) → 재획득 실패
        assert!(acquire(w, &info(my_pgid)).is_err());
        release(w);
        assert!(acquire(w, &info(my_pgid)).is_ok());
        release(w);
    }
}
