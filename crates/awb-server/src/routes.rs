// crates/awb-server/src/routes.rs (Task 3에서 최소 선언, Task 4에서 /projects·/chat·/status·/cancel·/preflight 추가)
use std::sync::{Arc, Mutex};
use crate::auth::DeviceStore;
use crate::pairing::PairingCode;
use crate::power::PowerGuard;
use crate::runreg::RunRegistry;
use crate::sessions::SessionStore;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{extract::State, Json};
use serde::Serialize;

#[derive(Clone)]
pub struct AppState {
    pub devices: DeviceStore,
    pub pairing: Arc<Mutex<Option<PairingCode>>>,
    pub roots: Vec<String>,
    pub power: PowerGuard,
    pub sessions: SessionStore,
    pub runs: RunRegistry,
    pub claude_bin: String,
    pub settings_path: String,
    pub runs_dir: String,
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

#[derive(serde::Deserialize)]
pub struct DiffQuery { pub path: String }

pub async fn diff_handler(Query(q): Query<DiffQuery>) -> Json<crate::gitdiff::DiffSummary> {
    Json(crate::gitdiff::summarize(&q.path))
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

#[derive(serde::Deserialize)]
pub struct AwakeBody { pub on: bool }

pub async fn awake_handler(State(st): State<AppState>, Json(b): Json<AwakeBody>) -> StatusCode {
    st.power.set(b.on);
    StatusCode::OK
}

#[derive(serde::Deserialize)]
pub struct ChatBody { pub prompt: String, #[serde(default)] pub plan: bool }
#[derive(Serialize)]
pub struct ChatResult { pub run_id: String, pub log: String }

pub async fn chat_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<ChatBody>) -> Result<Json<ChatResult>, (StatusCode, String)> {
    // 프로젝트 경로 확인
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project)
        .ok_or((StatusCode::NOT_FOUND, "unknown project".into()))?;
    let resume = st.sessions.get(&project);
    let h = awb_core::runner::start_stream_run(&st.claude_bin, &proj.path, &st.settings_path, b.plan, &b.prompt, resume.as_deref(), &st.runs_dir)
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    let run_id = h.log.rsplit('/').next().unwrap_or(&h.log).trim_end_matches(".log").to_string();
    st.runs.insert(&run_id, crate::runreg::RunMeta { log: h.log.clone(), pgid: h.pgid, workdir: proj.path.clone(), project: project.clone(), notified: false });
    // 완료 워처(푸시) spawn — Task 7에서 push::spawn_watch 로 연결. 이 태스크에선 등록만.
    Ok(Json(ChatResult { run_id, log: h.log }))
}

pub async fn status_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<Json<awb_core::runlog::RunStatus>, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(awb_core::runlog::run_status(&meta.log, &meta.workdir)))
}

pub async fn cancel_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<StatusCode, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    let dead = awb_core::runner::cancel_run(meta.pgid, &meta.workdir);
    Ok(if dead { StatusCode::OK } else { StatusCode::ACCEPTED })
}

pub async fn preflight_handler(State(st): State<AppState>) -> Json<awb_core::preflight::Preflight> {
    Json(awb_core::preflight::run_preflight(&st.roots, Some(st.claude_bin.clone())))
}

/// 완성 라우터: `/pair`는 무인증, 나머지(`/health`,`/projects`,`/diff`,`/awake`,`/chat`,`/status`,`/cancel`,`/preflight`)는 `require_token` 미들웨어 적용.
pub fn router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};
    use axum::middleware::from_fn_with_state;
    // 인증 필요 라우트
    let protected = axum::Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/projects", get(projects_handler))
        .route("/diff", get(diff_handler))
        .route("/awake", post(awake_handler))
        .route("/chat/{project}", post(chat_handler))
        .route("/status/{run_id}", get(status_handler))
        .route("/cancel/{run_id}", post(cancel_handler))
        .route("/preflight", get(preflight_handler))
        .layer(from_fn_with_state(state.devices.clone(), crate::auth::require_token));
    // 무인증: /pair
    axum::Router::new()
        .route("/pair", get(crate::pairing::pair_handler))
        .merge(protected)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn tmp(name: &str) -> String { std::env::temp_dir().join(name).to_string_lossy().to_string() }

    fn test_state(root: &str, devices_path: &str) -> AppState {
        let sessions_dir = std::env::temp_dir().join("awb_routes_test_sessions").to_string_lossy().to_string();
        AppState {
            devices: crate::auth::DeviceStore::load(devices_path),
            pairing: std::sync::Arc::new(std::sync::Mutex::new(None)),
            roots: vec![root.to_string()],
            power: crate::power::PowerGuard::new(),
            sessions: crate::sessions::SessionStore::load(&sessions_dir),
            runs: crate::runreg::RunRegistry::new(),
            claude_bin: "claude".into(),
            settings_path: "/tmp/ws.json".into(),
            runs_dir: std::env::temp_dir().join("awb_routes_test_runs").to_string_lossy().to_string(),
        }
    }

    #[tokio::test]
    async fn projects_returns_scanned_repos() {
        // origin 있는 가짜 git repo 하나 생성
        let base = std::env::temp_dir().join("awb_proj_scan"); let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("demo"); std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "init"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "remote", "add", "origin", "x"]).output().unwrap();
        let devices_path = tmp("awb_routes_devices_projects.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state(base.to_str().unwrap(), &devices_path);
        let token = "tok-projects-ok";
        state.devices.add(token, "test-device");
        let app = crate::routes::router(state);
        let res = app.oneshot(Request::builder().uri("/projects").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("demo"));
    }

    #[tokio::test]
    async fn projects_without_auth_header_is_unauthorized() {
        let devices_path = tmp("awb_routes_devices_noauth.json"); let _ = std::fs::remove_file(&devices_path);
        let app = crate::routes::router(test_state("/tmp", &devices_path));
        let res = app.oneshot(Request::builder().uri("/projects").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn projects_with_wrong_scheme_is_unauthorized() {
        let devices_path = tmp("awb_routes_devices_wrongscheme.json"); let _ = std::fs::remove_file(&devices_path);
        let app = crate::routes::router(test_state("/tmp", &devices_path));
        let res = app.oneshot(Request::builder().uri("/projects").header("authorization", "Basic xxx").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn projects_with_unknown_token_is_unauthorized() {
        let devices_path = tmp("awb_routes_devices_unknown.json"); let _ = std::fs::remove_file(&devices_path);
        let app = crate::routes::router(test_state("/tmp", &devices_path));
        let res = app.oneshot(Request::builder().uri("/projects").header("authorization", "Bearer unknown-token").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn projects_with_known_token_is_ok() {
        let devices_path = tmp("awb_routes_devices_known.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        let token = "tok-known-good";
        state.devices.add(token, "test-device");
        let app = crate::routes::router(state);
        let res = app.oneshot(Request::builder().uri("/projects").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn pair_route_requires_no_auth() {
        let devices_path = tmp("awb_routes_devices_pair.json"); let _ = std::fs::remove_file(&devices_path);
        let mut state = test_state("/tmp", &devices_path);
        state.pairing = std::sync::Arc::new(std::sync::Mutex::new(Some(crate::pairing::PairingCode { code: "ABC234".into(), expires_at: u64::MAX })));
        let app = crate::routes::router(state);
        // 무인증(Authorization 헤더 없음) + 틀린 코드 → 403이어야 하며 절대 401이 아니어야 함(인증 미들웨어 미적용 확인)
        let res = app.oneshot(Request::builder().uri("/pair?code=WRONG1").body(Body::empty()).unwrap()).await.unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
