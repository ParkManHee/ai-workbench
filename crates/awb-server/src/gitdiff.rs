// crates/awb-server/src/gitdiff.rs (Task 5) — git status/diff 파싱하여 변경 요약 생성
use serde::Serialize;
use std::process::Command;

#[derive(Serialize, Clone)]
pub struct DiffEntry { pub path: String, pub status: String }

#[derive(Serialize, Clone)]
pub struct DiffSummary { pub files: u32, pub insertions: u32, pub deletions: u32, pub entries: Vec<DiffEntry> }

pub fn summarize(workdir: &str) -> DiffSummary {
    let status = Command::new("git").args(["-C", workdir, "status", "--porcelain"]).output();
    let entries: Vec<DiffEntry> = status.map(|o| {
        String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).map(|l| {
            // porcelain v1 형식: "XY <path>" — 상태 접두 2글자는 항상 ASCII(단일바이트)이므로
            // 바이트 인덱스 2에서 잘라도 char-boundary 문제가 없다(경로에 멀티바이트 문자가 있어도 안전).
            let (st, path) = if l.len() >= 2 { l.split_at(2) } else { l.split_at(0) };
            DiffEntry { path: path.trim().to_string(), status: st.trim().to_string() }
        }).collect()
    }).unwrap_or_default();
    let numstat = Command::new("git").args(["-C", workdir, "diff", "--numstat"]).output();
    let (mut ins, mut del) = (0u32, 0u32);
    if let Ok(o) = numstat {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = l.split_whitespace();
            if let (Some(a), Some(b)) = (it.next(), it.next()) {
                ins += a.parse::<u32>().unwrap_or(0);
                del += b.parse::<u32>().unwrap_or(0);
            }
        }
    }
    DiffSummary { files: entries.len() as u32, insertions: ins, deletions: del, entries }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_changed_files() {
        let wd = std::env::temp_dir().join("awb_diff_wd"); let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let w = wd.to_str().unwrap();
        Command::new("git").args(["-C", w, "init"]).output().unwrap();
        std::fs::write(wd.join("a.txt"), "hello\n").unwrap();
        let s = summarize(w);
        assert!(s.files >= 1);                        // 미추적 a.txt 포함
        assert!(s.entries.iter().any(|e| e.path == "a.txt"));
    }
}
