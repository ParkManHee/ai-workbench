// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use awb_core::{scan, preflight, worklog, runner, runlog, transcript};

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
fn list_projects(roots: Vec<String>) -> Vec<scan::Project> {
    scan::scan_roots(&roots)
}

#[tauri::command]
fn preflight(roots: Vec<String>, claude_override: Option<String>) -> preflight::Preflight {
    preflight::run_preflight(&roots, claude_override)
}

#[tauri::command]
fn worklog_badge(name: String) -> Option<worklog::Badge> {
    worklog::badge_for(&name)
}

#[tauri::command]
fn start_run(claude_bin: String, workdir: String, settings: String, plan: bool, prompt: String, runs_dir: Option<String>) -> Result<runner::RunHandle, String> {
    let runs_dir = runs_dir.unwrap_or_else(|| format!("{}/.claude/.awb-runs", std::env::var("HOME").unwrap_or_default()));
    runner::start_run(&claude_bin, &workdir, &settings, plan, &prompt, &runs_dir)
}

#[tauri::command]
fn read_log(log: String, offset: u64) -> runlog::LogChunk {
    runlog::read_log(&log, offset)
}

#[tauri::command]
fn run_status(log: String, workdir: String) -> runlog::RunStatus {
    runlog::run_status(&log, &workdir)
}

#[tauri::command]
fn cancel_run(pgid: i32, workdir: String) -> bool {
    runner::cancel_run(pgid, &workdir)
}

#[tauri::command]
fn list_sessions(project_path: String) -> Vec<transcript::SessionInfo> {
    transcript::list_sessions(&transcript::project_slug(&project_path))
}

/// 대화 페이지: until 없으면 최근 1시간(최대 100), 있으면 그 이전 50개("이전 더 보기").
#[tauri::command]
fn transcript_page(project_path: String, session_id: String, until: Option<usize>) -> Result<transcript::Page, String> {
    if !transcript::safe_session_id(&session_id) { return Err("잘못된 세션 ID".into()); }
    let path = transcript::transcript_path(&transcript::project_slug(&project_path), &session_id);
    let limit = if until.is_some() { 50 } else { 100 };
    Ok(transcript::read_transcript_page(&path, until, limit, 3600))
}

/// 증분 조회(라이브 폴): from 라인 이후 새 메시지.
#[tauri::command]
fn transcript_from(project_path: String, session_id: String, from: usize) -> Result<serde_json::Value, String> {
    if !transcript::safe_session_id(&session_id) { return Err("잘못된 세션 ID".into()); }
    let path = transcript::transcript_path(&transcript::project_slug(&project_path), &session_id);
    let (messages, next, active) = transcript::read_transcript(&path, from);
    Ok(serde_json::json!({ "messages": messages, "next": next, "active": active }))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping, list_projects, preflight, worklog_badge, start_run, read_log, run_status, cancel_run, list_sessions, transcript_page, transcript_from])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn ping_returns_pong() {
        assert_eq!(super::ping(), "pong");
    }
}
