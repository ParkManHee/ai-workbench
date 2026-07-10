// 디스크 GC — .awb-runs(실행 로그)와 .awb-uploads(첨부 이미지)가 단조 증가하던 문제.
// 완료된 실행(.done 존재)의 로그·부속 파일과 오래된 업로드를 보존기간 경과 시 삭제한다.
// 활성 실행 로그(.done 없음)는 나이와 무관하게 보호.
use std::fs;
use std::time::{Duration, SystemTime};

fn default_max_age_secs() -> u64 {
    let days = std::env::var("AWB_GC_DAYS").ok().and_then(|s| s.parse().ok()).unwrap_or(7u64);
    days * 24 * 3600
}

fn is_older_than(p: &std::path::Path, cutoff: SystemTime) -> bool {
    fs::metadata(p).and_then(|m| m.modified()).map(|t| t < cutoff).unwrap_or(false)
}

/// 1회 수집. 반환: (runs 삭제 수, uploads 삭제 수)
pub fn gc_once(runs_dir: &str, uploads_dir: &str, max_age_secs: u64) -> (usize, usize) {
    let cutoff = SystemTime::now() - Duration::from_secs(max_age_secs);
    let mut runs_deleted = 0usize;
    if let Ok(entries) = fs::read_dir(runs_dir) {
        for e in entries.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { continue };
            if !is_older_than(&p, cutoff) { continue; }
            if name.ends_with(".log") {
                // 완료된 실행만 삭제(.done 존재) — 활성 로그 보호
                let done = format!("{}.done", p.to_string_lossy());
                if std::path::Path::new(&done).exists() {
                    if fs::remove_file(&p).is_ok() { runs_deleted += 1; }
                    if fs::remove_file(&done).is_ok() { runs_deleted += 1; }
                }
            } else if name.ends_with(".log.done") || name.ends_with(".mcp.json") {
                // 고아 .done / 승인 MCP 설정 잔여물
                if fs::remove_file(&p).is_ok() { runs_deleted += 1; }
            }
        }
    }
    let mut uploads_deleted = 0usize;
    if let Ok(entries) = fs::read_dir(uploads_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && is_older_than(&p, cutoff) {
                if fs::remove_file(&p).is_ok() { uploads_deleted += 1; }
            }
        }
    }
    (runs_deleted, uploads_deleted)
}

/// 데몬 시작 시 1회 + 6시간 간격 반복.
pub fn spawn_gc(runs_dir: String, uploads_dir: String) {
    tokio::spawn(async move {
        loop {
            let age = default_max_age_secs();
            let (r, u) = gc_once(&runs_dir, &uploads_dir, age);
            if r + u > 0 {
                eprintln!("gc: runs {r}개, uploads {u}개 정리(보존 {}일)", age / 86400);
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gc_deletes_done_runs_and_uploads_but_protects_active() {
        let base = std::env::temp_dir().join("awb_gc_test");
        let _ = fs::remove_dir_all(&base);
        let runs = base.join("runs"); let ups = base.join("uploads");
        fs::create_dir_all(&runs).unwrap(); fs::create_dir_all(&ups).unwrap();
        // 완료된 실행(log+done), 활성 실행(log만), 고아 mcp.json, 업로드 1개
        fs::write(runs.join("a.log"), "x").unwrap();
        fs::write(runs.join("a.log.done"), "0").unwrap();
        fs::write(runs.join("active.log"), "x").unwrap();
        fs::write(runs.join("approval-1-2.mcp.json"), "{}").unwrap();
        fs::write(ups.join("img.jpg"), "x").unwrap();
        // max_age=0 → 모두 "오래됨" 처리
        let (r, u) = gc_once(runs.to_str().unwrap(), ups.to_str().unwrap(), 0);
        assert_eq!(u, 1);
        assert!(r >= 3, "log+done+mcp.json 삭제, got {r}");
        assert!(runs.join("active.log").exists(), "활성 로그(.done 없음)는 보호");
        assert!(!runs.join("a.log").exists());
        assert!(!ups.join("img.jpg").exists());
    }
}
