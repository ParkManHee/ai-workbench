mod auth;
mod config;
mod gitdiff;
mod pairing;
mod power;
mod push;
mod launchd;
mod perm;
mod queue;
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
        push: push::PushStore::load(&format!("{}/.claude/.awb-push-tokens.json", std::env::var("HOME").unwrap_or_default())),
        uploads_dir: format!("{}/.claude/.awb-uploads", std::env::var("HOME").unwrap_or_default()),
    };
    let app = routes::router(state);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            // 단일 인스턴스 가드: 주소 사용중이면 기존 서버가 상주 중
            eprintln!("바인드 실패({addr}): {e} — 이미 다른 awb-server가 서빙 중일 수 있음. 종료.");
            std::process::exit(0);
        }
    };
    eprintln!("awb-server 서빙: http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
