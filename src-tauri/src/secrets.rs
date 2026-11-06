use std::collections::HashMap;
use tauri::State;

use crate::state::AppState;

const SERVICE: &str = "devil-of-devin";
const PREFIX: &str = "$secret:";

pub fn get(name: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, name)
        .ok()
        .and_then(|e| e.get_password().ok())
}

/// Resolve env values of the form `$secret:NAME` from Windows Credential Manager.
/// Plain values pass through unchanged; missing secrets become empty strings.
pub fn resolve_env(env: &mut HashMap<String, String>) {
    for v in env.values_mut() {
        if let Some(name) = v.strip_prefix(PREFIX) {
            *v = get(name).unwrap_or_default();
        }
    }
}

/// Same substitution for arbitrary JSON (MCP server configs: headers, env, args).
pub fn resolve_json(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            if let Some(name) = s.strip_prefix(PREFIX) {
                *s = get(name).unwrap_or_default();
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(resolve_json),
        serde_json::Value::Object(o) => o.values_mut().for_each(resolve_json),
        _ => {}
    }
}

/// Store a secret in Windows Credential Manager (never written to settings.json).
#[tauri::command]
pub fn secret_set(_state: State<'_, AppState>, name: String, value: String) -> Result<(), String> {
    keyring::Entry::new(SERVICE, &name)
        .map_err(|e| e.to_string())?
        .set_password(&value)
        .map_err(|e| e.to_string())
}

/// Returns true if a secret exists (value is never returned to the UI).
#[tauri::command]
pub fn secret_exists(_state: State<'_, AppState>, name: String) -> bool {
    get(&name).is_some()
}

#[tauri::command]
pub fn secret_delete(_state: State<'_, AppState>, name: String) -> Result<(), String> {
    keyring::Entry::new(SERVICE, &name)
        .map_err(|e| e.to_string())?
        .delete_credential()
        .map_err(|e| e.to_string())
}
