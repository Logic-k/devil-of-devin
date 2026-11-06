use serde::Serialize;
use std::path::PathBuf;
use std::process::Stdio;
use tauri::Manager;
use tokio::process::Command;

/// Devin CLI version this app was verified against (ACP handshake, `_meta.api_key`
/// auth, `--agent-type review`, session modes, slash-skill invocation).
pub const TESTED_DEVIN_VERSION: &str = "3000.11.234";

/// Pull the first dotted number out of a `devin version` string.
pub fn version_number(s: &str) -> Option<Vec<u64>> {
    for tok in s.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',') {
        let t = tok.trim_start_matches('v');
        if t.contains('.') && t.chars().all(|c| c.is_ascii_digit() || c == '.') {
            let parts: Vec<u64> = t.split('.').filter_map(|p| p.parse().ok()).collect();
            if parts.len() >= 2 {
                return Some(parts);
            }
        }
    }
    None
}

/// "tested" | "newer" | "older" | "unknown" relative to TESTED_DEVIN_VERSION.
pub fn version_status(installed: Option<&str>) -> String {
    let Some(inst) = installed.and_then(version_number) else { return "unknown".into() };
    let tested = version_number(TESTED_DEVIN_VERSION).unwrap_or_default();
    match inst.cmp(&tested) {
        std::cmp::Ordering::Equal => "tested",
        std::cmp::Ordering::Greater => "newer",
        std::cmp::Ordering::Less => "older",
    }
    .into()
}

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
    /// credentials.toml has a usable session token for ACP meta.api_key auth.
    pub session_token: bool,
    pub tested_version: String,
    /// "tested" | "newer" | "older" | "unknown"
    pub version_status: String,
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
        session_token: crate::acp::read_session_api_key().is_some(),
        tested_version: TESTED_DEVIN_VERSION.into(),
        version_status: "unknown".into(),
    };
    if let Some(p) = &path {
        let (ok, out, err) = run_capture({
            let mut c = Command::new(p);
            c.arg("version");
            c
        })
        .await;
        info.version = Some(if ok { out.trim().to_string() } else { err.trim().to_string() });
        info.version_status = version_status(info.version.as_deref());
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

/// Run `devin auth login` inside a ConPTY (the flow is an interactive TUI
/// that ignores piped stdin). Auto-presses Enter to pick "Log in with browser".
/// Output streams to `pty://out/login`; the exit event goes to `devin://login-done`.
#[tauri::command]
pub async fn devin_login(
    state: tauri::State<'_, crate::state::AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let configured = state.settings.lock().devin_path.clone();
    let p = find_devin(&configured).ok_or("devin not found")?;

    if let Some(old) = state.ptys.lock().remove("login") {
        let _ = old.kill();
    }
    let s = crate::pty::PtySession::spawn(
        app.clone(),
        "login".into(),
        crate::pty::PtySpawnOpts {
            command: p.display().to_string(),
            args: Some(vec!["auth".into(), "login".into()]),
            cwd: None,
            cols: Some(120),
            rows: Some(30),
            env: None,
            respond_to_queries: Some(true),
        },
    )
    .map_err(|e| e.to_string())?;
    state.ptys.lock().insert("login".to_string(), s.clone());

    // auto-select option 1 ("Log in with browser") once the TUI has drawn
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let _ = s.write(b"\r");

    // poll for exit → login-done event + cleanup
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let st = app2.state::<crate::state::AppState>();
        for _ in 0..600 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if s.exited() {
                break;
            }
        }
        st.ptys.lock().remove("login");
        let _ = tauri::Emitter::emit(&app2, "devin://login-done", serde_json::json!({}));
    });
    Ok(())
}

/// Open a URL in the system browser (no shell plugin needed).
#[tauri::command]
pub fn url_open(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) urls".into());
    }
    // `explorer <url>` opens the default browser and does not interpret `&` like cmd does
    std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Native folder picker — runs fully on the Rust side, so no
/// dialog-plugin capability is required.
#[tauri::command]
pub fn pick_folder(default: Option<String>) -> Option<String> {
    let mut d = rfd::FileDialog::new();
    if let Some(dir) = default {
        if std::path::Path::new(&dir).exists() {
            d = d.set_directory(&dir);
        }
    }
    d.pick_folder().map(|p| p.to_string_lossy().to_string())
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
