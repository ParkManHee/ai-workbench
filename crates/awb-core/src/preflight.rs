use serde::Serialize;
use std::fs;
use std::process::Command;
use crate::shell_env::{login_path, which_in};

#[derive(Serialize, Clone)]
pub struct Check { pub id: String, pub ok: bool, pub detail: String }
#[derive(Serialize, Clone)]
pub struct Preflight { pub claude_path: Option<String>, pub checks: Vec<Check> }

fn home() -> String { std::env::var("HOME").unwrap_or_default() }

fn resolve_claude(override_path: Option<String>) -> Option<String> {
    if let Some(p) = override_path { if std::path::Path::new(&p).is_file() { return Some(p); } else { return None; } }
    if let Some(p) = which_in(&login_path(), "claude") { return Some(p); }
    let cand = format!("{}/.local/bin/claude", home());
    if std::path::Path::new(&cand).is_file() { Some(cand) } else { None }
}

pub fn run_preflight(roots: &[String], claude_override: Option<String>) -> Preflight {
    let mut checks = Vec::new();
    // 1. claude
    let claude_path = resolve_claude(claude_override.clone());
    let claude_ok = claude_path.as_ref().map(|p|
        Command::new(p).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    ).unwrap_or(false);
    checks.push(Check { id: "claude".into(), ok: claude_ok,
        detail: claude_path.clone().unwrap_or_else(|| "claude 미발견 (PATH/설정 확인)".into()) });
    // 2. roots
    let roots_ok = roots.iter().any(|r| {
        let e = r.replace("~", &home()); std::path::Path::new(&e).is_dir() });
    checks.push(Check { id: "roots".into(), ok: roots_ok,
        detail: if roots_ok {"루트 유효".into()} else {"유효한 project_roots 없음".into()} });
    // 3. worker-settings
    let ws = format!("{}/.claude/worker-settings.json", home());
    let ws_ok = std::path::Path::new(&ws).is_file();
    checks.push(Check { id: "worker_settings".into(), ok: ws_ok, detail: ws });
    // 4. git-crypt unlock (대표 jsonl 매직)
    let locked = sample_locked();
    checks.push(Check { id: "git_crypt".into(), ok: !locked,
        detail: if locked {"트랜스크립트 잠김 — git-crypt unlock 필요".into()} else {"언락됨/평문".into()} });
    Preflight { claude_path, checks }
}

fn sample_locked() -> bool {
    let dir = format!("{}/.claude/projects", home());
    let walk = fs::read_dir(&dir);
    if let Ok(entries) = walk {
        for e in entries.flatten() {
            // 하위의 첫 .jsonl 하나만 샘플
            if let Ok(sub) = fs::read_dir(e.path()) {
                for f in sub.flatten() {
                    let p = f.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        if let Ok(bytes) = fs::read(&p) {
                            return bytes.windows(8).take(16).any(|w| w == b"GITCRYPT");
                        }
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preflight_flags_missing_claude_and_roots() {
        // 존재하지 않는 override + 빈 roots
        let pf = run_preflight(&[], Some("/no/such/claude".into()));
        let claude = pf.checks.iter().find(|c| c.id == "claude").unwrap();
        assert!(!claude.ok);
        let roots = pf.checks.iter().find(|c| c.id == "roots").unwrap();
        assert!(!roots.ok);
    }
}
