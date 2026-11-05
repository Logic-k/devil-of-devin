use serde::Serialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Locate the devin CLI binary.
pub fn find_devin(configured: &str) -> Option<PathBuf> {
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(home).join(r"devin\cli\bin\devin.exe");
        if p.exists() {
            return Some(p);
        }
    }
    // PATH lookup
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(';') {
            let p = PathBuf::from(dir).join("devin.exe");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

#[derive(Serialize)]
pub struct DevinInfo {
    pub path: Option<String>,
    pub version: Option<String>,
    pub authenticated: bool,
    pub auth_detail: String,
}

async fn run_capture(mut cmd: Command) -> (bool, String, String) {
    let out = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;
    match out {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (false, String::new(), e.to_string()),
    }
}

#[tauri::command]
pub async fn devin_info(state: tauri::State<'_, crate::state::AppState>) -> Result<DevinInfo, String> {
    let configured = state.settings.lock().devin_path.clone();
    let path = find_devin(&configured);
    let mut info = DevinInfo {
        path: path.as_ref().map(|p| p.display().to_string()),
        version: None,
        authenticated: false,
        auth_detail: String::new(),
    };
    if let Some(p) = &path {
        let (ok, out, err) = run_capture({
            let mut c = Command::new(p);
            c.arg("version");
            c
        })
        .await;
        info.version = Some(if ok { out.trim().to_string() } else { err.trim().to_string() });
        let (_, out, err) = run_capture({
            let mut c = Command::new(p);
            c.args(["auth", "status"]);
            c
        })
        .await;
        let txt = format!("{out}{err}");
        info.authenticated = !txt.to_lowercase().contains("not logged in");
        info.auth_detail = txt.trim().to_string();
    }
    Ok(info)
}

#[tauri::command]
pub async fn devin_auth_status(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<String, String> {
    let configured = state.settings.lock().devin_path.clone();
    let p = find_devin(&configured).ok_or("devin not found")?;
    let (_, out, err) = run_capture({
        let mut c = Command::new(p);
        c.args(["auth", "status"]);
        c
    })
    .await;
    Ok(format!("{out}{err}").trim().to_string())
}

/// `devin models list --format json` — requires login.
#[tauri::command]
pub async fn devin_models(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<serde_json::Value, String> {
    let configured = state.settings.lock().devin_path.clone();
    let p = find_devin(&configured).ok_or("devin not found")?;
    let (ok, out, err) = run_capture({
        let mut c = Command::new(p);
        c.args(["models", "list", "--format", "json"]);
        c
    })
    .await;
    if !ok {
        return Err(format!("{out}{err}").trim().to_string());
    }
    serde_json::from_str(&out).map_err(|e| format!("parse: {e}: {out}"))
}

/// Launch `devin auth login` detached — it opens the browser and waits for the
/// OAuth callback. Emits `devin://login-done` when the process exits.
#[tauri::command]
pub async fn devin_login(
    state: tauri::State<'_, crate::state::AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let configured = state.settings.lock().devin_path.clone();
    let p = find_devin(&configured).ok_or("devin not found")?;
    let mut child = Command::new(p)
        .args(["auth", "login"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move {
        let status = child.wait().await;
        let _ = tauri::Emitter::emit(
            &app,
            "devin://login-done",
            serde_json::json!({ "ok": status.map(|s| s.success()).unwrap_or(false) }),
        );
    });
    Ok(())
}

/// One-shot `devin -p` (non-interactive print) — used by quick query/review passes.
#[tauri::command]
pub async fn devin_print_once(
    state: tauri::State<'_, crate::state::AppState>,
    cwd: String,
    prompt: String,
    model: Option<String>,
    permission_mode: Option<String>,
) -> Result<String, String> {
    let configured = state.settings.lock().devin_path.clone();
    let p = find_devin(&configured).ok_or("devin not found")?;
    let mut c = Command::new(p);
    if let Some(m) = model {
        c.args(["--model", &m]);
    }
    if let Some(pm) = permission_mode {
        c.args(["--permission-mode", &pm]);
    }
    c.args(["--respect-workspace-trust", "false", "-p", "--", &prompt])
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = c.output().await.map_err(|e| e.to_string())?;
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}
