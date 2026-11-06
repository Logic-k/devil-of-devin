use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, State};

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
    pub status: String,
    pub created_at: String,
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
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Spawn a `devin acp` agent + create one session = one worker.
#[tauri::command]
pub async fn worker_create(
    state: State<'_, AppState>,
    app: AppHandle,
    opts: WorkerCreateOpts,
) -> Result<Worker, String> {
    let (model, mcp_servers, worker_env) = {
        let s = state.settings.lock();
        let model = opts.model.clone().unwrap_or_else(|| s.default_model.clone());
        (model, s.mcp_servers.clone(), s.worker_env.clone())
    };
    let mut env = worker_env;
    env.extend(opts.env.clone().unwrap_or_default());
    crate::secrets::resolve_env(&mut env);
    // auto-install lifecycle hooks so the app sees worker tool/prompt events
    {
        let g = state.hook_server.lock();
        if let Some(h) = &*g {
            let _ = crate::hooks::inject_into(&opts.cwd, h.port);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let agent = AcpAgent::spawn(
        &state,
        app,
        format!("w-{id}"),
        SpawnOpts {
            cwd: opts.cwd.clone(),
            model: Some(model.clone()),
            agent_type: opts.agent_type.clone(),
            cloud: false,
            auto_approve: opts.auto_approve.unwrap_or(false),
            env,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let resp = agent
        .session_new_authed(&opts.cwd, mcp_servers, true)
        .await?;
    let session_id = resp
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    state.acp.lock().insert(format!("w-{id}"), agent.clone());

    let w = Worker {
        id: id.clone(),
        name: opts
            .name
            .clone()
            .unwrap_or_else(|| format!("worker-{}", &id[..8])),
        agent_id: format!("w-{id}"),
        session_id,
        cwd: opts.cwd.clone(),
        model,
        role: opts.role.clone().unwrap_or_else(|| "general".into()),
        status: "idle".into(),
        created_at: chrono_now(),
    };
    state.workers.lock().insert(id.clone(), w.clone());
    Ok(w)
}

#[tauri::command]
pub async fn worker_prompt(
    state: State<'_, AppState>,
    worker_id: String,
    text: String,
) -> Result<Value, String> {
    let (agent, session) = {
        let g = state.workers.lock();
        let w = g.get(&worker_id).ok_or("no worker")?;
        let a = state
            .acp
            .lock()
            .get(&w.agent_id)
            .cloned()
            .ok_or("worker agent gone")?;
        (a, w.session_id.clone())
    };
    if let Some(w) = state.workers.lock().get_mut(&worker_id) {
        w.status = "running".into();
    }
    let r = agent.prompt(&session, &text).await;
    if let Some(w) = state.workers.lock().get_mut(&worker_id) {
        w.status = if r.is_ok() { "idle".into() } else { "error".into() };
    }
    r
}

#[tauri::command]
pub fn worker_list(state: State<'_, AppState>) -> Vec<Worker> {
    state.workers.lock().values().cloned().collect()
}

#[tauri::command]
pub async fn worker_stop(state: State<'_, AppState>, worker_id: String) -> Result<(), String> {
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
    Ok(())
}

/// Delete a worker entirely: close/delete its devin session, kill the
/// agent process, drop the entry from the worker list.
#[tauri::command]
pub async fn worker_delete(
    state: State<'_, AppState>,
    worker_id: String,
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
        if !session_id.is_empty() {
            let _ = a
                .call("session/delete", json!({ "sessionId": session_id }))
                .await;
        }
        a.kill().await;
    }
    Ok(())
}

/// Spawn a temporary ACP worker, run one prompt to completion, kill it.
/// Used by the research pipeline — returns final Value (stopReason).
pub async fn run_oneshot_worker(
    state: &AppState,
    app: AppHandle,
    label: &str,
    cwd: &str,
    model: &str,
    prompt: &str,
    auto_approve: bool,
) -> Result<Value, String> {
    let mut env = state.settings.lock().worker_env.clone();
    crate::secrets::resolve_env(&mut env);
    let agent = AcpAgent::spawn(
        state,
        app,
        format!("oneshot-{label}-{}", &uuid::Uuid::new_v4().to_string()[..8]),
        SpawnOpts {
            cwd: cwd.to_string(),
            model: Some(model.to_string()),
            agent_type: None,
            cloud: false,
            auto_approve,
            env,
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    register_agent(state, agent.clone());
    let mcp = state.settings.lock().mcp_servers.clone();
    let sid = agent
        .session_new_authed(cwd, mcp, true)
        .await?
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    let r = agent.prompt(&sid, prompt).await;
    let _ = agent.call("session/close", json!({ "sessionId": sid })).await;
    agent.kill().await;
    state.acp.lock().remove(&agent.id);
    r
}

/// Shared handle for the pipeline so agents register in AppState (for kill-all).
pub fn register_agent(state: &AppState, agent: Arc<AcpAgent>) {
    state.acp.lock().insert(agent.id.clone(), agent);
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}
