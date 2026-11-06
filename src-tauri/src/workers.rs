use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::acp::{AcpAgent, SpawnOpts};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub session_id: String,
    pub cwd: String,
    pub model: String,
    pub role: String,
    pub mode: String,
    pub status: String,
    pub created_at: String,
    /// Project this worker belongs to (path) — used by the sidebar grouping.
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub usage: crate::acp::Usage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerCreateOpts {
    pub name: Option<String>,
    pub cwd: String,
    pub model: Option<String>,
    pub role: Option<String>,
    pub agent_type: Option<String>,
    pub auto_approve: Option<bool>,
    pub mode: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub project: Option<String>,
    /// Resume this devin session instead of creating a new one.
    pub resume_session: Option<String>,
}

fn workers_file() -> std::path::PathBuf {
    crate::state::Settings::dir().join("workers.json")
}

/// Persist the worker registry so sessions can be re-attached after restart.
pub fn persist(state: &AppState) {
    let list: Vec<Worker> = state.workers.lock().values().cloned().collect();
    if let Ok(s) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::create_dir_all(crate::state::Settings::dir());
        let _ = std::fs::write(workers_file(), s);
    }
}

/// Workers saved by a previous app run (their devin sessions still exist and
/// can be re-attached with `worker_create { resumeSession }`).
#[tauri::command]
pub fn worker_saved_list() -> Vec<Worker> {
    std::fs::read_to_string(workers_file())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Worker>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|mut w| {
            w.status = "detached".into();
            w
        })
        .collect()
}

fn emit_workers(app: &AppHandle, state: &AppState) {
    let list: Vec<Worker> = state.workers.lock().values().cloned().collect();
    let _ = app.emit("workers://changed", list);
}

/// Spawn a `devin acp` agent + create (or load) one session = one worker.
#[tauri::command]
pub async fn worker_create(
    state: State<'_, AppState>,
    app: AppHandle,
    opts: WorkerCreateOpts,
) -> Result<Worker, String> {
    let (model, mut mcp_servers, worker_env, default_mode) = {
        let s = state.settings.lock();
        let model = opts.model.clone().unwrap_or_else(|| s.default_model.clone());
        (model, s.mcp_servers.clone(), s.worker_env.clone(), s.default_permission_mode.clone())
    };
    let mut env = worker_env;
    env.extend(opts.env.clone().unwrap_or_default());
    crate::secrets::resolve_env(&mut env);
    for m in mcp_servers.iter_mut() {
        crate::secrets::resolve_json(m);
    }
    // auto-install lifecycle hooks so the app sees worker tool/prompt events
    {
        let g = state.hook_server.lock();
        if let Some(h) = &*g {
            let _ = crate::hooks::inject_into(&opts.cwd, h.port);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let name = opts
        .name
        .clone()
        .unwrap_or_else(|| format!("worker-{}", &id[..8]));
    let agent_id = format!("w-{id}");
    let agent = AcpAgent::spawn(
        &state,
        app.clone(),
        agent_id.clone(),
        SpawnOpts {
            cwd: opts.cwd.clone(),
            model: Some(model.clone()),
            agent_type: opts.agent_type.clone(),
            cloud: false,
            auto_approve: opts.auto_approve.unwrap_or(false),
            env,
            label: Some(format!("worker:{name}")),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    // register early so the UI can show logs even if session/new fails
    state.acp.lock().insert(agent_id.clone(), agent.clone());

    let session_id = if let Some(sid) = opts.resume_session.clone() {
        crate::acp::ensure_authed(&agent).await?;
        agent.session_load(&sid, &opts.cwd).await?;
        sid
    } else {
        let resp = agent.session_new_authed(&opts.cwd, mcp_servers, true).await?;
        resp.get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let mode = opts.mode.clone().unwrap_or(default_mode);
    if !session_id.is_empty() && !mode.is_empty() {
        if let Err(e) = agent.set_mode(&session_id, &mode).await {
            agent.emit("log", json!({ "line": format!("set_mode({mode}) 실패: {e}") }));
        }
    }

    let w = Worker {
        id: id.clone(),
        name,
        agent_id,
        session_id,
        cwd: opts.cwd.clone(),
        model,
        role: opts.role.clone().unwrap_or_else(|| "general".into()),
        mode,
        status: "idle".into(),
        created_at: now_iso(),
        project: opts.project.clone().or_else(|| Some(opts.cwd.clone())),
        usage: Default::default(),
    };
    state.workers.lock().insert(id.clone(), w.clone());
    persist(&state);
    emit_workers(&app, &state);
    Ok(w)
}

#[tauri::command]
pub async fn worker_prompt(
    state: State<'_, AppState>,
    app: AppHandle,
    worker_id: String,
    text: String,
) -> Result<Value, String> {
    let (agent, session) = {
        let mut g = state.workers.lock();
        let w = g.get_mut(&worker_id).ok_or("no worker")?;
        if w.status == "running" {
            return Err("워커가 아직 이전 프롬프트를 실행 중입니다".into());
        }
        let a = state
            .acp
            .lock()
            .get(&w.agent_id)
            .cloned()
            .ok_or("worker agent gone")?;
        w.status = "running".into();
        (a, w.session_id.clone())
    };
    emit_workers(&app, &state);
    let r = agent.prompt(&session, &text).await;
    if let Some(w) = state.workers.lock().get_mut(&worker_id) {
        w.status = if r.is_ok() { "idle".into() } else { "error".into() };
        w.usage = agent.usage.lock().clone();
    }
    persist(&state);
    emit_workers(&app, &state);
    r
}

#[tauri::command]
pub async fn worker_set_mode(
    state: State<'_, AppState>,
    app: AppHandle,
    worker_id: String,
    mode: String,
) -> Result<(), String> {
    let (agent, session) = {
        let g = state.workers.lock();
        let w = g.get(&worker_id).ok_or("no worker")?;
        let a = state.acp.lock().get(&w.agent_id).cloned().ok_or("worker agent gone")?;
        (a, w.session_id.clone())
    };
    agent.set_mode(&session, &mode).await?;
    if let Some(w) = state.workers.lock().get_mut(&worker_id) {
        w.mode = mode;
    }
    persist(&state);
    emit_workers(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn worker_list(state: State<'_, AppState>) -> Vec<Worker> {
    state.workers.lock().values().cloned().collect()
}

#[tauri::command]
pub async fn worker_stop(state: State<'_, AppState>, app: AppHandle, worker_id: String) -> Result<(), String> {
    let agent_id = {
        let mut g = state.workers.lock();
        match g.get_mut(&worker_id) {
            Some(w) => {
                w.status = "stopped".into();
                w.agent_id.clone()
            }
            None => return Ok(()),
        }
    };
    let removed = state.acp.lock().remove(&agent_id);
    if let Some(a) = removed {
        a.kill().await;
    }
    persist(&state);
    emit_workers(&app, &state);
    Ok(())
}

/// Delete a worker entirely: close/delete its devin session, kill the
/// agent process, drop the entry from the worker list.
#[tauri::command]
pub async fn worker_delete(
    state: State<'_, AppState>,
    app: AppHandle,
    worker_id: String,
    delete_session: Option<bool>,
) -> Result<(), String> {
    let (agent_id, session_id) = {
        let mut g = state.workers.lock();
        match g.remove(&worker_id) {
            Some(w) => (w.agent_id, w.session_id),
            None => return Ok(()),
        }
    };
    let removed = state.acp.lock().remove(&agent_id);
    if let Some(a) = removed {
        if !session_id.is_empty() && delete_session.unwrap_or(true) {
            let _ = a
                .call_timeout(
                    "session/delete",
                    json!({ "sessionId": session_id }),
                    std::time::Duration::from_secs(30),
                )
                .await;
        }
        a.kill().await;
    }
    persist(&state);
    emit_workers(&app, &state);
    Ok(())
}

/// Forget a saved (detached) worker without touching the devin session.
#[tauri::command]
pub fn worker_forget_saved(worker_id: String) -> Result<(), String> {
    let mut list = worker_saved_list();
    list.retain(|w| w.id != worker_id);
    std::fs::write(
        workers_file(),
        serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ---------- one-shot runner (pipelines / loop) ----------

pub struct OneshotOpts<'a> {
    pub label: &'a str,
    pub cwd: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    /// ACP session mode; `None` keeps the agent's default (accept-edits).
    pub mode: Option<&'a str>,
    pub agent_type: Option<&'a str>,
    pub auto_approve: bool,
    /// Kill the agent if the prompt has not finished in time.
    pub timeout: Option<std::time::Duration>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OneshotResult {
    pub agent_id: String,
    pub session_id: String,
    pub stop_reason: String,
    pub text: String,
    pub usage: crate::acp::Usage,
}

/// Spawn a temporary ACP worker, run one prompt to completion, kill it.
/// This is by construction a fresh, non-inheriting session — exactly what
/// the harness's independent `task_validation` requires.
pub async fn run_oneshot(
    state: &AppState,
    app: AppHandle,
    opts: OneshotOpts<'_>,
) -> Result<OneshotResult, String> {
    let mut env = state.settings.lock().worker_env.clone();
    crate::secrets::resolve_env(&mut env);
    let agent_id = format!("oneshot-{}-{}", opts.label, &uuid::Uuid::new_v4().to_string()[..8]);
    let agent = AcpAgent::spawn(
        state,
        app,
        agent_id.clone(),
        SpawnOpts {
            cwd: opts.cwd.to_string(),
            model: Some(opts.model.to_string()),
            agent_type: opts.agent_type.map(|s| s.to_string()),
            cloud: false,
            auto_approve: opts.auto_approve,
            env,
            label: Some(opts.label.to_string()),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    register_agent(state, agent.clone());
    let result = run_oneshot_inner(state, &agent, &opts).await;
    agent.kill().await;
    state.acp.lock().remove(&agent.id);
    result
}

async fn run_oneshot_inner(
    state: &AppState,
    agent: &Arc<AcpAgent>,
    opts: &OneshotOpts<'_>,
) -> Result<OneshotResult, String> {
    let mut mcp = state.settings.lock().mcp_servers.clone();
    for m in mcp.iter_mut() {
        crate::secrets::resolve_json(m);
    }
    let sid = agent
        .session_new_authed(opts.cwd, mcp, true)
        .await?
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    if let Some(mode) = opts.mode {
        let _ = agent.set_mode(&sid, mode).await;
    }
    agent.set_collect(true);
    let r = match opts.timeout {
        Some(t) => match tokio::time::timeout(t, agent.prompt(&sid, opts.prompt)).await {
            Ok(r) => r,
            Err(_) => {
                let _ = agent.cancel(&sid).await;
                Err(format!("프롬프트 시간 초과 ({}s)", t.as_secs()))
            }
        },
        None => agent.prompt(&sid, opts.prompt).await,
    };
    let text = agent.take_collected();
    agent.set_collect(false);
    let _ = agent
        .call_timeout(
            "session/close",
            json!({ "sessionId": sid }),
            std::time::Duration::from_secs(15),
        )
        .await;
    let v = r?;
    Ok(OneshotResult {
        agent_id: agent.id.clone(),
        session_id: sid,
        stop_reason: v
            .get("stopReason")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        text,
        usage: agent.usage.lock().clone(),
    })
}

/// Backwards-compatible helper used by the research pipeline.
pub async fn run_oneshot_worker(
    state: &AppState,
    app: AppHandle,
    label: &str,
    cwd: &str,
    model: &str,
    prompt: &str,
    auto_approve: bool,
) -> Result<Value, String> {
    let r = run_oneshot(
        state,
        app,
        OneshotOpts {
            label,
            cwd,
            model,
            prompt,
            mode: Some("smart"),
            agent_type: None,
            auto_approve,
            timeout: None,
        },
    )
    .await?;
    Ok(json!({ "stopReason": r.stop_reason, "text": r.text }))
}

/// Shared handle for the pipeline so agents register in AppState (for kill-all).
pub fn register_agent(state: &AppState, agent: Arc<AcpAgent>) {
    state.acp.lock().insert(agent.id.clone(), agent);
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
