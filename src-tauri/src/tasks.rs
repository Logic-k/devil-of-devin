use serde_json::Value;
use tauri::State;

use crate::state::AppState;

fn tasks_path(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join(".devil").join("tasks.json")
}

#[tauri::command]
pub fn tasks_load(_state: State<'_, AppState>, dir: String) -> Vec<Value> {
    std::fs::read_to_string(tasks_path(&dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn tasks_save(_state: State<'_, AppState>, dir: String, tasks: Vec<Value>) -> Result<(), String> {
    let p = tasks_path(&dir);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
