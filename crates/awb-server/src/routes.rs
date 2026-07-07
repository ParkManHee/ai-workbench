// crates/awb-server/src/routes.rs (Task 3에서 최소 선언, Task 4에서 /projects 추가, Task 6에서 확장)
use std::sync::{Arc, Mutex};
use crate::auth::DeviceStore;
use crate::pairing::PairingCode;
use axum::extract::Query;
use axum::{extract::State, Json};
use serde::Serialize;

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

// 이 태스크의 router(): /projects, /diff 만(인증 미적용 — Task 6에서 완성 라우터로 대체)
pub fn router(state: AppState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/projects", get(projects_handler))
        .route("/diff", get(diff_handler))
        .with_state(state)
}

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
