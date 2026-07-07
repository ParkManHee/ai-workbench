mod auth;
mod config;
mod gitdiff;
mod pairing;
mod power;
mod routes;
mod runreg;
mod sessions;
mod streamevt;

#[tokio::main]
async fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    match arg.as_str() {
        "serve" => serve().await,
        other => { eprintln!("알 수 없는 명령: {other} (지원: serve)"); std::process::exit(2); }
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
    println!("=== 폰 페어링 QR (60초 유효) — 코드 {} ===", pc.code);
    println!("{}", pairing::render_terminal(&payload));
    pairing::save_png(&payload, &format!("{}/.claude/.awb-pair.png", std::env::var("HOME").unwrap_or_default()));
    let state = routes::AppState {
        devices,
        pairing: std::sync::Arc::new(std::sync::Mutex::new(Some(pc))),
        roots: routes::default_roots(),
        power: power::PowerGuard::new(),
        sessions: sessions::SessionStore::load(&cfg.sessions_dir),
        runs: runreg::RunRegistry::new(),
        claude_bin: cfg.claude_bin.clone(),
        settings_path: cfg.settings_path.clone(),
        runs_dir: cfg.runs_dir.clone(),
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
