// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod scan;
mod shell_env;

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
fn list_projects(roots: Vec<String>) -> Vec<scan::Project> {
    scan::scan_roots(&roots)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping, list_projects])
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
