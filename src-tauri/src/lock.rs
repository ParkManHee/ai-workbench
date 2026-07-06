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
    // pgid<=0은 유효한 소유자 pgid가 아니다(0=killpg가 호출자 자신의 그룹을 대상으로 하므로 오판;
    // 음수도 비정상). acquire()가 러너 업데이트 전에 과도기적으로 pgid:0인 메타를 쓸 수 있으므로,
    // 이런 값은 "살아있음"으로 간주해 탈취를 막는다(안전한 기본값: 훔치지 않는다).
    if pgid <= 0 { return true; }
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
            // rename은 원자적: 이 정확한 stale dir을 rename할 수 있는 건 단 하나의 레이서뿐이다.
            // 패자의 rename은 실패하고(이미 사라짐), 이후 create_dir 경쟁에서 승자에게 밀려 Err를 반환한다.
            // → 갓 생성된 승자의 lock dir을 절대 덮어쓰지 않는다.
            let tmp = dir.with_extension(format!("stale.{}", std::process::id()));
            if fs::rename(&dir, &tmp).is_ok() {
                let _ = fs::remove_dir_all(&tmp);
            }
            match fs::create_dir(&dir) {
                Ok(_) => { let _ = fs::write(dir.join("meta.json"), serde_json::to_string(info).unwrap()); Ok(()) }
                Err(_) => Err(status(workdir).unwrap_or_else(|| info.clone())),
            }
        }
    }
}

pub fn release(workdir: &str) { let _ = fs::remove_dir_all(lock_dir(workdir)); }

#[cfg(test)]
mod tests {
    use super::*;
    fn info(pgid: i32) -> LockInfo { LockInfo{ pid: std::process::id(), pgid, start_ts: 1, source: "test".into() } }

    #[test]
    fn pgid_zero_or_negative_is_treated_as_alive() {
        assert!(pgid_alive(0));
        assert!(pgid_alive(-1));
    }

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

    #[test]
    fn stale_lock_is_stolen() {
        let d = std::env::temp_dir().join("awb_lock_proj_stale"); let _ = std::fs::create_dir_all(&d);
        let w = d.to_str().unwrap();
        release(w); // 청소

        // 확실히 죽은(존재하지 않는) pgid를 찾는다.
        let dead_pgid: i32 = 999_999;
        assert!(!pgid_alive(dead_pgid), "test precondition failed: pgid {} unexpectedly alive", dead_pgid);

        // 죽은 소유자의 lock dir을 수동으로 생성.
        let dir = lock_dir(w);
        std::fs::create_dir_all(&dir).unwrap();
        let stale_info = info(dead_pgid);
        fs::write(dir.join("meta.json"), serde_json::to_string(&stale_info).unwrap()).unwrap();

        // 새 소유자가 stale lock을 탈취할 수 있어야 한다.
        let my_pgid = unsafe { libc::getpgrp() };
        assert!(acquire(w, &info(my_pgid)).is_ok());

        // 새 소유자가 반영되어 있어야 한다.
        let cur = status(w).expect("lock status should exist after steal");
        assert_eq!(cur.pid, std::process::id());
        assert_eq!(cur.pgid, my_pgid);
        assert_eq!(cur.source, "test");

        release(w);
    }
}
