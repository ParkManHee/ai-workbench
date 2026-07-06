// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod lock;
mod preflight;
mod runner;
mod runlog;
mod scan;
mod shell_env;
mod worklog;

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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping, list_projects, preflight, worklog_badge, start_run, read_log])
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
