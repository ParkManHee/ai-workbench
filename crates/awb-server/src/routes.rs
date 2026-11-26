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
    /// 폰에서 올린 첨부 이미지 저장 디렉터리(경로는 서버가 결정 — 클라이언트 주입 불가)
    pub uploads_dir: String,
    /// 실행 중 도착한 후속 턴 큐(프로젝트별) — 현재 턴 종료 시 자동 실행
    pub queue: crate::queue::TurnQueue,
    /// 권한 승인 대기(승인 모드 실행의 permission prompt 릴레이)
    pub perms: crate::perm::PermStore,
    /// MCP 릴레이 → /permission/request 호출용 내부 시크릿(데몬 시작 시 생성)
    pub perm_secret: String,
    /// 데몬 자신의 base url — 스폰한 MCP 스크립트에 전달
    pub base_url: String,
    /// 데몬 시작 시각(epoch초) — /info uptime용
    pub started_at: u64,
    /// 마지막으로 관측한 사용량 한도(run 로그의 rate_limit_event) — /info 로 앱에 싣는다
    pub rate_limit: Arc<Mutex<Option<crate::streamevt::RateLimit>>>,
    /// 한도 리셋 때 이어가기 예약
    pub resume: crate::resume::ResumeStore,
    /// 요일·시각 반복 실행 예약
    pub schedules: crate::schedule::ScheduleStore,
}

/// 폰↔데몬 프로토콜 버전 — 폰이 자신이 아는 버전과 비교해 업데이트 배너를 띄운다.
/// (호환이 깨지는 API 변경 시 올린다: 2 = 턴큐잉/권한승인/기기관리 추가)
pub const API_VERSION: u32 = 2;

/// 로그 경로에서 run_id 파생(파일명 stem) — chat/queue 공용.
pub fn run_id_of(log: &str) -> String {
    log.rsplit('/').next().unwrap_or(log).trim_end_matches(".log").to_string()
}

#[derive(Serialize)]
pub struct ProjectDto {
    pub name: String,
    pub path: String,
    pub last_activity: u64,
    pub badge: Option<awb_core::worklog::Badge>,
    /// "working"(🟢 에이전트 동작중) | "waiting"(🔴 질문 대기) | None
    pub agent_status: Option<String>,
    /// 앱에 보이는 세션(/sessions 와 같은 기준, 보관 제외) — 프로젝트 화면이 프로젝트 아래에 붙인다. 옛 앱은 이 필드를 무시한다
    pub sessions: Vec<crate::sessions::SessionRow>,
}

/// 트랜스크립트를 프로젝트 수만큼 읽어 수 초 걸릴 수 있다 — 런타임 워커를 막지 않게 블로킹 풀에서.
pub async fn projects_handler(State(st): State<AppState>) -> Json<Vec<ProjectDto>> {
    let dtos = tokio::task::spawn_blocking(move || {
        project_dtos(&st, &awb_core::transcript::projects_root(), &awb_core::live::live_interactive_ids())
    }).await.unwrap_or_default();
    Json(dtos)
}

fn project_dtos(st: &AppState, tx_root: &str, live_ids: &std::collections::HashSet<String>) -> Vec<ProjectDto> {
    // 격리 트리도 origin 있는 git 루트라 스캔에 걸린다 — 그 세션은 원래 프로젝트 아래에 보이므로 따로 세우지 않는다
    let trees = crate::worktree::store(st).paths();
    awb_core::scan::scan_roots(&st.roots).into_iter().filter(|p| !trees.contains(&p.path)).map(|p| {
        let badge = awb_core::worklog::badge_for(&p.name);
        let slug = awb_core::transcript::project_slug(&p.path);
        let agent_status = awb_core::transcript::project_status(&slug);
        let sessions = crate::worktree::project_sessions(st, &p.name, &p.path, tx_root, live_ids, None, false);
        ProjectDto { name: p.name, path: p.path, last_activity: p.last_activity, badge, agent_status, sessions }
    }).collect()
}

/// 답 필요 목록 — 이 PC 의 모든 프로젝트에서 권한 요청과 질문으로 끝난(답 대기) 세션을 한 번에.
/// 앱이 PC 마다 한 번 부르고 합친다.
pub async fn needs_you_handler(State(st): State<AppState>) -> Json<serde_json::Value> {
    let v = tokio::task::spawn_blocking(move || {
        needs_you(&st, &awb_core::transcript::projects_root(), &awb_core::live::live_interactive_ids())
    }).await.unwrap_or_default();
    Json(v)
}

fn needs_you(st: &AppState, tx_root: &str, live_ids: &std::collections::HashSet<String>) -> serde_json::Value {
    let permissions: Vec<serde_json::Value> = st.perms.pending_all().into_iter()
        .map(|(id, project, tool, summary)| serde_json::json!({ "id": id, "project": project, "tool_name": tool, "summary": summary }))
        .collect();
    let waiting: Vec<serde_json::Value> = project_dtos(st, tx_root, live_ids).into_iter().flat_map(|p| {
        let (project, path) = (p.name, p.path);
        p.sessions.into_iter().filter(|s| s.info.waiting).map(move |s| {
            let mut v = serde_json::to_value(&s).unwrap_or_default();
            v["project"] = project.clone().into();
            v["path"] = path.clone().into();
            v
        })
    }).collect();
    serde_json::json!({ "permissions": permissions, "waiting": waiting })
}

#[derive(serde::Deserialize)]
pub struct DiffQuery { pub path: String, #[serde(default)] pub file: Option<String>, #[serde(default)] pub scope: Option<String>, #[serde(default)] pub run: Option<String> }

/// file 없으면 변경 요약, file 있으면 해당 파일의 unified diff(폰에서 파일 행 탭 시).
/// scope=turn&run=<id> 면 그 run 의 시작·끝 스냅샷 사이(그 턴의 변경)만.
pub async fn diff_handler(State(st): State<AppState>, Query(q): Query<DiffQuery>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if q.scope.as_deref() == Some("turn") {
        let (t0, t1) = q.run.as_deref().and_then(|r| crate::gitdiff::turn_trees(&st.runs_dir, r))
            .ok_or((StatusCode::NOT_FOUND, "이 run 의 스냅샷이 없습니다".into()))?;
        return Ok(Json(match q.file {
            Some(f) => serde_json::json!({ "file": f, "diff": crate::gitdiff::file_diff_turn(&q.path, &t0, &t1, &f) }),
            None => serde_json::to_value(crate::gitdiff::summarize_turn(&q.path, &t0, &t1)).unwrap_or_default(),
        }));
    }
    Ok(match q.file {
        Some(f) => Json(serde_json::json!({ "file": f, "diff": crate::gitdiff::file_diff(&q.path, &f) })),
        None => Json(serde_json::to_value(crate::gitdiff::summarize(&q.path)).unwrap_or_default()),
    })
}

fn project_path(st: &AppState, project: &str) -> Result<String, (StatusCode, String)> {
    awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).map(|p| p.path)
        .ok_or((StatusCode::NOT_FOUND, "unknown project".into()))
}

#[derive(serde::Deserialize)]
pub struct GitQuery { #[serde(default)] pub session: Option<String> }

/// session 이 격리 세션이면 그 트리, 아니면(없거나 일반 세션) 원래 트리. 경로는 서버 매핑에서만 — 앱이 경로를 주입하지 않는다
fn git_path(st: &AppState, project: &str, q: &GitQuery) -> Result<String, (StatusCode, String)> {
    let path = project_path(st, project)?;
    let wt = crate::worktree::for_resume(st, project, q.session.as_deref()).map_err(|e| (StatusCode::CONFLICT, e))?;
    Ok(wt.map_or(path, |w| w.path))
}

/// git/gh 는 수 초~수십 초 걸린다(push·PR) — 런타임 워커를 막지 않게 블로킹 풀에서.
async fn git_blocking(f: impl FnOnce() -> Result<String, (StatusCode, String)> + Send + 'static) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let out = tokio::task::spawn_blocking(f).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    Ok(Json(serde_json::json!({ "output": out })))
}

#[derive(serde::Deserialize)]
pub struct CommitBody { pub message: String, #[serde(default)] pub branch: Option<String> }

pub async fn git_commit_handler(State(st): State<AppState>, Path(project): Path<String>, Query(gq): Query<GitQuery>, Json(b): Json<CommitBody>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = git_path(&st, &project, &gq)?;
    git_blocking(move || crate::gitdiff::commit(&path, &b.message, b.branch.as_deref())).await
}

pub async fn git_push_handler(State(st): State<AppState>, Path(project): Path<String>, Query(gq): Query<GitQuery>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = git_path(&st, &project, &gq)?;
    git_blocking(move || crate::gitdiff::push(&path)).await
}

#[derive(serde::Deserialize)]
pub struct PrBody { pub title: String, #[serde(default)] pub body: String }

pub async fn git_pr_handler(State(st): State<AppState>, Path(project): Path<String>, Query(gq): Query<GitQuery>, Json(b): Json<PrBody>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = git_path(&st, &project, &gq)?;
    git_blocking(move || crate::gitdiff::pr(&path, &b.title, &b.body)).await
}

/// 커밋 메시지 초안(haiku 한 줄) + 기본 브랜치 여부(앱이 브랜치 이름 칸을 띄울지). 초안 실패는 빈 문자열.
pub async fn git_draft_handler(State(st): State<AppState>, Path(project): Path<String>, Query(gq): Query<GitQuery>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = git_path(&st, &project, &gq)?;
    let (input, on_default) = tokio::task::spawn_blocking(move || (crate::gitdiff::draft_input(&path), crate::gitdiff::on_default_branch(&path)))
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut message = String::new();
    if let Some(input) = input {
        let prompt = format!("아래 git 변경을 한국어 한 줄 커밋 메시지로 요약해라. 마크다운·백틱 없이 평문 한 줄만 출력한다.\n\n{input}");
        // 프로젝트 맥락·도구 없이 요약만 — 임시 디렉터리에서, 세션도 남기지 않는다
        let fut = tokio::process::Command::new(&st.claude_bin)
            .args(["-p", &prompt, "--model", "haiku", "--tools", "", "--no-session-persistence"])
            .current_dir(std::env::temp_dir()).kill_on_drop(true).output();
        if let Ok(Ok(o)) = tokio::time::timeout(std::time::Duration::from_secs(60), fut).await {
            if o.status.success() {
                message = crate::gitdiff::clean_draft(&String::from_utf8_lossy(&o.stdout));
            }
        }
    }
    Ok(Json(serde_json::json!({ "message": message, "on_default": on_default })))
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

/// `claude --effort` 허용값(claude --help 기준)
const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// `claude --permission-mode` 로 그대로 넘기는 값 — 감독·편집 자동·Auto·전체. 마지막만 승인 중계 없이 돈다
const PERMISSION_MODES: [&str; 4] = ["default", "acceptEdits", "auto", "bypassPermissions"];
pub(crate) fn default_mode() -> String { "auto".into() }

#[derive(serde::Deserialize)]
pub struct ChatBody { pub prompt: String, #[serde(default)] pub plan: bool, #[serde(default)] pub resume_session_id: Option<String>, #[serde(default = "default_mode")] pub mode: String, #[serde(default)] pub model: Option<String>, #[serde(default)] pub effort: Option<String>, #[serde(default)] pub new_session: bool,
    /// 새 대화 첫 메시지에서만 — 원래 트리 옆 격리 트리(claude/phone-<id>)를 만들어 거기서 돈다. 옛 앱은 보내지 않는다
    #[serde(default)] pub isolate: bool }
#[derive(Serialize)]
pub struct ChatResult {
    pub run_id: Option<String>,
    pub log: Option<String>,
    /// true면 실행 중이라 큐에 적재됨(position = 대기 순번). 현재 턴 종료 시 자동 실행.
    pub queued: bool,
    pub position: Option<usize>,
    /// 격리 트리에서 돌면 그 트리 — 앱이 변경 보기·이어받기 경로와 브랜치 표시에 쓴다
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<crate::worktree::Worktree>,
    /// 터미널 세션의 채널로 넣었다 — 실행(run)이 없고 답은 그 세션 대화 파일로 온다
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub channel: bool,
}

/// 터미널에 열려 있고 폰 채널이 붙은 세션이면 그 claude pid. 다른 프로젝트의 세션으로는 보내지 않는다(cwd 대조)
fn channel_pid(st: &AppState, project: &str, repo: &str, sid: &str) -> Option<u32> {
    let e = awb_core::live::find_live(sid)?;
    let wt = crate::worktree::for_resume(st, project, Some(sid)).ok().flatten();
    let dir = wt.as_ref().map_or(repo, |w| w.path.as_str());
    (e.cwd.as_deref().is_some_and(|c| awb_core::live::same_dir(c, dir)) && crate::channel::attached(e.pid)).then_some(e.pid)
}

/// new_session 이면 저장된 마지막 세션을 쓰지 않는다 — 앱 "+ 새 대화" 의 첫 전송이 이전 대화를 잇던 것
pub fn resolve_resume(st: &AppState, project: &str, want: Option<String>, new_session: bool) -> (Option<String>, Option<String>) {
    let stored = if new_session { None } else { st.sessions.get(project) };
    crate::sessions::decide_resume(want, stored, st.sessions.forked_from(project), &awb_core::live::live_interactive_ids())
}

pub async fn chat_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<ChatBody>) -> Result<Json<ChatResult>, (StatusCode, String)> {
    // 프로젝트 경로 확인
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project)
        .ok_or((StatusCode::NOT_FOUND, "unknown project".into()))?;
    // resume_session_id가 있으면 특정 세션으로 강제 resume(폰에서 과거 세션 선택), 없으면 프로젝트별 저장된 마지막 세션 사용(new_session 이면 resume 없이 새로)
    // claude CLI가 받는 값만 통과 — 틀린 값을 조용히 무시하지 않고 거부한다
    if let Some(e) = b.effort.as_deref() {
        if !EFFORT_LEVELS.contains(&e) { return Err((StatusCode::BAD_REQUEST, format!("unknown effort: {e}"))); }
    }
    if !PERMISSION_MODES.contains(&b.mode.as_str()) { return Err((StatusCode::BAD_REQUEST, format!("unknown mode: {}", b.mode))); }
    // 진행 중인 대화를 중간에 격리로 옮기지 않는다 — 이미 원래 트리에 쓴 변경과 대화가 갈린다
    if b.isolate && b.resume_session_id.is_some() { return Err((StatusCode::BAD_REQUEST, "격리는 새 대화의 첫 메시지에서만 켤 수 있습니다".into())); }
    // 채널이 붙은 터미널 세션 — fork 하지 않고 그 터미널 대화에 넣는다. 모드·모델·계획은 그 터미널 설정을 따른다
    if let Some(sid) = b.resume_session_id.clone().filter(|s| awb_core::transcript::safe_session_id(s)) {
        let (st2, p, repo, prompt) = (st.clone(), project.clone(), proj.path.clone(), b.prompt.clone());
        let sent = tokio::task::spawn_blocking(move || channel_pid(&st2, &p, &repo, &sid).map(|pid| crate::channel::send(pid, &prompt)))
            .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match sent {
            Some(Ok(())) => return Ok(Json(ChatResult { run_id: None, log: None, queued: false, position: None, worktree: None, channel: true })),
            Some(Err(e)) => return Err((StatusCode::CONFLICT, e)),
            None => {}
        }
    }
    let (resume, fork_of) = if b.isolate { (None, None) } else { resolve_resume(&st, &project, b.resume_session_id.clone(), b.new_session) };
    let wt = if b.isolate {
        let (p, repo) = (project.clone(), proj.path.clone());
        let wt = tokio::task::spawn_blocking(move || crate::worktree::create(&p, &repo)).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_err(|e| (StatusCode::CONFLICT, format!("격리 트리를 만들지 못했습니다: {e}")))?;
        crate::worktree::store(&st).add(&wt).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("격리 트리 기록 실패({}): {e}", wt.path)))?;
        Some(wt)
    } else {
        crate::worktree::for_resume(&st, &project, resume.as_deref()).map_err(|e| (StatusCode::CONFLICT, e))?
    };
    let workdir = wt.as_ref().map_or_else(|| proj.path.clone(), |w| w.path.clone());
    let mcp_cfg = approval_mcp_cfg_for(&st, &project, &b.mode);
    let h = match awb_core::runner::start_stream_run(&st.claude_bin, &workdir, &st.settings_path, b.plan, &b.prompt, resume.as_deref(), fork_of.is_some(), &st.runs_dir, mcp_cfg.as_deref(), b.model.as_deref(), b.effort.as_deref(), &b.mode) {
        Ok(h) => h,
        // 이미 실행 중 → 409 거부 대신 큐 적재(현재 턴 종료 시 자동 실행)
        Err(e) if e.contains("이미 실행 중") => {
            let pos = st.queue.enqueue(&project, crate::queue::QueuedTurn {
                prompt: b.prompt.clone(), plan: b.plan, resume_session_id: b.resume_session_id.clone(), mode: b.mode.clone(), model: b.model.clone(), effort: b.effort.clone(), new_session: b.new_session,
            });
            return Ok(Json(ChatResult { run_id: None, log: None, queued: true, position: Some(pos), worktree: wt, channel: false }));
        }
        Err(e) => {
            // 방금 만든 빈 트리를 남기지 않는다(변경·커밋이 없을 때만 지워진다)
            if b.isolate { if let Some(w) = &wt { crate::worktree::cleanup_json(&st, w); } }
            return Err((StatusCode::CONFLICT, e));
        }
    };
    let run_id = run_id_of(&h.log);
    st.runs.insert(&run_id, crate::runreg::RunMeta { log: h.log.clone(), pgid: h.pgid, workdir, project: project.clone(), notified: false, fork_of, worktree: wt.clone() });
    crate::push::spawn_watch(st.clone(), run_id.clone()); // 완료 워처: WS 미소비 시 푸시 발송, 완료 시 락 해제도 보장
    Ok(Json(ChatResult { run_id: Some(run_id), log: Some(h.log), queued: false, position: None, worktree: wt, channel: false }))
}

pub async fn status_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<Json<awb_core::runlog::RunStatus>, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(awb_core::runlog::run_status(&meta.log, &meta.workdir)))
}

pub async fn cancel_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<StatusCode, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    let dead = awb_core::runner::cancel_run(meta.pgid, &meta.workdir);
    // SIGTERM/SIGKILL로 그룹 전체가 죽으면 래퍼가 .done을 못 쓸 수 있으므로, 취소를 관측 가능하게 직접 기록한다(128+SIGTERM=143).
    awb_core::runlog::mark_done_if_absent(&meta.log, 143);
    Ok(if dead { StatusCode::OK } else { StatusCode::ACCEPTED })
}

/// 전체(bypassPermissions)만 중계 없이 사전 허용 설정으로 돈다 — 나머지 모드는 claude 가 묻는 것을 폰으로 넘긴다
pub fn approval_mcp_cfg_for(st: &AppState, project: &str, mode: &str) -> Option<String> {
    if mode == "bypassPermissions" { None } else { write_approval_mcp_cfg(st, project) }
}

/// 승인 모드 실행용 MCP 설정 파일 생성(런마다) — claude가 이 설정으로 릴레이 MCP를 띄운다.
pub fn write_approval_mcp_cfg(st: &AppState, project: &str) -> Option<String> {
    let script = format!("{}/awb-approval-mcp.cjs", awb_core::runner::scripts_dir());
    // launchd 등 최소 PATH 환경에서도 node가 해석되도록 로그인셸 PATH에서 절대경로를 찾는다
    let node = awb_core::shell_env::which_in(&awb_core::shell_env::login_path(), "node").unwrap_or_else(|| "node".to_string());
    let cfg = serde_json::json!({
        "mcpServers": { "awb-approval": {
            "command": node,
            "args": [script],
            "env": {
                "AWB_DAEMON": st.base_url,
                "AWB_PERM_SECRET": st.perm_secret,
                "AWB_PROJECT": project,
            }
        }}
    });
    let runs_dir = awb_core::paths::expand_tilde(&st.runs_dir);
    let _ = std::fs::create_dir_all(&runs_dir);
    let path = format!("{}/approval-{}-{}.mcp.json", runs_dir, std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
    match std::fs::write(&path, cfg.to_string()) {
        Ok(_) => Some(path),
        Err(e) => { eprintln!("MCP 설정 생성 실패: {e}"); None }
    }
}

#[derive(serde::Deserialize)]
pub struct PermRequestBody { pub project: String, pub tool_name: String, pub input: serde_json::Value }

/// 앱에 주는 권한 입력 상한 — ExitPlanMode 는 계획 전문을 보고 승인해야 하므로 넉넉히 둔다(긴 계획도 수천 자).
/// 무한정이 아닌 이유: 대기 건마다 폴링(/permission/pending)·답 필요 목록 응답에 그대로 실린다.
const PERM_INPUT_MAX_CHARS: usize = 20_000;
/// 푸시 본문 상한 — 알림 한 칸에 보일 만큼만, 페이로드 한도(4KB) 안에 둔다.
const PERM_PUSH_MAX_CHARS: usize = 300;

/// (앱에 줄 입력, 푸시 본문) — 넘치면 문자 경계에서 자르고 「…」 를 붙인다.
fn perm_summaries(input: &serde_json::Value) -> (String, String) {
    let raw = input.to_string();
    let cut = |max| match crate::streamevt::truncate_chars(&raw, max) { (t, true) => format!("{t}…"), (t, false) => t };
    (cut(PERM_INPUT_MAX_CHARS), cut(PERM_PUSH_MAX_CHARS))
}

/// MCP 릴레이가 호출(내부 시크릿 인증). 폰 응답까지 대기(최대 300초) 후 allow/deny 반환.
pub async fn permission_request_handler(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(b): Json<PermRequestBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    if auth != format!("Bearer {}", st.perm_secret) { return Err(StatusCode::UNAUTHORIZED); }
    // 프로젝트당 실행은 하나(락)라 요청한 run 은 그 프로젝트의 활성 run 이다. 로그에서 세션을 못 찾으면 기억하지 않고 매번 묻는다
    let session_id = st.runs.active_for_project(&b.project)
        .and_then(|(_, m)| crate::sessions::capture_session_id_from_log(&m.log));
    if let Some(sid) = &session_id {
        if st.perms.is_session_allowed(sid, &b.tool_name) { return Ok(Json(serde_json::json!({ "allow": true }))); }
    }
    let id = crate::pairing::random_token()[..16].to_string();
    let (summary, push_body) = perm_summaries(&b.input);
    let (tx, rx) = tokio::sync::oneshot::channel();
    st.perms.insert(&id, crate::perm::PendingPerm {
        project: b.project.clone(), tool_name: b.tool_name.clone(), input_summary: summary, session_id, tx: Some(tx),
    });
    // 앱 밖에서도 승인하러 올 수 있게 푸시(딥링크 포함)
    let tokens = st.push.list();
    let data = serde_json::json!({
        "hostname": resolve_hostname(), "project": b.project, "path": "", "session": st.sessions.get(&b.project),
    });
    crate::push::send(&tokens, &format!("🔐 승인 요청 — {}", b.project), &format!("{}: {}", b.tool_name, push_body), &data);
    let d = tokio::time::timeout(std::time::Duration::from_secs(300), rx).await.ok().and_then(|r| r.ok())
        .unwrap_or(crate::perm::Decision { allow: false, message: None });
    st.perms.remove(&id); // 타임아웃 잔류 정리(정상 응답이면 이미 제거됨)
    Ok(Json(serde_json::json!({ "allow": d.allow, "message": d.message })))
}

/// 폰 폴링: 이 프로젝트의 승인 대기 목록.
pub async fn permission_pending_handler(State(st): State<AppState>, Path(project): Path<String>) -> Json<serde_json::Value> {
    let items: Vec<serde_json::Value> = st.perms.pending_for(&project).into_iter()
        .map(|(id, tool, summary)| serde_json::json!({ "id": id, "tool_name": tool, "summary": summary }))
        .collect();
    Json(serde_json::json!({ "pending": items }))
}

#[derive(serde::Deserialize)]
pub struct PermAnswerBody { pub id: String, pub allow: bool, #[serde(default)] pub for_session: bool, #[serde(default)] pub message: Option<String> }

/// 폰의 승인/거부 — 대기 중인 MCP 요청을 깨운다.
pub async fn permission_answer_handler(State(st): State<AppState>, Json(b): Json<PermAnswerBody>) -> StatusCode {
    if st.perms.answer(&b.id, b.allow, b.for_session, b.message) { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

/// 프로젝트의 활성 run 조회 — 폰이 재진입/타 기기에서 진행 중 실행에 attach(취소·대기표시)할 수 있게.
/// 활성 run이 없으면 200 + {run_id: null} ("없음"은 정상 상태라 404가 아님).
/// session·forked_from: 백그라운드에서 끝난 실행은 앱이 done(새 session_id)을 못 받는다 — 갈라진 세션을 여기서 찾는다
pub async fn active_run_handler(State(st): State<AppState>, Path(project): Path<String>) -> Json<serde_json::Value> {
    let run_id = st.runs.active_for_project(&project).map(|(id, _)| id);
    Json(serde_json::json!({ "run_id": run_id, "queued": st.queue.len(&project),
        "session": st.sessions.get(&project), "forked_from": st.sessions.forked_from(&project) }))
}

#[derive(Serialize, PartialEq, Debug)]
pub struct DevServer { pub port: u16, pub process: String, pub reachable: bool }

/// macOS 시스템 리스너 — HTTP 로 응답해도 개발 서버가 아니다(ControlCenter=AirPlay 수신 5000/7000).
const SYSTEM_PROCS: &[&str] = &["ControlCe", "rapportd"];

/// lsof LISTEN 출력 파싱(순수) — 폰에서 열 수 있는 개발 서버 후보.
/// reachable: 0.0.0.0/'*' 바인딩(=Tailscale로 접근 가능). 127.0.0.1은 --host 필요 안내용으로 포함.
pub fn parse_dev_servers(lsof_out: &str, exclude_ports: &[u16]) -> Vec<DevServer> {
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for line in lsof_out.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 9 { continue; }
        // NAME 컬럼("*:8081" / "127.0.0.1:8787" / "[::1]:5000")은 "(LISTEN)" 바로 앞
        let addr_port = if *cols.last().unwrap_or(&"") == "(LISTEN)" { cols[cols.len() - 2] } else { cols[cols.len() - 1] };
        let Some(idx) = addr_port.rfind(':') else { continue };
        let (addr, port_s) = addr_port.split_at(idx);
        let Ok(port) = port_s[1..].parse::<u16>() else { continue };
        if exclude_ports.contains(&port) || !(1024..=49151).contains(&port) { continue; }
        if SYSTEM_PROCS.contains(&cols[0]) { continue; }
        let reachable = addr == "*" || addr == "0.0.0.0" || addr == "[::]";
        // IPv4/IPv6 로컬호스트는 안내용으로만
        if !reachable && !(addr.starts_with("127.") || addr == "[::1]") { continue; }
        if !seen.insert(port) { continue; }
        out.push(DevServer { port, process: cols[0].to_string(), reachable });
    }
    out.sort_by_key(|d| d.port);
    out
}

/// Mac에서 리슨 중인 개발 서버 목록 — 폰 프리뷰(브라우저 오픈)용.
pub async fn devservers_handler(State(st): State<AppState>) -> Json<Vec<DevServer>> {
    let port: u16 = st.base_url.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(8787);
    let out = std::process::Command::new("lsof").args(["-iTCP", "-sTCP:LISTEN", "-P", "-n"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let cands = parse_dev_servers(&out, &[port, 5037]);
    let probed = tokio::task::spawn_blocking(move || {
        std::thread::scope(|sc| {
            let hs: Vec<_> = cands.into_iter().map(|d| sc.spawn(move || speaks_http(d.port).then_some(d))).collect();
            hs.into_iter().filter_map(|h| h.join().ok().flatten()).collect::<Vec<_>>()
        })
    }).await.unwrap_or_default();
    Json(probed)
}

/// HTTP 로 응답하는 포트만 남긴다 — DB·메시지큐(docker MySQL 3306·RabbitMQ 5672)가 개발 서버로 올라와
/// 폰에서 열면 빈 화면이 됐다(2026-09-23 실측). 400ms 는 로컬 루프백 응답에 충분한 상한.
fn speaks_http(port: u16) -> bool {
    use std::io::{Read, Write};
    let t = std::time::Duration::from_millis(400);
    for ip in [std::net::IpAddr::from([127, 0, 0, 1]), std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST)] {
        let Ok(mut s) = std::net::TcpStream::connect_timeout(&(ip, port).into(), t) else { continue };
        let _ = s.set_read_timeout(Some(t));
        let _ = s.set_write_timeout(Some(t));
        let mut buf = [0u8; 5];
        return s.write_all(b"HEAD / HTTP/1.0\r\nHost: localhost\r\n\r\n").is_ok() && s.read_exact(&mut buf).is_ok() && &buf == b"HTTP/";
    }
    false
}

#[derive(Serialize)]
pub struct DeviceDto { pub id: String, pub label: String, pub paired_at: u64 }

/// 페어링된 기기 목록(해시 제외) — 분실 기기 확인·회수용.
pub async fn devices_handler(State(st): State<AppState>) -> Json<Vec<DeviceDto>> {
    Json(st.devices.list().into_iter().map(|d| DeviceDto { id: d.id, label: d.label, paired_at: d.paired_at }).collect())
}

/// 기기 토큰 회수 — 해당 기기는 즉시 401(재페어링 필요).
pub async fn device_revoke_handler(State(st): State<AppState>, Path(id): Path<String>) -> StatusCode {
    if st.devices.remove(&id) { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

pub async fn preflight_handler(State(st): State<AppState>) -> Json<awb_core::preflight::Preflight> {
    Json(awb_core::preflight::run_preflight(&st.roots, Some(st.claude_bin.clone())))
}

#[derive(Serialize)]
pub struct InfoDto {
    pub hostname: String,
    pub version: String,
    pub api_version: u32,
    pub uptime_secs: u64,
    pub active_runs: usize,
    pub rate_limit: Option<crate::streamevt::RateLimit>,
}

/// macOS 친숙한 이름(`scutil --get ComputerName`) → 실패 시 `hostname` 명령 → 최종 폴백 `"Mac"`.
/// 폰이 여러 PC를 페어링했을 때 목록에 표시할 라벨로 사용한다.
pub fn resolve_hostname() -> String {
    let scutil = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output();
    if let Ok(o) = scutil {
        if o.status.success() {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !name.is_empty() { return name; }
        }
    }
    let hostname = std::process::Command::new("hostname").output();
    if let Ok(o) = hostname {
        if o.status.success() {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !name.is_empty() { return name; }
        }
    }
    "Mac".to_string()
}

pub async fn info_handler(State(st): State<AppState>) -> Json<InfoDto> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Json(InfoDto {
        hostname: resolve_hostname(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: API_VERSION,
        uptime_secs: now.saturating_sub(st.started_at),
        active_runs: st.runs.count(),
        rate_limit: st.rate_limit.lock().unwrap().clone(),
    })
}

pub async fn resume_get_handler(State(st): State<AppState>, Path(project): Path<String>) -> Json<Option<crate::resume::Reservation>> {
    Json(st.resume.get(&project))
}

/// 프로젝트당 1개 — 다시 걸면 덮어쓴다. 값 검증은 실행 시점 /chat 과 같은 기준으로 미리 한다.
pub async fn resume_set_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<crate::resume::Reservation>) -> Result<StatusCode, (StatusCode, String)> {
    project_path(&st, &project)?;
    if let Some(e) = b.effort.as_deref() {
        if !EFFORT_LEVELS.contains(&e) { return Err((StatusCode::BAD_REQUEST, format!("unknown effort: {e}"))); }
    }
    if !PERMISSION_MODES.contains(&b.mode.as_str()) { return Err((StatusCode::BAD_REQUEST, format!("unknown mode: {}", b.mode))); }
    st.resume.set(&project, b);
    Ok(StatusCode::OK)
}

pub async fn resume_delete_handler(State(st): State<AppState>, Path(project): Path<String>) -> StatusCode {
    if st.resume.remove(&project) { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

pub async fn schedules_list_handler(State(st): State<AppState>, Path(project): Path<String>) -> Json<Vec<crate::schedule::Schedule>> {
    Json(st.schedules.list(&project))
}

#[derive(serde::Deserialize)]
pub struct NewSchedule { pub days: Vec<u8>, pub time: String, pub prompt: String, #[serde(default = "default_mode")] pub mode: String, #[serde(default)] pub model: Option<String>, #[serde(default)] pub effort: Option<String> }

/// 값 검증은 실행 시점 /chat 과 같은 기준으로 미리 한다 — 틀린 예약이 그 시각에 조용히 실패하지 않게.
pub async fn schedule_add_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<NewSchedule>) -> Result<Json<crate::schedule::Schedule>, (StatusCode, String)> {
    project_path(&st, &project)?;
    let mut days = b.days;
    days.sort_unstable();
    days.dedup();
    if days.is_empty() || days.iter().any(|d| *d > 6) { return Err((StatusCode::BAD_REQUEST, "days: 0(일)~6(토) 중 하나 이상".into())); }
    if b.prompt.trim().is_empty() { return Err((StatusCode::BAD_REQUEST, "empty prompt".into())); }
    if let Some(e) = b.effort.as_deref() {
        if !EFFORT_LEVELS.contains(&e) { return Err((StatusCode::BAD_REQUEST, format!("unknown effort: {e}"))); }
    }
    if !PERMISSION_MODES.contains(&b.mode.as_str()) { return Err((StatusCode::BAD_REQUEST, format!("unknown mode: {}", b.mode))); }
    let next_at = crate::schedule::next_local(&days, &b.time, crate::schedule::now())
        .ok_or((StatusCode::BAD_REQUEST, format!("time: HH:MM 이어야 한다: {}", b.time)))?;
    let s = crate::schedule::Schedule {
        id: crate::pairing::random_token()[..12].to_string(), project, days, time: b.time, prompt: b.prompt, mode: b.mode, model: b.model, effort: b.effort, next_at,
    };
    st.schedules.add(s.clone());
    Ok(Json(s))
}

pub async fn schedule_delete_handler(State(st): State<AppState>, Path((project, id)): Path<(String, String)>) -> StatusCode {
    if st.schedules.remove(&project, &id) { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

#[derive(serde::Deserialize)]
pub struct SessionsQuery { pub q: Option<String>, #[serde(default)] pub archived: u8 }

pub async fn sessions_handler(State(st): State<AppState>, Path(project): Path<String>, Query(sq): Query<SessionsQuery>) -> Result<Json<Vec<crate::sessions::SessionRow>>, StatusCode> {
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(crate::worktree::project_sessions(&st, &project, &proj.path, &awb_core::transcript::projects_root(), &awb_core::live::live_interactive_ids(), sq.q.as_deref(), sq.archived == 1)))
}

#[derive(serde::Deserialize)]
pub struct SessionMetaBody {
    #[serde(default, deserialize_with = "crate::sessions::present")]
    pub name: Option<Option<String>>,
    pub archived: Option<bool>,
}

/// 세션 이름·보관 — 데몬 메타 파일에만 쓰고 트랜스크립트는 건드리지 않는다.
/// 격리 세션을 보관하면 그 트리를 정리하고(변경·새 커밋이 있으면 남긴다) 결과를 worktree 에 싣는다.
pub async fn session_meta_handler(State(st): State<AppState>, Path((project, session_id)): Path<(String, String)>, Json(b): Json<SessionMetaBody>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !awb_core::transcript::safe_session_id(&session_id) { return Err(StatusCode::BAD_REQUEST); }
    awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    let archiving = b.archived == Some(true);
    let m = st.sessions.set_meta(&project, &session_id, b.name, b.archived).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut v = serde_json::to_value(&m).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let (true, Ok(Some(wt))) = (archiving, crate::worktree::for_resume(&st, &project, Some(&session_id))) {
        let st2 = st.clone();
        v["worktree"] = tokio::task::spawn_blocking(move || crate::worktree::cleanup_json(&st2, &wt)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(Json(v))
}

#[derive(serde::Deserialize)]
pub struct TxQuery { #[serde(default)] pub from: usize, pub tail: Option<u8>, pub until: Option<usize>, pub limit: Option<usize> }

pub async fn transcript_handler(State(st): State<AppState>, Path((project, session_id)): Path<(String, String)>, Query(q): Query<TxQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !awb_core::transcript::safe_session_id(&session_id) { return Err(StatusCode::BAD_REQUEST); }
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    let path = crate::worktree::transcript_file(&st, &project, &proj.path, &session_id);
    if q.tail == Some(1) {
        // 최초 로드: 최근 1시간(최대 limit, 기본 100), 없으면 마지막 20개 폴백
        let page = awb_core::transcript::read_transcript_page(&path, None, q.limit.unwrap_or(100), 3600);
        return Ok(Json(serde_json::json!({ "messages": page.messages, "prev": page.prev, "next": page.next, "active": page.active })));
    }
    if let Some(u) = q.until {
        // 위로 스크롤: u 이전 메시지 마지막 limit(기본 50)개
        let page = awb_core::transcript::read_transcript_page(&path, Some(u), q.limit.unwrap_or(50), 3600);
        return Ok(Json(serde_json::json!({ "messages": page.messages, "prev": page.prev, "next": page.next, "active": page.active })));
    }
    let (msgs, next, active) = awb_core::transcript::read_transcript(&path, q.from);
    Ok(Json(serde_json::json!({ "messages": msgs, "next": next, "active": active })))
}

/// PC 터미널에서 열린 대화형 세션을 앱에서 끝낸다. 레지스트리·시작시각·cwd 가 모두 맞는 pid 에만 신호를 보낸다.
/// 격리 세션이면 (터미널에 열려 있지 않아도) 끝난 뒤 그 트리를 정리하고 결과를 worktree 에 싣는다.
pub async fn session_end_handler(State(st): State<AppState>, Path((project, session_id)): Path<(String, String)>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !awb_core::transcript::safe_session_id(&session_id) { return Err(StatusCode::BAD_REQUEST); }
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    let wt = crate::worktree::for_resume(&st, &project, Some(&session_id)).ok().flatten();
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || {
        let dir = wt.as_ref().map_or(proj.path.as_str(), |w| w.path.as_str());
        let term = awb_core::live::find_live(&session_id).and_then(|e| {
            // 다른 프로젝트의 세션을 이 경로로 끄지 못하게
            if !e.cwd.as_deref().is_some_and(|c| awb_core::live::same_dir(c, dir)) { return None; }
            awb_core::live::terminate(e.pid, &e.proc_start, awb_core::live::proc_lstart).map(|ended| (ended, e.pid))
        });
        let Some(wt) = wt else {
            let (ended, pid) = term.ok_or(StatusCode::NOT_FOUND)?;
            return Ok(Json(serde_json::json!({ "ended": ended, "pid": pid })));
        };
        // 터미널에 열려 있지 않으면 끝낼 프로세스가 없다 = 이미 끝남
        let ended = term.is_none_or(|(e, _)| e);
        let tree = if ended { crate::worktree::cleanup_json(&st2, &wt) }
            else { serde_json::json!({ "path": wt.path, "branch": wt.branch, "removed": false, "reason": "터미널 claude 가 아직 종료 중" }) };
        Ok(Json(serde_json::json!({ "ended": ended, "pid": term.map(|(_, p)| p), "worktree": tree })))
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

#[derive(serde::Deserialize)]
pub struct UploadQuery { pub ext: String }

const UPLOAD_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];
pub const UPLOAD_LIMIT_BYTES: usize = 15 * 1024 * 1024;

/// 폰 첨부 이미지 업로드: raw bytes → uploads_dir에 저장, 절대경로 반환.
/// 파일명은 서버가 생성(시각-pid-순번.확장자) — 클라이언트가 경로를 주입할 수 없다.
pub async fn upload_handler(State(st): State<AppState>, Query(q): Query<UploadQuery>, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ext = q.ext.to_ascii_lowercase();
    if !UPLOAD_EXTS.contains(&ext.as_str()) {
        return Err((StatusCode::BAD_REQUEST, format!("허용되지 않는 확장자: {ext} (jpg/jpeg/png/webp)")));
    }
    // raw bytes 수신. 클라이언트는 픽커 base64 → Uint8Array 본문(expo/fetch WinterCG).
    // multipart는 쓰지 않는다: expo 전역 fetch가 RN식 {uri,...} FormData 파트를 지원하지 않고
    // ("unsupported FormData part implementation"), RN fetch는 로컬 uri의 blob()을 읽지 못한다.
    if body.is_empty() { return Err((StatusCode::BAD_REQUEST, "빈 본문".into())); }
    std::fs::create_dir_all(&st.uploads_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("업로드 디렉터리 생성 실패: {e}")))?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = format!("{}/{}-{}-{}.{}", st.uploads_dir, secs, std::process::id(), seq, ext);
    std::fs::write(&path, &body).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("저장 실패: {e}")))?;
    Ok(Json(serde_json::json!({ "path": path })))
}

#[derive(serde::Deserialize)]
pub struct PushRegisterBody { pub token: String }

pub async fn push_register_handler(State(st): State<AppState>, Json(b): Json<PushRegisterBody>) -> StatusCode {
    st.push.add(&b.token);
    StatusCode::OK
}

/// 완성 라우터: `/pair`,`/stream/:run_id`는 무인증(자체 토큰검증), 나머지(`/health`,`/projects`,`/diff`,`/awake`,`/chat`,`/status`,`/cancel`,`/preflight`,`/push/register`,`/sessions`,`/transcript`,`/info`)는 `require_token` 미들웨어 적용.
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
        .route("/runs/active/{project}", get(active_run_handler))
        .route("/permission/pending/{project}", get(permission_pending_handler))
        .route("/needs-you", get(needs_you_handler))
        .route("/devices", get(devices_handler))
        .route("/devservers", get(devservers_handler))
        .route("/devices/{id}/revoke", post(device_revoke_handler))
        .route("/permission/answer", post(permission_answer_handler))
        .route("/cancel/{run_id}", post(cancel_handler))
        .route("/preflight", get(preflight_handler))
        .route("/push/register", post(push_register_handler))
        .route("/sessions/{project}", get(sessions_handler))
        .route("/sessions/{project}/{session_id}/end", post(session_end_handler))
        .route("/sessions/{project}/{session_id}/meta", post(session_meta_handler))
        .route("/transcript/{project}/{session_id}", get(transcript_handler))
        .route("/info", get(info_handler))
        .route("/resume-at-reset/{project}", get(resume_get_handler).post(resume_set_handler).delete(resume_delete_handler))
        .route("/schedules/{project}", get(schedules_list_handler).post(schedule_add_handler))
        .route("/schedules/{project}/{id}", axum::routing::delete(schedule_delete_handler))
        .route("/git/{project}/commit", post(git_commit_handler))
        .route("/git/{project}/push", post(git_push_handler))
        .route("/git/{project}/pr", post(git_pr_handler))
        .route("/git/{project}/draft", get(git_draft_handler))
        // 이미지 업로드: 기본 body 한도(2MB)를 사진 크기에 맞게 상향
        .route("/upload", post(upload_handler).layer(axum::extract::DefaultBodyLimit::max(UPLOAD_LIMIT_BYTES)))
        .layer(from_fn_with_state(state.devices.clone(), crate::auth::require_token));
    // 무인증(자체 토큰검증): /pair, /stream/:run_id(?token=<t> 쿼리로 WS 업그레이드 전 검증)
    axum::Router::new()
        .route("/pair", get(crate::pairing::pair_handler))
        .route("/permission/request", post(permission_request_handler)) // 내부 시크릿 자체 검증
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
            uploads_dir: std::env::temp_dir().join("awb_routes_test_uploads").to_string_lossy().to_string(),
            queue: crate::queue::TurnQueue::new(),
            perms: crate::perm::PermStore::new(),
            perm_secret: "test-perm-secret".to_string(),
            base_url: "http://127.0.0.1:0".to_string(),
            started_at: 0,
            rate_limit: Default::default(),
            resume: crate::resume::ResumeStore::load(&std::env::temp_dir().join("awb_routes_test_resume.json").to_string_lossy().to_string()),
            schedules: crate::schedule::ScheduleStore::load(&std::env::temp_dir().join("awb_routes_test_schedules.json").to_string_lossy().to_string()),
        }
    }

    #[test]
    fn parse_dev_servers_filters_and_flags_reachability() {
        let sample = concat!(
            "COMMAND     PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME\n",
            "node      45033   mh   25u  IPv4 0x1234      0t0  TCP *:8081 (LISTEN)\n",
            "vite      12345   mh   30u  IPv4 0x5678      0t0  TCP 127.0.0.1:5173 (LISTEN)\n",
            "awb-serve  7735   mh   10u  IPv4 0x9abc      0t0  TCP 100.119.50.46:8787 (LISTEN)\n",
            "rapportd    600   mh    4u  IPv4 0xdef0      0t0  TCP *:49500 (LISTEN)\n",
            "ControlCe   700   mh    9u  IPv4 0xdef1      0t0  TCP *:5000 (LISTEN)\n",
            "adb        1111   mh    8u  IPv4 0x1111      0t0  TCP 127.0.0.1:5037 (LISTEN)\n",
        );
        let v = crate::routes::parse_dev_servers(sample, &[8787, 5037]);
        assert_eq!(v.len(), 2, "{v:?}"); // 8081(reachable) + 5173(localhost 안내) — 8787/5037/49500(범위밖)/5000(시스템) 제외
        assert_eq!(v[0], crate::routes::DevServer { port: 5173, process: "vite".into(), reachable: false });
        assert_eq!(v[1], crate::routes::DevServer { port: 8081, process: "node".into(), reachable: true });
    }

    #[test]
    fn speaks_http_rejects_non_http_listeners() {
        use std::io::{Read, Write};
        // 한 번 받아 응답하는 리스너: HTTP 서버 흉내 / MySQL 처럼 접속 즉시 인사말을 보내는 서버 흉내
        fn serve(reply: &'static [u8]) -> u16 {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = l.local_addr().unwrap().port();
            std::thread::spawn(move || if let Ok((mut c, _)) = l.accept() {
                let mut b = [0u8; 64]; let _ = c.set_read_timeout(Some(std::time::Duration::from_millis(100))); let _ = c.read(&mut b);
                let _ = c.write_all(reply);
            });
            port
        }
        assert!(super::speaks_http(serve(b"HTTP/1.1 200 OK\r\n\r\n")));
        assert!(!super::speaks_http(serve(b"J\x00\x00\x00\x0a8.0.33\x00")));
    }

    #[test]
    fn perm_summary_keeps_full_plan_for_app_and_short_push() {
        let plan = format!("# 계획\n{}", "단계 설명 ".repeat(1000)); // 6천 자 넘는 계획
        let input = serde_json::json!({ "plan": plan });
        let (app, push) = perm_summaries(&input);
        let back: serde_json::Value = serde_json::from_str(&app).expect("앱 입력은 잘리지 않은 JSON");
        assert_eq!(back["plan"], plan);
        assert!(push.ends_with('…') && push.chars().count() == PERM_PUSH_MAX_CHARS + 1);
        let (huge, _) = perm_summaries(&serde_json::json!({ "plan": "가".repeat(PERM_INPUT_MAX_CHARS) }));
        assert!(huge.ends_with('…') && huge.chars().count() == PERM_INPUT_MAX_CHARS + 1);
    }

    #[tokio::test]
    async fn permission_request_waits_for_answer() {
        let devices_path = tmp("awb_routes_devices_perm.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        // 요청 태스크: 폰 응답까지 대기
        let st2 = state.clone();
        let req = tokio::spawn(async move {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("authorization", "Bearer test-perm-secret".parse().unwrap());
            crate::routes::permission_request_handler(
                axum::extract::State(st2),
                headers,
                Json(crate::routes::PermRequestBody {
                    project: "proj".into(), tool_name: "Bash".into(), input: serde_json::json!({"command": "ls"}),
                }),
            ).await
        });
        // pending에 나타날 때까지 대기 후 허용 응답
        let mut id = None;
        for _ in 0..50 {
            let items = state.perms.pending_for("proj");
            if let Some((pid, tool, summary)) = items.into_iter().next() {
                assert_eq!(tool, "Bash");
                assert!(summary.contains("ls"));
                id = Some(pid);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let id = id.expect("pending에 요청이 나타나야 함");
        assert!(state.perms.answer(&id, true, false, None));
        let res = req.await.unwrap().unwrap();
        assert_eq!(res.0["allow"], true);
        // 잘못된 시크릿은 401
        let mut bad = axum::http::HeaderMap::new();
        bad.insert("authorization", "Bearer wrong".parse().unwrap());
        let e = crate::routes::permission_request_handler(
            axum::extract::State(state.clone()), bad,
            Json(crate::routes::PermRequestBody { project: "p".into(), tool_name: "t".into(), input: serde_json::json!({}) }),
        ).await;
        assert!(matches!(e, Err(StatusCode::UNAUTHORIZED)));
    }

    #[tokio::test]
    async fn permission_request_skips_phone_for_session_allowed_tool() {
        let devices_path = tmp("awb_routes_devices_perm_sess.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        let log = tmp("awb_routes_perm_sess.log");
        std::fs::write(&log, "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sid-perm\"}\n").unwrap();
        state.runs.insert("run-perm", crate::runreg::RunMeta { log, pgid: 0, workdir: "/tmp".into(), project: "psess".into(), notified: false, fork_of: None, worktree: None });
        let request = |tool: &'static str| {
            let st = state.clone();
            tokio::spawn(async move {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert("authorization", "Bearer test-perm-secret".parse().unwrap());
                crate::routes::permission_request_handler(axum::extract::State(st), headers,
                    Json(crate::routes::PermRequestBody { project: "psess".into(), tool_name: tool.into(), input: serde_json::json!({}) })).await
            })
        };
        async fn wait_pending(st: &AppState) -> String {
            for _ in 0..50 {
                if let Some((id, _, _)) = st.perms.pending_for("psess").into_iter().next() { return id; }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            panic!("pending에 요청이 나타나야 함");
        }
        let first = request("Bash");
        let id = wait_pending(&state).await;
        assert!(state.perms.answer(&id, true, true, None));
        assert_eq!(first.await.unwrap().unwrap().0["allow"], true);
        // 같은 세션·같은 툴: 폰에 묻지 않고 바로 허용
        let again = tokio::time::timeout(std::time::Duration::from_secs(2), request("Bash")).await
            .expect("세션 허용된 툴은 대기 없이 돌아와야 함").unwrap().unwrap();
        assert_eq!(again.0["allow"], true);
        // 다른 툴은 다시 묻고, 거절 사유가 그대로 돌아간다
        let other = request("Write");
        let id = wait_pending(&state).await;
        assert!(state.perms.answer(&id, false, false, Some("쓰지 마".into())));
        let res = other.await.unwrap().unwrap();
        assert_eq!(res.0["allow"], false);
        assert_eq!(res.0["message"], "쓰지 마");
    }

    #[test]
    fn new_session_ignores_stored_session() {
        let devices_path = tmp("awb_routes_devices_newsess.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        state.sessions.record("newsess_demo", "old-sid", None);
        assert_eq!(resolve_resume(&state, "newsess_demo", None, true), (None, None));
        assert_eq!(resolve_resume(&state, "newsess_demo", None, false).0.as_deref(), Some("old-sid"));
    }

    #[tokio::test]
    async fn active_run_reports_forked_session() {
        let devices_path = tmp("awb_routes_devices_activefork.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        state.sessions.record("activefork_demo", "fork-sid", Some("term-sid"));
        let res = active_run_handler(State(state), Path("activefork_demo".into())).await;
        assert_eq!(res.0["run_id"], serde_json::Value::Null);
        assert_eq!(res.0["session"], "fork-sid");
        assert_eq!(res.0["forked_from"], "term-sid");
    }

    #[tokio::test]
    async fn chat_rejects_unknown_permission_mode() {
        let base = std::env::temp_dir().join("awb_mode_scan"); let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("demo"); std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "init"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "remote", "add", "origin", "x"]).output().unwrap();
        let devices_path = tmp("awb_routes_devices_mode.json"); let _ = std::fs::remove_file(&devices_path);
        let mut state = test_state(base.to_str().unwrap(), &devices_path);
        state.claude_bin = "/usr/bin/false".into(); // 검사가 빠져 실행까지 가도 실제 claude 는 돌지 않게
        // 비슷하지만 다른 문자열이 전체 모드(중계 없음)로 새지 않아야 한다
        for mode in ["bypass", "BypassPermissions", "bypassPermissions ", "", "dontAsk"] {
            let body: ChatBody = serde_json::from_value(serde_json::json!({ "prompt": "hi", "mode": mode })).unwrap();
            let r = chat_handler(axum::extract::State(state.clone()), Path("demo".into()), Json(body)).await;
            assert!(matches!(r, Err((StatusCode::BAD_REQUEST, _))), "mode {mode:?} 는 400 이어야 함");
        }
    }

    #[tokio::test]
    async fn upload_without_auth_is_unauthorized() {
        let devices_path = tmp("awb_routes_devices_upload_noauth.json"); let _ = std::fs::remove_file(&devices_path);
        let app = crate::routes::router(test_state("/tmp", &devices_path));
        let res = app.oneshot(Request::builder().method("POST").uri("/upload?ext=jpg").body(Body::from("xx")).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upload_saves_bytes_and_returns_path() {
        let devices_path = tmp("awb_routes_devices_upload_ok.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        let token = "tok-upload-ok";
        state.devices.add(token, "test-device");
        let app = crate::routes::router(state);
        let res = app.oneshot(Request::builder().method("POST").uri("/upload?ext=png")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(vec![1u8, 2, 3, 4])).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let path = v["path"].as_str().unwrap();
        assert!(path.ends_with(".png"), "{path}");
        assert_eq!(std::fs::read(path).unwrap(), vec![1u8, 2, 3, 4]);
    }

    #[tokio::test]
    async fn upload_rejects_disallowed_ext() {
        let devices_path = tmp("awb_routes_devices_upload_ext.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        let token = "tok-upload-ext";
        state.devices.add(token, "test-device");
        let app = crate::routes::router(state);
        let res = app.oneshot(Request::builder().method("POST").uri("/upload?ext=sh")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from("echo pwned")).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
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

    #[test]
    fn projects_carry_visible_sessions_per_project() {
        // 세션 있는 프로젝트 · 종료된 터미널 세션만 있는 프로젝트 · 보관된 세션만 있는 프로젝트
        let base = std::env::temp_dir().join("awb_proj_sessions"); let _ = std::fs::remove_dir_all(&base);
        let tx_root = base.join("tx");
        let line = |ep: &str| format!("{{\"type\":\"user\",\"entrypoint\":\"{ep}\",\"message\":{{\"role\":\"user\",\"content\":\"질문\"}}}}\n");
        for (name, files) in [
            ("pg-has", vec![("s-app", "sdk-cli"), ("s-live", "cli"), ("s-dead", "cli")]),
            ("pg-none", vec![("s-dead2", "cli")]),
            ("pg-archived", vec![("s-arch", "sdk-cli")]),
        ] {
            let repo = base.join("repos").join(name); std::fs::create_dir_all(&repo).unwrap();
            std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "init"]).output().unwrap();
            std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "remote", "add", "origin", "x"]).output().unwrap();
            let real = std::fs::canonicalize(&repo).unwrap();
            let dir = tx_root.join(awb_core::transcript::project_slug(real.to_str().unwrap()));
            std::fs::create_dir_all(&dir).unwrap();
            for (sid, ep) in files { std::fs::write(dir.join(format!("{sid}.jsonl")), line(ep)).unwrap(); }
        }
        let devices_path = tmp("awb_routes_devices_proj_sessions.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state(base.join("repos").to_str().unwrap(), &devices_path);
        state.sessions.set_meta("pg-archived", "s-arch", None, Some(true)).unwrap();
        let live: std::collections::HashSet<String> = ["s-live".to_string()].into();
        let v = serde_json::to_value(super::project_dtos(&state, tx_root.to_str().unwrap(), &live)).unwrap();
        let ids = |name: &str| -> Vec<String> {
            let p = v.as_array().unwrap().iter().find(|p| p["name"] == name).unwrap_or_else(|| panic!("{name} 없음: {v}"));
            let mut ids: Vec<String> = p["sessions"].as_array().unwrap().iter().map(|s| s["session_id"].as_str().unwrap().to_string()).collect();
            ids.sort(); ids
        };
        assert_eq!(ids("pg-has"), vec!["s-app", "s-live"], "열린 터미널·앱 대화만, 종료된 터미널 세션은 빠진다");
        assert_eq!(ids("pg-none"), Vec::<String>::new());
        assert_eq!(ids("pg-archived"), Vec::<String>::new(), "보관된 세션은 0개로 친다");
    }

    #[test]
    fn needs_you_lists_permissions_and_question_sessions_only() {
        // 질문으로 끝난 앱 대화 · 완료 보고로 끝난 앱 대화 · 대기 중인 권한 요청 하나
        let base = std::env::temp_dir().join("awb_needs_you"); let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repos").join("ny-proj"); std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "init"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "remote", "add", "origin", "x"]).output().unwrap();
        let real = std::fs::canonicalize(&repo).unwrap();
        let tx_root = base.join("tx");
        let dir = tx_root.join(awb_core::transcript::project_slug(real.to_str().unwrap())); std::fs::create_dir_all(&dir).unwrap();
        let convo = |answer: &str| format!(concat!(
            "{{\"type\":\"user\",\"entrypoint\":\"sdk-cli\",\"message\":{{\"role\":\"user\",\"content\":\"해줘\"}}}}\n",
            "{{\"type\":\"assistant\",\"entrypoint\":\"sdk-cli\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"{a}\"}}]}}}}\n"), a = answer);
        // 90초 「작업 중」 창을 지나 턴이 끝난 상태로 — mtime 을 10분 전으로
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        for (sid, answer) in [("s-ask", "A안과 B안 중 어느 쪽으로 할까요?"), ("s-done", "완료했습니다.")] {
            let f = dir.join(format!("{sid}.jsonl")); std::fs::write(&f, convo(answer)).unwrap();
            std::fs::File::options().write(true).open(&f).unwrap().set_modified(old).unwrap();
        }
        let devices_path = tmp("awb_routes_devices_needs_you.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state(base.join("repos").to_str().unwrap(), &devices_path);
        state.perms.insert("perm-1", crate::perm::PendingPerm { project: "ny-proj".into(), tool_name: "Write".into(), input_summary: "{}".into(), session_id: None, tx: None });
        let v = super::needs_you(&state, tx_root.to_str().unwrap(), &Default::default());
        assert_eq!(v["permissions"].as_array().unwrap().len(), 1, "{v}");
        assert_eq!(v["permissions"][0]["project"], "ny-proj");
        assert_eq!(v["permissions"][0]["id"], "perm-1");
        let waiting: Vec<&str> = v["waiting"].as_array().unwrap().iter().map(|s| s["session_id"].as_str().unwrap()).collect();
        assert_eq!(waiting, vec!["s-ask"], "완료 보고로 끝난 세션은 답 필요가 아니다: {v}");
        assert_eq!(v["waiting"][0]["project"], "ny-proj");
        assert_eq!(v["waiting"][0]["path"], real.to_str().unwrap());
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
        awb_core::runlog::mark_done_if_absent(&log, 143);
        let content = std::fs::read_to_string(&done_path).unwrap();
        assert_eq!(content, "143");
        let _ = std::fs::remove_file(&done_path);
    }

    #[test]
    fn mark_done_if_absent_does_not_clobber_existing() {
        let log = tmp("awb_routes_test_cancel_existing.log");
        let done_path = format!("{log}.done");
        std::fs::write(&done_path, "0").unwrap();
        awb_core::runlog::mark_done_if_absent(&log, 143);
        let content = std::fs::read_to_string(&done_path).unwrap();
        assert_eq!(content, "0");
        let _ = std::fs::remove_file(&done_path);
    }

    #[test]
    fn resolve_hostname_is_non_empty() {
        // 환경별로 scutil/hostname 가용성이 다르므로(리눅스 CI 등) 값 자체보다 "항상 뭔가 반환"만 검증한다.
        let name = resolve_hostname();
        assert!(!name.is_empty());
    }

    #[tokio::test]
    async fn info_requires_auth_and_returns_hostname() {
        let devices_path = tmp("awb_routes_devices_info.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        let token = "tok-info-ok";
        state.devices.add(token, "test-device");
        let app = crate::routes::router(state);
        let unauth = app.clone().oneshot(Request::builder().uri("/info").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
        let res = app.oneshot(Request::builder().uri("/info").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(body["hostname"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn info_has_rate_limit_once_ws_done_is_built() {
        // QA #2: 앱은 done 을 받자마자 /info 를 읽는다 — 워처 틱 전이어도 한도가 실려 있어야 한다
        let dir = std::env::temp_dir().join("awb_routes_done_rl"); let _ = std::fs::remove_dir_all(&dir); std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("r.log").to_string_lossy().to_string();
        std::fs::write(&log, "{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\",\"resetsAt\":1783508400},\"session_id\":\"s1\"}\n").unwrap();
        std::fs::write(format!("{log}.done"), "1").unwrap();
        let devices_path = tmp("awb_routes_devices_done_rl.json"); let _ = std::fs::remove_file(&devices_path);
        let state = test_state("/tmp", &devices_path);
        state.devices.add("tok-rl", "test-device");
        let meta = crate::runreg::RunMeta { log, pgid: 0, workdir: dir.to_string_lossy().to_string(), project: "p".into(), notified: false, fork_of: None, worktree: None };
        state.runs.insert("r", meta.clone());
        crate::ws::finish(&state, &meta, "r", None);
        let res = crate::routes::router(state).oneshot(Request::builder().uri("/info").header("authorization", "Bearer tok-rl").body(Body::empty()).unwrap()).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["rate_limit"]["status"], "rejected");
        assert_eq!(body["rate_limit"]["project"], "p");
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

    // ── 폰 격리 트리 ──
    fn sh_git(dir: &std::path::Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }
    /// origin 이 있고 커밋이 하나 있는 저장소(스캔에 걸리고 HEAD 가 있다)
    fn origin_repo(dir: &std::path::Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        sh_git(dir, &["init", "-q"]);
        sh_git(dir, &["remote", "add", "origin", "x"]);
        sh_git(dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        std::fs::canonicalize(dir).unwrap().to_string_lossy().to_string()
    }
    /// 다른 테스트와 파일을 나누지 않는 상태 — 매핑·푸시 토큰·run 로그를 이 테스트 전용으로
    fn iso_state(base: &std::path::Path) -> AppState {
        let mut st = test_state(base.join("repos").to_str().unwrap(), base.join("devices.json").to_str().unwrap());
        st.sessions = crate::sessions::SessionStore::load(base.join("sessions").to_str().unwrap());
        st.push = crate::push::PushStore::load(base.join("push.json").to_str().unwrap());
        st.runs_dir = base.join("runs").to_string_lossy().to_string();
        st
    }
    async fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
        for _ in 0..100 { if f() { return; } tokio::time::sleep(std::time::Duration::from_millis(100)).await; }
        panic!("시간 안에 안 됨: {what}");
    }

    #[tokio::test]
    async fn isolated_chat_runs_in_worktree_and_resume_follows_it() {
        let base = std::env::temp_dir().join("awb_iso_chat"); let _ = std::fs::remove_dir_all(&base);
        let repo = origin_repo(&base.join("repos").join("isodemo"));
        let branch_before = sh_git(std::path::Path::new(&repo), &["rev-parse", "--abbrev-ref", "HEAD"]);
        let mut st = iso_state(&base);
        let fake = base.join("fakeclaude");
        std::fs::write(&fake, "#!/bin/sh\necho '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sid-iso-chat\"}'\npwd\n").unwrap();
        { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        st.claude_bin = fake.to_string_lossy().to_string();
        let send = |v: serde_json::Value| { let st = st.clone(); async move { chat_handler(State(st), Path("isodemo".into()), Json(serde_json::from_value::<ChatBody>(v).unwrap())).await } };
        let done = |log: &str| { let l = log.to_string(); move || std::path::Path::new(&format!("{l}.done")).exists() };

        // 진행 중인 대화를 격리로 옮기지 않는다
        let r = send(serde_json::json!({ "prompt": "hi", "isolate": true, "resume_session_id": "old" })).await;
        assert!(matches!(r, Err((StatusCode::BAD_REQUEST, _))));

        let r = send(serde_json::json!({ "prompt": "hi", "isolate": true, "new_session": true, "mode": "bypassPermissions" })).await.unwrap().0;
        let wt = r.worktree.expect("격리 실행은 트리를 돌려준다");
        assert!(wt.path.starts_with(&format!("{repo}.wt-phone-")), "{}", wt.path);
        assert!(wt.branch.starts_with("claude/phone-"));
        assert_eq!(sh_git(std::path::Path::new(&wt.path), &["rev-parse", "--abbrev-ref", "HEAD"]), wt.branch);
        assert_eq!(sh_git(std::path::Path::new(&repo), &["rev-parse", "--abbrev-ref", "HEAD"]), branch_before, "원래 트리 브랜치는 그대로");
        let log = r.log.unwrap();
        wait_for("첫 실행 종료", done(&log)).await;
        assert!(std::fs::read_to_string(&log).unwrap().lines().any(|l| l == wt.path), "cwd 가 격리 트리여야 한다");

        // 완료 워처가 세션을 트리에 묶은 뒤 — 이어 보내면 같은 트리에서 돈다
        let store = crate::worktree::store(&st);
        wait_for("세션 매핑", || store.for_session("sid-iso-chat").is_some()).await;
        let r2 = send(serde_json::json!({ "prompt": "more", "resume_session_id": "sid-iso-chat", "mode": "bypassPermissions" })).await.unwrap().0;
        assert_eq!(r2.worktree.as_ref().map(|w| w.path.as_str()), Some(wt.path.as_str()));
        let log2 = r2.log.unwrap();
        wait_for("이어 보낸 실행 종료", done(&log2)).await;
        assert!(std::fs::read_to_string(&log2).unwrap().lines().any(|l| l == wt.path), "이어 보낸 실행도 격리 트리에서");
        // 격리를 끈 새 대화는 원래 트리에서
        let r3 = send(serde_json::json!({ "prompt": "plain", "new_session": true, "mode": "bypassPermissions" })).await.unwrap().0;
        assert!(r3.worktree.is_none());
        let log3 = r3.log.unwrap();
        wait_for("일반 실행 종료", done(&log3)).await;
        assert!(std::fs::read_to_string(&log3).unwrap().lines().any(|l| l == repo));
        wait_for("일반 실행 락 해제", || !awb_core::lock::held(&repo)).await;
        let _ = std::process::Command::new("git").args(["-C", &repo, "worktree", "remove", &wt.path]).output();
    }

    #[test]
    fn worktree_sessions_listed_under_original_project() {
        let base = std::env::temp_dir().join("awb_iso_list"); let _ = std::fs::remove_dir_all(&base);
        let repo = origin_repo(&base.join("repos").join("lp-proj"));
        // 격리 트리도 origin 있는 git 루트라 스캔에 걸린다 — 따로 프로젝트로 세우지 않아야 한다
        let wt_path = origin_repo(&base.join("repos").join("lp-proj.wt-phone-abc123"));
        let st = iso_state(&base);
        crate::worktree::store(&st).add(&crate::worktree::Worktree {
            project: "lp-proj".into(), repo: repo.clone(), path: wt_path.clone(), branch: "claude/phone-abc123".into(), base: "x".into(), removed: false,
        }).unwrap();
        let tx_root = base.join("tx");
        let line = "{\"type\":\"user\",\"entrypoint\":\"sdk-cli\",\"message\":{\"role\":\"user\",\"content\":\"질문\"}}\n";
        for (dir, sid) in [(&repo, "s-main"), (&wt_path, "s-iso")] {
            let d = tx_root.join(awb_core::transcript::project_slug(dir)); std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{sid}.jsonl")), line).unwrap();
        }
        let v = serde_json::to_value(super::project_dtos(&st, tx_root.to_str().unwrap(), &Default::default())).unwrap();
        let names: Vec<&str> = v.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["lp-proj"], "격리 트리는 프로젝트로 안 나온다: {v}");
        let sessions = v[0]["sessions"].as_array().unwrap();
        let mut ids: Vec<&str> = sessions.iter().map(|s| s["session_id"].as_str().unwrap()).collect();
        ids.sort();
        assert_eq!(ids, vec!["s-iso", "s-main"]);
        let iso = sessions.iter().find(|s| s["session_id"] == "s-iso").unwrap();
        assert_eq!(iso["worktree"]["path"], wt_path.as_str());
        assert_eq!(iso["worktree"]["branch"], "claude/phone-abc123");
        assert!(sessions.iter().find(|s| s["session_id"] == "s-main").unwrap().get("worktree").is_none());
    }

    #[tokio::test]
    async fn ending_session_removes_clean_tree_and_keeps_changed_one() {
        let base = std::env::temp_dir().join("awb_iso_end"); let _ = std::fs::remove_dir_all(&base);
        let repo = origin_repo(&base.join("repos").join("endproj"));
        let st = iso_state(&base);
        let store = crate::worktree::store(&st);
        let mk = |sid: &str| {
            let wt = crate::worktree::create("endproj", &repo).unwrap();
            store.add(&wt).unwrap();
            store.note_session(sid, &wt.path).unwrap();
            wt
        };
        let (clean, dirty, committed, archived) = (mk("s-clean"), mk("s-dirty"), mk("s-commit"), mk("s-arch"));
        std::fs::write(std::path::Path::new(&dirty.path).join("new.txt"), "x").unwrap();
        sh_git(std::path::Path::new(&committed.path), &["commit", "-q", "--allow-empty", "-m", "work"]);
        let end = |sid: &str| { let st = st.clone(); let sid = sid.to_string(); async move { session_end_handler(State(st), Path(("endproj".into(), sid))).await.unwrap().0 } };
        let branch_exists = |b: &str| !sh_git(std::path::Path::new(&repo), &["branch", "--list", b]).is_empty();

        let v = end("s-clean").await;
        assert_eq!(v["ended"], true, "{v}");
        assert_eq!(v["worktree"]["removed"], true, "{v}");
        assert_eq!(v["worktree"]["branch_deleted"], true, "{v}");
        assert!(!std::path::Path::new(&clean.path).exists());
        assert!(!branch_exists(&clean.branch));
        assert!(crate::worktree::for_resume(&st, "endproj", Some("s-clean")).is_err(), "지운 트리의 세션은 이어갈 수 없다");

        let v = end("s-dirty").await;
        assert_eq!(v["worktree"]["removed"], false, "{v}");
        assert!(v["worktree"]["reason"].as_str().unwrap().contains("커밋 안 한 변경"), "{v}");
        assert!(std::path::Path::new(&dirty.path).join("new.txt").exists());

        let v = end("s-commit").await;
        assert_eq!(v["worktree"]["removed"], false, "{v}");
        assert!(v["worktree"]["reason"].as_str().unwrap().contains("새 커밋 1개"), "{v}");
        assert!(branch_exists(&committed.branch));

        let body: SessionMetaBody = serde_json::from_value(serde_json::json!({ "archived": true })).unwrap();
        let v = session_meta_handler(State(st.clone()), Path(("endproj".into(), "s-arch".into())), Json(body)).await.unwrap().0;
        assert_eq!(v["archived"], true);
        assert_eq!(v["worktree"]["removed"], true, "보관도 변경 없는 트리를 지운다: {v}");
        assert!(!std::path::Path::new(&archived.path).exists());

        // 격리 세션이 아닌 끝난 세션은 예전처럼 404
        assert!(matches!(session_end_handler(State(st.clone()), Path(("endproj".into(), "s-none".into()))).await, Err(StatusCode::NOT_FOUND)));
    }
}
