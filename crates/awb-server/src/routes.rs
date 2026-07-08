// crates/awb-server/src/routes.rs (Task 3에서 최소 선언, Task 4에서 /projects·/chat·/status·/cancel·/preflight 추가)
use std::sync::{Arc, Mutex};
use crate::auth::DeviceStore;
use crate::pairing::PairingCode;
use crate::power::PowerGuard;
use crate::push::PushStore;
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
    pub push: PushStore,
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
pub struct ChatBody { pub prompt: String, #[serde(default)] pub plan: bool, #[serde(default)] pub resume_session_id: Option<String> }
#[derive(Serialize)]
pub struct ChatResult { pub run_id: String, pub log: String }

pub async fn chat_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<ChatBody>) -> Result<Json<ChatResult>, (StatusCode, String)> {
    // 프로젝트 경로 확인
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project)
        .ok_or((StatusCode::NOT_FOUND, "unknown project".into()))?;
    // resume_session_id가 있으면 특정 세션으로 강제 resume(폰에서 과거 세션 선택), 없으면 프로젝트별 저장된 마지막 세션 사용
    let resume = b.resume_session_id.clone().or_else(|| st.sessions.get(&project));
    let h = awb_core::runner::start_stream_run(&st.claude_bin, &proj.path, &st.settings_path, b.plan, &b.prompt, resume.as_deref(), &st.runs_dir)
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    let run_id = h.log.rsplit('/').next().unwrap_or(&h.log).trim_end_matches(".log").to_string();
    st.runs.insert(&run_id, crate::runreg::RunMeta { log: h.log.clone(), pgid: h.pgid, workdir: proj.path.clone(), project: project.clone(), notified: false });
    crate::push::spawn_watch(st.clone(), run_id.clone()); // 완료 워처: WS 미소비 시 푸시 발송, 완료 시 락 해제도 보장
    Ok(Json(ChatResult { run_id, log: h.log }))
}

pub async fn status_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<Json<awb_core::runlog::RunStatus>, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(awb_core::runlog::run_status(&meta.log, &meta.workdir)))
}

/// `<log>.done` 마커가 없을 때만 `code`를 기록한다(이미 존재하면 실제 종료코드를 덮어쓰지 않음).
/// cancel_handler에서 사용: 프로세스 그룹을 SIGTERM/SIGKILL하면 래퍼 sh가 `.done`을 쓰기 전에
/// 죽을 수 있어 WS/워처가 완료를 관측하지 못하는데, 그 경우를 여기서 보정한다.
fn mark_done_if_absent(log: &str, code: i32) {
    let done_path = format!("{log}.done");
    if !std::path::Path::new(&done_path).exists() {
        let _ = std::fs::write(&done_path, code.to_string());
    }
}

pub async fn cancel_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<StatusCode, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    let dead = awb_core::runner::cancel_run(meta.pgid, &meta.workdir);
    // SIGTERM/SIGKILL로 그룹 전체가 죽으면 래퍼가 .done을 못 쓸 수 있으므로, 취소를 관측 가능하게 직접 기록한다(128+SIGTERM=143).
    mark_done_if_absent(&meta.log, 143);
    Ok(if dead { StatusCode::OK } else { StatusCode::ACCEPTED })
}

pub async fn preflight_handler(State(st): State<AppState>) -> Json<awb_core::preflight::Preflight> {
    Json(awb_core::preflight::run_preflight(&st.roots, Some(st.claude_bin.clone())))
}

pub async fn sessions_handler(State(st): State<AppState>, Path(project): Path<String>) -> Result<Json<Vec<crate::transcript::SessionInfo>>, StatusCode> {
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(crate::transcript::list_sessions(&crate::transcript::project_slug(&proj.path))))
}

#[derive(serde::Deserialize)]
pub struct TxQuery { #[serde(default)] pub from: usize }

pub async fn transcript_handler(State(st): State<AppState>, Path((project, session_id)): Path<(String, String)>, Query(q): Query<TxQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !crate::transcript::safe_session_id(&session_id) { return Err(StatusCode::BAD_REQUEST); }
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    let slug = crate::transcript::project_slug(&proj.path);
    let path = format!("{}/.claude/projects/{}/{}.jsonl", std::env::var("HOME").unwrap_or_default(), slug, session_id);
    let (msgs, next, active) = crate::transcript::read_transcript(&path, q.from);
    Ok(Json(serde_json::json!({ "messages": msgs, "next": next, "active": active })))
}

#[derive(serde::Deserialize)]
pub struct PushRegisterBody { pub token: String }

pub async fn push_register_handler(State(st): State<AppState>, Json(b): Json<PushRegisterBody>) -> StatusCode {
    st.push.add(&b.token);
    StatusCode::OK
}

/// 완성 라우터: `/pair`,`/stream/:run_id`는 무인증(자체 토큰검증), 나머지(`/health`,`/projects`,`/diff`,`/awake`,`/chat`,`/status`,`/cancel`,`/preflight`,`/push/register`,`/sessions`,`/transcript`)는 `require_token` 미들웨어 적용.
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
        .route("/push/register", post(push_register_handler))
        .route("/sessions/{project}", get(sessions_handler))
        .route("/transcript/{project}/{session_id}", get(transcript_handler))
        .layer(from_fn_with_state(state.devices.clone(), crate::auth::require_token));
    // 무인증(자체 토큰검증): /pair, /stream/:run_id(?token=<t> 쿼리로 WS 업그레이드 전 검증)
    axum::Router::new()
        .route("/pair", get(crate::pairing::pair_handler))
        .route("/stream/{run_id}", get(crate::ws::stream_handler))
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
            push: crate::push::PushStore::load(&std::env::temp_dir().join("awb_routes_test_push.json").to_string_lossy().to_string()),
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

    #[test]
    fn mark_done_if_absent_writes_when_missing() {
        let log = tmp("awb_routes_test_cancel_missing.log");
        let done_path = format!("{log}.done");
        let _ = std::fs::remove_file(&done_path);
        mark_done_if_absent(&log, 143);
        let content = std::fs::read_to_string(&done_path).unwrap();
        assert_eq!(content, "143");
        let _ = std::fs::remove_file(&done_path);
    }

    #[test]
    fn mark_done_if_absent_does_not_clobber_existing() {
        let log = tmp("awb_routes_test_cancel_existing.log");
        let done_path = format!("{log}.done");
        std::fs::write(&done_path, "0").unwrap();
        mark_done_if_absent(&log, 143);
        let content = std::fs::read_to_string(&done_path).unwrap();
        assert_eq!(content, "0");
        let _ = std::fs::remove_file(&done_path);
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
