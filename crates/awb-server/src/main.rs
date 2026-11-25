mod auth;
mod config;
mod gitdiff;
mod pairing;
mod power;
mod push;
mod gc;
mod launchd;
mod perm;
mod queue;
mod resume;
mod routes;
mod runreg;
mod sessions;
mod streamevt;
mod ws;

#[tokio::main]
async fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    match arg.as_str() {
        "serve" => serve().await,
        "install" => launchd::install(),
        "uninstall" => launchd::uninstall(),
        other => { eprintln!("알 수 없는 명령: {other} (지원: serve|install|uninstall)"); std::process::exit(2); }
    }
}

async fn serve() {
    let cfg = config::ServerConfig::from_env();
    let addr = config::bind_addr(&cfg);
    // 바인드를 맨 앞에 — 실패 재시작 루프마다 QR 출력·예약 스케줄러 첫 틱(지난 예약 실행)이 반복되지 않게
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("바인드 실패({addr}): {e} — 종료.");
            std::process::exit(bind_fail_exit_code(e.kind()));
        }
    };
    if config::tailnet_ipv4().is_none() {
        eprintln!("경고: tailscale IPv4 미탐지 — 루프백({addr})에만 바인드(외부 폰 접속 불가). Tailscale 실행 확인.");
    }
    let devices = auth::DeviceStore::load(&cfg.devices_path);
    let pc = pairing::PairingCode::generate();
    let ip = config::tailnet_ipv4().unwrap_or_else(|| "127.0.0.1".into());
    let payload = format!("awb://{ip}:{}?code={}", cfg.port, pc.code);
    println!("=== 폰 페어링 QR (10분 유효) — 코드 {} ===", pc.code);
    println!("{}", pairing::render_terminal(&payload));
    pairing::save_png(&payload, &format!("{}/.claude/.awb-pair.png", std::env::var("HOME").unwrap_or_default()));
    let roots = routes::default_roots();
    // claude 실행 파일 경로 결정: awb-core preflight의 탐지 결과 우선(PATH+~/.local/bin 탐색),
    // 없으면 AWB_CLAUDE_BIN 환경변수, 그마저 없으면 리터럴 "claude"(PATH 의존 최종 폴백).
    let claude_bin = awb_core::preflight::run_preflight(&roots, None).claude_path
        .or_else(|| std::env::var("AWB_CLAUDE_BIN").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "claude".into());
    let state = routes::AppState {
        devices,
        pairing: std::sync::Arc::new(std::sync::Mutex::new(Some(pc))),
        roots,
        power: power::PowerGuard::new(),
        sessions: sessions::SessionStore::load(&cfg.sessions_dir),
        runs: runreg::RunRegistry::new(),
        claude_bin,
        settings_path: cfg.settings_path.clone(),
        runs_dir: cfg.runs_dir.clone(),
        queue: queue::TurnQueue::new(),
        perms: perm::PermStore::new(),
        perm_secret: pairing::random_token(),
        base_url: format!("http://{addr}"),
        started_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        push: push::PushStore::load(&format!("{}/.claude/.awb-push-tokens.json", std::env::var("HOME").unwrap_or_default())),
        uploads_dir: format!("{}/.claude/.awb-uploads", std::env::var("HOME").unwrap_or_default()),
        rate_limit: Default::default(),
        resume: resume::ResumeStore::load(&format!("{}/.claude/.awb-resume-at-reset.json", std::env::var("HOME").unwrap_or_default())),
    };
    gc::spawn_gc(cfg.runs_dir.clone(), state.uploads_dir.clone());
    resume::spawn_scheduler(state.clone());
    let app = routes::router(state.clone());
    eprintln!("awb-server 서빙: http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

/// 주소 사용 중(다른 awb-server 상주)만 정상 종료(0) — launchd KeepAlive{SuccessfulExit:false} 가 되살리지 않게.
/// 그 밖(Tailscale 꺼져 tailnet IP 가 없음 = EADDRNOTAVAIL 등)은 1 로 끝내 launchd 가 다시 띄우게 한다.
/// 2026-11-05~24 exit 0 으로 끝나 19일간 데몬이 꺼져 있었다.
fn bind_fail_exit_code(kind: std::io::ErrorKind) -> i32 {
    if kind == std::io::ErrorKind::AddrInUse { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::bind_fail_exit_code;
    use std::io::ErrorKind;

    #[test]
    fn bind_fail_restarts_unless_addr_in_use() {
        assert_eq!(bind_fail_exit_code(ErrorKind::AddrInUse), 0);
        assert_eq!(bind_fail_exit_code(ErrorKind::AddrNotAvailable), 1);
    }
}
