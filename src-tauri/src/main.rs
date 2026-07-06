// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod preflight;
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping, list_projects, preflight, worklog_badge])
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
