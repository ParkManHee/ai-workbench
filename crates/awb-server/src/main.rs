mod auth;
mod config;

use axum::{routing::get, Router};

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
    // 임시 라우터(/health) — Task 6에서 routes::router로 교체
    let app: Router = Router::new().route("/health", get(|| async { "ok" }));
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
