use std::process::Command;

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub devices_path: String,
    pub runs_dir: String,
    pub sessions_dir: String,
    pub settings_path: String,
}

fn home() -> String { std::env::var("HOME").unwrap_or_default() }

impl ServerConfig {
    pub fn from_env() -> ServerConfig {
        let port = std::env::var("AWB_SERVER_PORT").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(8787);
        ServerConfig {
            port,
            devices_path: format!("{}/.claude/.awb-devices.json", home()),
            runs_dir: format!("{}/.claude/.awb-runs", home()),
            sessions_dir: format!("{}/.claude/.awb-sessions", home()),
            settings_path: format!("{}/.claude/worker-settings.json", home()),
        }
    }
}

/// tailscale이 할당한 이 머신의 IPv4(100.x.y.z). 미탐지면 None.
pub fn tailnet_ipv4() -> Option<String> {
    let out = Command::new("tailscale").args(["ip", "-4"]).output().ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().map(|l| l.trim()).find(|l| l.starts_with("100.")).map(|l| l.to_string())
}

/// 바인드 주소: tailnet IP가 있으면 그 주소, 없으면 루프백(경고는 호출자가).
pub fn bind_addr(cfg: &ServerConfig) -> String {
    match tailnet_ipv4() {
        Some(ip) => format!("{ip}:{}", cfg.port),
        None => format!("127.0.0.1:{}", cfg.port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_addr_uses_loopback_without_tailscale() {
        // tailscale 미설치/미탐지 환경 가정 시 루프백 폴백 포맷 확인
        let cfg = ServerConfig { port: 9999, devices_path: "/x".into(), runs_dir: "/y".into(), sessions_dir: "/s".into(), settings_path: "/w".into() };
        let addr = bind_addr(&cfg);
        assert!(addr.ends_with(":9999"));
        assert!(addr.starts_with("100.") || addr.starts_with("127.0.0.1"));
    }
    #[test]
    fn from_env_default_port_is_8787() {
        // AWB_SERVER_PORT 미설정 시 8787 (테스트 격리 위해 값 확인만)
        std::env::remove_var("AWB_SERVER_PORT");
        assert_eq!(ServerConfig::from_env().port, 8787);
    }
}
