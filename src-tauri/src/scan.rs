use serde::Serialize;
use std::fs;
use std::process::Command;
use std::time::UNIX_EPOCH;

#[derive(Serialize, Clone)]
pub struct Project { pub name: String, pub path: String, /* pub has_origin: bool, */ pub last_activity: u64 }

fn realpath(p: &str) -> String {
    fs::canonicalize(p).map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string())
}

pub fn is_git_repo_root(dir: &str) -> bool {
    let top = Command::new("git").args(["-C", dir, "rev-parse", "--show-toplevel"]).output();
    let is_root = matches!(&top, Ok(o) if o.status.success()
        && realpath(String::from_utf8_lossy(&o.stdout).trim()) == realpath(dir));
    if !is_root { return false; }
    Command::new("git").args(["-C", dir, "remote", "get-url", "origin"])
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn mtime(p: &std::path::Path) -> u64 {
    p.metadata().and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn scan_roots(roots: &[String]) -> Vec<Project> {
    let mut out = Vec::new();
    for root in roots {
        let expanded = crate::paths::expand_tilde(root);
        let entries = match fs::read_dir(&expanded) { Ok(e) => e, Err(_) => continue };
        for e in entries.flatten() {
            let path = e.path();
            if !path.is_dir() { continue; }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" { continue; }
            let ps = path.to_string_lossy().to_string();
            if is_git_repo_root(&ps) {
                // let has_origin = true; // is_git_repo_root 가 origin 보장
                out.push(Project { name, path: realpath(&ps), /* has_origin, */ last_activity: mtime(&path) });
            }
        }
    }
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};
    fn git(args: &[&str], cwd: &std::path::Path) {
        Command::new("git").args(args).current_dir(cwd).output().unwrap();
    }
    #[test]
    fn scan_only_returns_git_roots_with_origin() {
        let base = std::env::temp_dir().join("awb_scan_test");
        let _ = fs::remove_dir_all(&base);
        let root = base.join("roots"); fs::create_dir_all(&root).unwrap();
        // repo A (origin O)
        let a = root.join("alpha"); fs::create_dir_all(&a).unwrap();
        git(&["init","-q"], &a); git(&["remote","add","origin","https://x/alpha.git"], &a);
        // plain dir (no git) -> 제외
        fs::create_dir_all(root.join("plaindir")).unwrap();
        // repo without origin -> 제외
        let b = root.join("beta"); fs::create_dir_all(&b).unwrap(); git(&["init","-q"], &b);
        let names: Vec<String> = scan_roots(&[root.to_string_lossy().to_string()])
            .into_iter().map(|p| p.name).collect();
        assert!(names.contains(&"alpha".to_string()));
        assert!(!names.contains(&"plaindir".to_string()));
        assert!(!names.contains(&"beta".to_string()));
    }
}
