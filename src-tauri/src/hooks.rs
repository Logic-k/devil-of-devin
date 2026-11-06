use axum::{extract::State as AxState, routing::post, Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

pub struct HookServer {
    pub port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

struct HookCtx {
    app: AppHandle,
}

/// Core: bind the hook receiver on 127.0.0.1 and register it in state.
/// Idempotent — returns the existing port if already running.
pub async fn start_server(state: &AppState, app: AppHandle, port: Option<u16>) -> Result<u16, String> {
    if let Some(h) = &*state.hook_server.lock() {
        return Ok(h.port);
    }
    let ctx = Arc::new(HookCtx { app });
    let router = Router::new()
        .route("/hook", post(hook_handler))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(ctx);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port.unwrap_or(0))))
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    *state.hook_server.lock() = Some(HookServer { port, shutdown: tx });
    Ok(port)
}

/// Start the local hook receiver: POST /hook on 127.0.0.1.
#[tauri::command]
pub async fn hook_server_start(
    state: State<'_, AppState>,
    app: AppHandle,
    port: Option<u16>,
) -> Result<u16, String> {
    start_server(&state, app, port).await
}

async fn hook_handler(AxState(ctx): AxState<Arc<HookCtx>>, Json(body): Json<Value>) -> &'static str {
    let _ = ctx.app.emit("hook://event", body);
    "ok"
}

#[tauri::command]
pub fn hook_server_stop(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(h) = state.hook_server.lock().take() {
        let _ = h.shutdown.send(());
    }
    Ok(())
}

const HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "UserPromptSubmit",
    "PermissionRequest",
    "Stop",
    "PostCompaction",
    "SessionStart",
    "SessionEnd",
];

/// Write/merge `<cwd>/.devin/hooks.v1.json` so all worker lifecycle events
/// are POSTed to our local hook server. Existing hook entries are preserved.
pub fn inject_into(cwd: &str, port: u16) -> Result<String, String> {
    let dir = std::path::Path::new(cwd).join(".devin");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("hooks.v1.json");

    let mut doc: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));

    let marker = format!("devil-hook-{port}");
    // devin hooks run under Git Bash on Windows — must end with a zero exit
    // status (a failing UserPromptSubmit hook BLOCKS the prompt entirely).
    let cmd = format!(
        "curl.exe -s -m 3 -X POST http://127.0.0.1:{port}/hook -H \"Content-Type: application/json\" --data-binary @- >/dev/null 2>&1 || true"
    );

    let obj = doc
        .as_object_mut()
        .ok_or_else(|| "hooks file is not an object".to_string())?;
    for ev in HOOK_EVENTS {
        let entry = obj.entry(ev.to_string()).or_insert_with(|| json!([]));
        let arr = entry
            .as_array_mut()
            .ok_or_else(|| "hook event is not an array".to_string())?;
        // drop stale devil entries (old ports / old broken commands), keep user hooks
        arr.retain(|m| !m.to_string().contains("_devil"));
        arr.push(json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": cmd,
                "timeout": 5,
                "_devil": marker
            }]
        }));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn hooks_inject(cwd: String, port: u16) -> Result<String, String> {
    inject_into(&cwd, port)
}
