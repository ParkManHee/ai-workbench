# ai-workbench Mobile — Plan 2a: awb-server 기반·인증·읽기 엔드포인트

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 폰이 접속하는 Mac 데몬(`awb-server`)의 기반을 세운다 — axum HTTP 서버를 tailnet 인터페이스에만 바인드(단일 인스턴스 가드), QR 페어링으로 디바이스 토큰을 발급하고 Bearer 토큰으로 요청을 인증하며, 읽기 엔드포인트(`/projects`·`/diff`·`/awake`)를 제공한다. claude 실행·스트리밍·푸시는 Plan 2b 범위.

**Architecture:** 신규 `crates/awb-server` 바이너리 크레이트. axum(0.8)+tokio 비동기 런타임. awb-core(동기)를 `spawn_blocking` 없이 직접 호출(스캔·프리플라이트는 짧음). 서버 상태 `AppState`(설정·디바이스 스토어·페어링 코드·전원 핸들)를 `axum::extract::State`로 공유. tailnet IPv4는 `tailscale ip -4`로 탐지, 실패 시 루프백 폴백(경고 로그). 단일 인스턴스는 tailnet 주소:포트 바인드 실패(주소 사용중)로 감지.

**Tech Stack:** Rust(stable), axum 0.8, tokio(rt-multi-thread+macros+process+signal), tower(ServiceExt::oneshot 테스트), serde/serde_json, sha2(토큰 해시), rand(코드·토큰 난수), qrcode(터미널/PNG QR)+image(PNG 인코딩), awb-core(path 의존). 테스트: `#[tokio::test]` + `router.oneshot(Request)` 로 네트워크 없이 핸들러 검증.

## Global Constraints

- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- macOS 우선(유닉스). caffeinate는 macOS 전용 — 비-macOS 분기는 주석 TODO만.
- **awb-core는 순수 로직만** — awb-server가 awb-core를 소비하되, awb-core에 HTTP/tauri/전송 의존을 절대 추가하지 않는다.
- **바인드는 tailnet 인터페이스 전용** — `0.0.0.0`/공개 바인드 금지. tailscale 미탐지 시 `127.0.0.1` 폴백(로그로 경고).
- **토큰은 SHA-256 해시만 저장** — 원문 저장 금지. 저장 파일 `~/.claude/.awb-devices.json`.
- **페어링 코드만 단기(만료 60s)**, 디바이스 토큰은 장기(무만료, unpair로만 해제).
- 고정 포트 상수 하나(기본 `8787`, `AWB_SERVER_PORT`로 override). 단일 인스턴스 가드는 바인드 실패로 판정.
- 공유 실행 락(runlock)은 Plan 2b(실행)에서 사용 — 이 플랜엔 실행 경로 없음.
- 모노레포: `crates/awb-server`를 워크스페이스 멤버로 추가.

## File Structure (Plan 2a 범위)

```
Cargo.toml                       수정 — workspace members에 "crates/awb-server" 추가
crates/awb-server/
  Cargo.toml                     신규 — 바이너리 크레이트 + 의존성
  src/main.rs                    진입점: CLI(serve|unpair) + tailnet bind + 단일인스턴스 + 라우터 조립
  src/config.rs                  ServerConfig(포트·경로) + tailnet IPv4 탐지
  src/auth.rs                    DeviceStore(해시 저장/검증) + Bearer 미들웨어 + unpair
  src/pairing.rs                 PairingCode(생성·만료·검증) + QR 렌더(터미널/PNG)
  src/power.rs                   PowerGuard: caffeinate on/off
  src/routes.rs                  AppState + 핸들러(/health,/projects,/diff,/awake) + 라우터 빌더
```

---

### Task 1: awb-server 크레이트 스켈레톤 + config + 단일 인스턴스 바인드 + /health

**Files:** Create `crates/awb-server/Cargo.toml`, `crates/awb-server/src/main.rs`, `crates/awb-server/src/config.rs`; Modify root `Cargo.toml`(members).

**Interfaces:**
- Produces: `config::ServerConfig { port: u16, devices_path: String, runs_dir: String }`, `config::tailnet_ipv4() -> Option<String>`, `config::bind_addr(cfg) -> String`.
- Produces: `awb-server` 바이너리. `serve` 서브커맨드(기본)로 tailnet:port 바인드 시도, 실패 시 "이미 실행 중(포트 사용)" 로그 후 종료.
- Produces (router): `routes::router(state) -> axum::Router` 는 Task 6에서 완성 — 이 태스크에선 `/health`만 가진 임시 라우터를 main에 인라인.

- [ ] **Step 1: 워크스페이스 멤버 추가 + 크레이트 매니페스트**

Modify root `Cargo.toml` — `members` 배열에 추가(순서 무관):
```toml
[workspace]
resolver = "2"
members = ["src-tauri", "crates/awb-core", "crates/awb-server"]
```

Create `crates/awb-server/Cargo.toml`:
```toml
[package]
name = "awb-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "awb-server"
path = "src/main.rs"

[dependencies]
awb-core = { path = "../awb-core" }
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "signal", "net", "time"] }
tower = { version = "0.5", features = ["util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
rand = "0.8"
qrcode = "0.14"
image = { version = "0.25", default-features = false, features = ["png"] }
```

- [ ] **Step 2: config.rs 작성 + 실패 테스트(bind_addr/포트 override)**

Create `crates/awb-server/src/config.rs`:
```rust
use std::process::Command;

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub devices_path: String,
    pub runs_dir: String,
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
```
Append test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_addr_uses_loopback_without_tailscale() {
        // tailscale 미설치/미탐지 환경 가정 시 루프백 폴백 포맷 확인
        let cfg = ServerConfig { port: 9999, devices_path: "/x".into(), runs_dir: "/y".into() };
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
```

- [ ] **Step 3: 실패 확인** — Run: `cargo test -p awb-server config::` → FAIL(크레이트/모듈 미완성으로 컴파일 에러 또는 미발견).

- [ ] **Step 4: main.rs 작성(bind + 단일 인스턴스 + /health)**

Create `crates/awb-server/src/main.rs`:
```rust
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
```

- [ ] **Step 5: 통과 확인** — Run: `cargo test -p awb-server config::` → PASS(2 tests). Run: `cargo build -p awb-server` → 성공.

- [ ] **Step 6: 스모크(수동 가능)** — `cargo run -p awb-server -- serve &` 후 `curl -s http://127.0.0.1:8787/health` → `ok`. (tailscale 있으면 tailnet IP로 바인드됨.) 확인 후 프로세스 종료.

- [ ] **Step 7: 커밋** — `git add -A && git commit -m "$(printf 'feat(server): awb-server 스켈레톤 + tailnet 바인드 + 단일 인스턴스 가드 + /health\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"`

---

### Task 2: 디바이스 토큰 스토어 + Bearer 인증 미들웨어

**Files:** Create `crates/awb-server/src/auth.rs`; Modify `main.rs`(`mod auth;`).

**Interfaces:**
- Consumes: `config::ServerConfig.devices_path`.
- Produces:
  - `auth::Device { id: String, token_hash: String, label: String, paired_at: u64 }`
  - `auth::DeviceStore { path: String }` with:
    - `load(path) -> DeviceStore`
    - `add(&self, raw_token: &str, label: &str) -> Device` — SHA-256 해시 저장, 파일에 append.
    - `verify(&self, raw_token: &str) -> bool` — 해시 일치하는 디바이스 존재?
    - `list(&self) -> Vec<Device>`, `remove(&self, id: &str) -> bool`
  - `auth::sha256_hex(s: &str) -> String`
  - `auth::require_token` — axum 미들웨어(`from_fn_with_state`): `Authorization: Bearer <t>` 없거나 verify 실패 시 401.

- [ ] **Step 1: 실패 테스트 — 해시 저장/검증, unknown 거부**

Create `crates/awb-server/src/auth.rs` (테스트 먼저 포함):
```rust
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct Device { pub id: String, pub token_hash: String, pub label: String, pub paired_at: u64 }

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new(); h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone)]
pub struct DeviceStore { pub path: String }

impl DeviceStore {
    pub fn load(path: &str) -> DeviceStore { DeviceStore { path: path.to_string() } }
    fn read(&self) -> Vec<Device> {
        fs::read_to_string(&self.path).ok()
            .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn write(&self, v: &[Device]) {
        if let Ok(s) = serde_json::to_string_pretty(v) { let _ = fs::write(&self.path, s); }
    }
    pub fn list(&self) -> Vec<Device> { self.read() }
    pub fn add(&self, raw_token: &str, label: &str) -> Device {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let id = sha256_hex(&format!("{label}:{now}"))[..12].to_string();
        let dev = Device { id, token_hash: sha256_hex(raw_token), label: label.to_string(), paired_at: now };
        let mut v = self.read(); v.push(dev.clone()); self.write(&v); dev
    }
    pub fn verify(&self, raw_token: &str) -> bool {
        let h = sha256_hex(raw_token);
        self.read().iter().any(|d| d.token_hash == h)
    }
    pub fn remove(&self, id: &str) -> bool {
        let mut v = self.read(); let before = v.len();
        v.retain(|d| d.id != id); self.write(&v); v.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str) -> String { std::env::temp_dir().join(name).to_string_lossy().to_string() }
    #[test]
    fn add_then_verify_roundtrip() {
        let p = tmp("awb_devices_test1.json"); let _ = std::fs::remove_file(&p);
        let s = DeviceStore::load(&p);
        assert!(!s.verify("tok-abc"));            // 없음
        let d = s.add("tok-abc", "phone1");
        assert!(s.verify("tok-abc"));             // 등록 후 통과
        assert!(!s.verify("tok-xyz"));            // 다른 토큰 거부
        // 원문 저장 안 함(해시만)
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(!raw.contains("tok-abc"));
        assert!(raw.contains(&d.token_hash));
        assert!(s.remove(&d.id));                 // 제거
        assert!(!s.verify("tok-abc"));
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p awb-server auth::` → FAIL(모듈 미등록).

- [ ] **Step 3: 미들웨어 추가 + main에 mod 등록**

Append to `auth.rs`:
```rust
use axum::{extract::State, http::{Request, StatusCode}, middleware::Next, response::Response, body::Body};

pub async fn require_token(
    State(store): State<DeviceStore>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = req.headers().get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| store.verify(t))
        .unwrap_or(false);
    if ok { Ok(next.run(req).await) } else { Err(StatusCode::UNAUTHORIZED) }
}
```
Add `mod auth;` to `main.rs`.

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p awb-server auth::` → PASS. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 커밋** — `... -m "feat(server): 디바이스 토큰 스토어(SHA-256 해시) + Bearer 인증 미들웨어"` (+트레일러)

---

### Task 3: QR 페어링 (코드 생성·만료·검증 + QR 렌더) + /pair 라우트

**Files:** Create `crates/awb-server/src/pairing.rs`; Modify `main.rs`(`mod pairing;`).

**Interfaces:**
- Consumes: `auth::DeviceStore`.
- Produces:
  - `pairing::PairingCode { code: String, expires_at: u64 }` with `generate() -> PairingCode`(6자리 base32류 난수, 만료 now+60), `is_valid(&self, code, now) -> bool`.
  - `pairing::random_token() -> String` (32바이트 hex).
  - `pairing::render_terminal(payload: &str) -> String` (유니코드 QR 문자열), `pairing::save_png(payload, path)`.
  - `pairing::pair_handler` — `GET /pair?code=<>`: 현재 페어링 코드와 일치·미만료면 새 토큰 발급→DeviceStore.add→`{token, device_id}` JSON, 아니면 403.

- [ ] **Step 1: 실패 테스트 — 코드 만료·검증, 토큰 발급 길이**

Create `crates/awb-server/src/pairing.rs`:
```rust
use rand::Rng;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct PairingCode { pub code: String, pub expires_at: u64 }

fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

pub fn random_token() -> String {
    let mut b = [0u8; 32]; rand::thread_rng().fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl PairingCode {
    pub fn generate() -> PairingCode {
        const A: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789"; // 혼동문자 제외
        let mut r = rand::thread_rng();
        let code: String = (0..6).map(|_| A[r.gen_range(0..A.len())] as char).collect();
        PairingCode { code, expires_at: now() + 60 }
    }
    pub fn is_valid(&self, code: &str, at: u64) -> bool {
        self.code == code && at <= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_validates_until_expiry() {
        let pc = PairingCode { code: "ABC234".into(), expires_at: 1000 };
        assert!(pc.is_valid("ABC234", 999));
        assert!(pc.is_valid("ABC234", 1000));
        assert!(!pc.is_valid("ABC234", 1001));   // 만료
        assert!(!pc.is_valid("WRONG1", 999));     // 코드 불일치
    }
    #[test]
    fn token_is_64_hex_chars() {
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p awb-server pairing::` → FAIL.

- [ ] **Step 3: QR 렌더 + 핸들러 구현**

Append to `pairing.rs`:
```rust
use qrcode::QrCode;
use qrcode::render::unicode;

pub fn render_terminal(payload: &str) -> String {
    match QrCode::new(payload.as_bytes()) {
        Ok(code) => code.render::<unicode::Dense1x2>().quiet_zone(true).build(),
        Err(_) => "(QR 생성 실패)".to_string(),
    }
}

pub fn save_png(payload: &str, path: &str) {
    if let Ok(code) = QrCode::new(payload.as_bytes()) {
        let img = code.render::<image::Luma<u8>>().build();
        let _ = img.save(path);
    }
}
```
Handler (uses shared state from routes; defined here, wired in Task 6):
```rust
use axum::{extract::{State, Query}, http::StatusCode, Json};
use crate::routes::AppState;

#[derive(serde::Deserialize)]
pub struct PairQuery { pub code: String }

#[derive(Serialize)]
pub struct PairResult { pub token: String, pub device_id: String }

pub async fn pair_handler(
    State(st): State<AppState>,
    Query(q): Query<PairQuery>,
) -> Result<Json<PairResult>, StatusCode> {
    let at = now();
    let valid = { st.pairing.lock().unwrap().as_ref().map(|pc| pc.is_valid(&q.code, at)).unwrap_or(false) };
    if !valid { return Err(StatusCode::FORBIDDEN); }
    // 1회성: 사용 후 코드 무효화
    *st.pairing.lock().unwrap() = None;
    let token = random_token();
    let dev = st.devices.add(&token, "mobile");
    Ok(Json(PairResult { token, device_id: dev.id }))
}
```
Add `mod pairing;` to `main.rs`. (`AppState`는 Task 6에서 정의 — 이 태스크 커밋 시점엔 routes.rs 최소 스텁이 필요하므로, Step 3 마지막에 `routes.rs`에 `AppState`만 먼저 선언해 컴파일을 맞춘다. 아래 스텁:)
```rust
// crates/awb-server/src/routes.rs (Task 3에서 최소 선언, Task 6에서 확장)
use std::sync::{Arc, Mutex};
use crate::auth::DeviceStore;
use crate::pairing::PairingCode;

#[derive(Clone)]
pub struct AppState {
    pub devices: DeviceStore,
    pub pairing: Arc<Mutex<Option<PairingCode>>>,
}
```
Add `mod routes;` to `main.rs`.

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p awb-server pairing::` → PASS. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 커밋** — `... -m "feat(server): QR 페어링(코드 만료·1회성) + 토큰 발급 + /pair 핸들러"` (+트레일러)

---

### Task 4: /projects 엔드포인트 (awb-core scan+preflight)

**Files:** Modify `crates/awb-server/src/routes.rs`(핸들러 추가).

**Interfaces:**
- Consumes: `awb_core::scan::scan_roots`, `awb_core::preflight::run_preflight`, `awb_core::worklog::badge_for`.
- Produces: `routes::projects_handler` — `GET /projects`: 설정된 roots를 스캔해 프로젝트 목록 + worklog 배지 JSON 반환. roots는 `AppState.roots: Vec<String>`.
- Produces: `AppState`에 `roots: Vec<String>` 필드 추가(기본 `~/bitbucket`, `~/github` — `AWB_ROOTS` 콤마구분 override).

- [ ] **Step 1: 실패 테스트 — 임시 git repo 스캔 결과 반환**

Append to `routes.rs` test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state(root: &str) -> AppState {
        AppState {
            devices: crate::auth::DeviceStore::load("/tmp/awb_dev_unused.json"),
            pairing: std::sync::Arc::new(std::sync::Mutex::new(None)),
            roots: vec![root.to_string()],
        }
    }
    #[tokio::test]
    async fn projects_returns_scanned_repos() {
        // origin 있는 가짜 git repo 하나 생성
        let base = std::env::temp_dir().join("awb_proj_scan"); let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("demo"); std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "init"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "remote", "add", "origin", "x"]).output().unwrap();
        let app = crate::routes::router(test_state(base.to_str().unwrap()));
        let res = app.oneshot(Request::builder().uri("/projects").header("authorization","Bearer skip").body(Body::empty()).unwrap()).await.unwrap();
        // 인증 미들웨어는 Task 6에서 붙음 — 이 단계 라우터는 /projects에 인증 미적용(단위검증). 200 + demo 포함.
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("demo"));
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p awb-server routes::tests::projects_returns_scanned_repos` → FAIL(핸들러/roots 없음).

- [ ] **Step 3: 구현**

Modify `routes.rs`:
```rust
use axum::{extract::State, Json};
use serde::Serialize;

// AppState에 roots 추가
#[derive(Clone)]
pub struct AppState {
    pub devices: DeviceStore,
    pub pairing: Arc<Mutex<Option<PairingCode>>>,
    pub roots: Vec<String>,
}

#[derive(Serialize)]
pub struct ProjectDto {
    pub name: String,
    pub path: String,
    pub last_activity: u64,
    pub badge: Option<awb_core::worklog::Badge>,
}

pub async fn projects_handler(State(st): State<AppState>) -> Json<Vec<ProjectDto>> {
    let projects = awb_core::scan::scan_roots(&st.roots);
    let dtos = projects.into_iter().map(|p| {
        let badge = awb_core::worklog::badge_for(&p.name);
        ProjectDto { name: p.name, path: p.path, last_activity: p.last_activity, badge }
    }).collect();
    Json(dtos)
}

pub fn default_roots() -> Vec<String> {
    match std::env::var("AWB_ROOTS") {
        Ok(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Err(_) => {
            let home = std::env::var("HOME").unwrap_or_default();
            vec![format!("{home}/bitbucket"), format!("{home}/github")]
        }
    }
}

// 이 태스크의 router(): /projects 만(인증 미적용 — Task 6에서 완성 라우터로 대체)
pub fn router(state: AppState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/projects", get(projects_handler))
        .with_state(state)
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p awb-server routes::tests::projects_returns_scanned_repos` → PASS. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 커밋** — `... -m "feat(server): /projects (awb-core scan+preflight 배지)"` (+트레일러)

---

### Task 5: /diff 엔드포인트 (git 변경 요약)

**Files:** Create `crates/awb-server/src/gitdiff.rs`; Modify `main.rs`(`mod gitdiff;`), `routes.rs`(핸들러).

**Interfaces:**
- Produces: `gitdiff::DiffSummary { files: u32, insertions: u32, deletions: u32, entries: Vec<DiffEntry> }`, `gitdiff::DiffEntry { path: String, status: String }`.
- Produces: `gitdiff::summarize(workdir: &str) -> DiffSummary` — `git -C <workdir> status --porcelain`(파일·상태) + `git -C <workdir> diff --numstat`(라인 합계) 파싱.
- Produces: `routes::diff_handler` — `GET /diff?path=<workdir>`: summarize 반환.

- [ ] **Step 1: 실패 테스트 — 변경 파일 카운트**

Create `crates/awb-server/src/gitdiff.rs`:
```rust
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
            let (st, path) = l.split_at(l.char_indices().nth(2).map(|(i,_)| i).unwrap_or(0));
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
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p awb-server gitdiff::` → FAIL.

- [ ] **Step 3: 핸들러 배선**

Append to `routes.rs`:
```rust
use axum::extract::Query;

#[derive(serde::Deserialize)]
pub struct DiffQuery { pub path: String }

pub async fn diff_handler(Query(q): Query<DiffQuery>) -> Json<crate::gitdiff::DiffSummary> {
    Json(crate::gitdiff::summarize(&q.path))
}
```
Add `mod gitdiff;` to `main.rs`.

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p awb-server gitdiff::` → PASS. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 커밋** — `... -m "feat(server): /diff git 변경 요약(porcelain+numstat)"` (+트레일러)

---

### Task 6: /awake 전원 어서션 + 인증 라우터 통합 + 페어링 코드 표시

**Files:** Create `crates/awb-server/src/power.rs`; Modify `routes.rs`(완성 라우터·awake 핸들러·AppState 전원핸들), `main.rs`(serve에서 페어링 코드 생성·QR 표시·완성 라우터 사용).

**Interfaces:**
- Produces: `power::PowerGuard { child: Arc<Mutex<Option<Child>>> }` with `set(on: bool)` — on이면 `caffeinate -s`(시스템 슬립 방지) 자식 spawn, off면 kill.
- Produces: `routes::awake_handler` — `POST /awake` body `{on: bool}` → PowerGuard.set.
- Produces: `routes::router(state)` **완성본** — `/pair`(무인증), 나머지(`/projects`,`/diff`,`/awake`,`/health`)는 `require_token` 미들웨어 적용. `AppState`에 `power: PowerGuard` 추가.
- Produces: main `serve()`가 시작 시 `PairingCode::generate()` → AppState에 저장 → 터미널 QR 출력(payload=`awb://<tailnet_ip>:<port>?code=<code>`) + `~/.claude/.awb-pair.png` 저장.

- [ ] **Step 1: 실패 테스트 — awake 토글이 caffeinate 자식을 생성/종료**

Create `crates/awb-server/src/power.rs`:
```rust
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PowerGuard { child: Arc<Mutex<Option<Child>>> }

impl PowerGuard {
    pub fn new() -> PowerGuard { PowerGuard { child: Arc::new(Mutex::new(None)) } }
    pub fn is_active(&self) -> bool { self.child.lock().unwrap().is_some() }
    pub fn set(&self, on: bool) {
        let mut g = self.child.lock().unwrap();
        if on {
            if g.is_none() {
                // macOS: caffeinate -s (AC 전원 시 시스템 슬립 방지). 비-macOS는 TODO.
                if let Ok(c) = Command::new("caffeinate").arg("-s").spawn() { *g = Some(c); }
            }
        } else if let Some(mut c) = g.take() {
            let _ = c.kill(); let _ = c.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggle_spawns_and_kills() {
        let pg = PowerGuard::new();
        assert!(!pg.is_active());
        pg.set(true);
        assert!(pg.is_active());   // caffeinate 자식 존재(마 macOS 가정; caffeinate 없으면 이 테스트는 macOS 전용)
        pg.set(false);
        assert!(!pg.is_active());
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p awb-server power::` → FAIL.

- [ ] **Step 3: 완성 라우터 + awake 핸들러 + AppState 확장 + main serve 페어링 표시**

Modify `routes.rs` — AppState에 `power` 추가, awake 핸들러, 완성 라우터:
```rust
use crate::power::PowerGuard;

#[derive(Clone)]
pub struct AppState {
    pub devices: DeviceStore,
    pub pairing: Arc<Mutex<Option<PairingCode>>>,
    pub roots: Vec<String>,
    pub power: PowerGuard,
}

#[derive(serde::Deserialize)]
pub struct AwakeBody { pub on: bool }

pub async fn awake_handler(State(st): State<AppState>, Json(b): Json<AwakeBody>) -> StatusCode {
    st.power.set(b.on);
    StatusCode::OK
}

pub fn router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};
    use axum::middleware::from_fn_with_state;
    // 인증 필요 라우트
    let protected = axum::Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/projects", get(projects_handler))
        .route("/diff", get(diff_handler))
        .route("/awake", post(awake_handler))
        .layer(from_fn_with_state(state.devices.clone(), crate::auth::require_token));
    // 무인증: /pair
    axum::Router::new()
        .route("/pair", get(crate::pairing::pair_handler))
        .merge(protected)
        .with_state(state)
}
```
(`StatusCode`·`Json`·`get`/`post`·`State` import 정리. Task4의 임시 `router()`는 이 완성본으로 대체.)

Modify `main.rs` `serve()` — 라우터를 `routes::router(state)`로 교체하고 페어링 코드 생성·표시:
```rust
// serve() 내부, 바인드 전:
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
};
let app = routes::router(state);
```
Add `mod power;` to `main.rs`. Remove the temporary inline `/health` router.

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p awb-server` → PASS(config/auth/pairing/routes/gitdiff/power 전체). `cargo build -p awb-server` → 성공. `cargo test --workspace` → 기존 16 + awb-server 신규 전부 green.

- [ ] **Step 5: 스모크(수동)** — `cargo run -p awb-server -- serve` → 터미널 QR + "서빙" 로그. 다른 셸에서: `curl -s http://127.0.0.1:8787/projects` → 401(무토큰). tailscale 있으면 tailnet IP로 접속. 확인 후 종료.

- [ ] **Step 6: 커밋** — `... -m "feat(server): /awake 전원 어서션 + 인증 라우터 통합 + 시작 시 페어링 QR 표시"` (+트레일러)

---

## Plan 2a Self-Review

- **Spec coverage:** 스펙 §awb-server 중 이 플랜 범위 = tailnet 바인드(Task1)·단일 인스턴스 가드(Task1)·QR 페어링/토큰 이중방어(Task2/3)·`/projects`(Task4)·`/diff`(Task5)·`/awake` 전원 어서션(Task6). 인증 미들웨어(Task2/6). **범위 밖(→Plan 2b):** `/chat`·WS `/stream`·`/cancel`·`/status`·stream-json 러너·세션(`--resume`)·Expo 푸시·`/push/register`.
- **Placeholder scan:** 각 스텝 실제 코드·명령·기대출력. TODO는 비-macOS 분기(caffeinate)만(의도적, 스펙의 macOS 우선).
- **Type consistency:** `AppState`는 Task3(devices/pairing)→Task4(+roots)→Task6(+power)로 점증 확장하며 각 태스크가 정의를 갱신함을 명시(중간 컴파일 위해 Task3에서 최소 선언). `DeviceStore`(add/verify/list/remove), `PairingCode`(generate/is_valid), `PowerGuard`(set/is_active) 시그니처 태스크 간 일치. `router(state)`는 Task4 임시→Task6 완성본으로 대체됨을 명시.
- **위험:** (1) axum 0.8 미들웨어/State API 버전차 — 빌드 실패 시 axum 0.8 문서대로 `from_fn_with_state`·`Next`·`to_bytes` 시그니처 조정(구현자 재량). (2) tailnet/caffeinate/git 의존 테스트는 환경 민감 — 단위테스트는 순수 로직(해시·코드만료·numstat 파싱) 위주, 실제 바인드/QR/caffeinate는 수동 스모크로 표기. (3) power 테스트는 macOS의 caffeinate 존재 가정(스펙 macOS 우선과 일치).

## 다음 플랜
- **Plan 2b (awb-server 실행·스트리밍·푸시):** stream-json 러너(claude --output-format stream-json [--resume])·세션 스토어(project→session_id)·`/chat`(락 획득)·WS `/stream/:run_id`(이벤트 로그 append·offset catch-up)·`/cancel`·`/status`·Expo 푸시(완료 발송·notified 플래그)·`/push/register`. awb-core runner/runlog/lock 재사용.
- **Plan 3 (mobile RN+Expo 앱):** 페어링·프로젝트목록·채팅·WS 스트리밍·푸시 등록·git요약.
