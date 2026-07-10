// 데몬 상주화 — macOS LaunchAgent 등록/해제.
// `awb-server install`: 로그인 시 자동 시작(RunAtLoad) + 크래시 시 재시작(KeepAlive.SuccessfulExit=false).
// 단일 인스턴스 가드(바인드 실패 exit 0)와 조합: 수동 인스턴스가 떠 있으면 launchd 쪽은 조용히 종료·재시도 안 함.
use std::process::Command;

pub const LABEL: &str = "com.aiworkbench.awb-server";

fn home() -> String { std::env::var("HOME").unwrap_or_default() }
fn plist_path() -> String { format!("{}/Library/LaunchAgents/{LABEL}.plist", home()) }
pub fn log_path() -> String { format!("{}/.claude/awb-daemon.log", home()) }

/// plist 내용(순수) — exe: 데몬 바이너리 절대경로, log: stdout/err 파일, path_env: 로그인셸 PATH.
pub fn plist_content(exe: &str, log: &str, path_env: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>serve</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key><false/>
  </dict>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>{path_env}</string>
  </dict>
</dict>
</plist>
"#
    )
}

fn gui_domain() -> String {
    let uid = Command::new("id").arg("-u").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "501".to_string());
    format!("gui/{uid}")
}

fn launchctl(args: &[&str]) -> bool {
    Command::new("launchctl").args(args).status().map(|s| s.success()).unwrap_or(false)
}

pub fn install() {
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => { eprintln!("실행파일 경로 확인 실패: {e}"); std::process::exit(1); }
    };
    let log = log_path();
    // 로그인셸 PATH를 심어 claude/node 해석이 터미널과 동일하게 되도록
    let path_env = awb_core::shell_env::login_path();
    let plist = plist_content(&exe, &log, &path_env);
    let path = plist_path();
    if let Some(dir) = std::path::Path::new(&path).parent() { let _ = std::fs::create_dir_all(dir); }
    if let Err(e) = std::fs::write(&path, plist) {
        eprintln!("plist 기록 실패({path}): {e}");
        std::process::exit(1);
    }
    let domain = gui_domain();
    // 기존 등록 제거(없으면 무시) 후 등록
    let _ = launchctl(&["bootout", &domain, &path]);
    if launchctl(&["bootstrap", &domain, &path]) {
        println!("LaunchAgent 등록 완료: {path}");
        println!("데몬 로그: {log}");
        println!("해제: awb-server uninstall");
    } else {
        eprintln!("launchctl bootstrap 실패 — 수동 확인 필요: launchctl bootstrap {domain} {path}");
        std::process::exit(1);
    }
}

pub fn uninstall() {
    let domain = gui_domain();
    let path = plist_path();
    let _ = launchctl(&["bootout", &domain, &path]);
    let _ = std::fs::remove_file(&path);
    println!("LaunchAgent 해제 완료");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plist_contains_keepalive_and_paths() {
        let p = plist_content("/x/awb-server", "/y/daemon.log", "/usr/bin:/bin");
        assert!(p.contains("<string>/x/awb-server</string>"));
        assert!(p.contains("<string>serve</string>"));
        assert!(p.contains("SuccessfulExit"));
        assert!(p.contains("<string>/y/daemon.log</string>"));
        assert!(p.contains("<key>PATH</key><string>/usr/bin:/bin</string>"));
        assert!(p.contains(LABEL));
    }
}
