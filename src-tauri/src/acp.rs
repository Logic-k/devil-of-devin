use anyhow::{anyhow, Context};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

/// One spawned `devin acp` subprocess speaking JSON-RPC over stdio.
pub struct AcpAgent {
    pub id: String,
    pub cwd: String,
    pub model: Option<String>,
    child: AsyncMutex<Child>,
    stdin: AsyncMutex<ChildStdin>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    pub sessions: Mutex<HashSet<String>>,
    pub init: Mutex<Option<Value>>,
    /// When true, permission requests are auto-answered with the most permissive
    /// `allow_*` option instead of being forwarded to the UI.
    pub auto_approve: std::sync::atomic::AtomicBool,
    /// Set once `authenticate` succeeded (token or browser) for this process.
    pub authed: std::sync::atomic::AtomicBool,
    pub app: AppHandle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpawnOpts {
    pub cwd: String,
    pub model: Option<String>,
    pub agent_type: Option<String>,
    pub cloud: bool,
    pub auto_approve: bool,
    pub env: HashMap<String, String>,
}

impl AcpAgent {
    /// Spawn `devin acp`, start the stdout reader, run `initialize`.
    pub async fn spawn(
        state: &crate::state::AppState,
        app: AppHandle,
        id: String,
        opts: SpawnOpts,
    ) -> anyhow::Result<Arc<AcpAgent>> {
        let devin = crate::devin_cli::find_devin(&state.settings.lock().devin_path)
            .ok_or_else(|| anyhow!("devin CLI not found — set devin_path in settings"))?;

        let mut cmd = Command::new(devin);
        cmd.arg("acp");
        if let Some(m) = &opts.model {
            cmd.args(["--model", m]);
        }
        if let Some(t) = &opts.agent_type {
            cmd.args(["--agent-type", t]);
        }
        if opts.cloud {
            cmd.arg("--cloud");
        }
        cmd.current_dir(&opts.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().context("spawn devin acp")?;
        let stdin = child.stdin.take().context("stdin")?;
        let stdout = child.stdout.take().context("stdout")?;

        let agent = Arc::new(AcpAgent {
            id: id.clone(),
            cwd: opts.cwd.clone(),
            model: opts.model.clone(),
            child: AsyncMutex::new(child),
            stdin: AsyncMutex::new(stdin),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashSet::new()),
            init: Mutex::new(None),
            auto_approve: std::sync::atomic::AtomicBool::new(opts.auto_approve),
            authed: std::sync::atomic::AtomicBool::new(false),
            app: app.clone(),
        });

        // stdout reader task
        {
            let agent = agent.clone();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => agent.on_line(&line).await,
                        _ => break,
                    }
                }
                let code = {
                    let mut c = agent.child.lock().await;
                    c.try_wait().ok().flatten().and_then(|s| s.code())
                };
                let _ = agent.app.emit(
                    &format!("acp://exit/{}", agent.id),
                    json!({ "code": code }),
                );
            });
        }

        // initialize handshake
        let init = agent
            .call(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": false },
                        "terminal": false
                    },
                    "clientInfo": { "name": "devil-of-devin", "title": "Devil of Devin", "version": "0.1.0" }
                }),
            )
            .await
            .map_err(|e| anyhow!("initialize failed: {e}"))?;
        *agent.init.lock() = Some(init);

        // proactive auth via stored session token (never opens a browser here;
        // the interactive fallback stays in session_new_authed)
        if let Some(k) = read_session_api_key() {
            if authenticate_apikey(&agent, &k).await.is_ok() {
                agent.authed.store(true, Ordering::SeqCst);
                let _ = agent.app.emit(
                    &format!("acp://log/{}", agent.id),
                    json!({ "line": "acp authed via session token" }),
                );
            }
        }

        Ok(agent)
    }

    async fn on_line(&self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                let _ = self.app.emit(
                    &format!("acp://log/{}", self.id),
                    json!({ "line": line }),
                );
                return;
            }
        };
        let _ = self
            .app
            .emit(&format!("acp://raw/{}", self.id), msg.clone());

        let is_request = msg.get("method").is_some() && msg.get("id").is_some();
        let is_notify = msg.get("method").is_some() && msg.get("id").is_none();
        let is_response = msg.get("method").is_none() && msg.get("id").is_some();

        if is_response {
            let id = msg.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            let res = if let Some(err) = msg.get("error") {
                Err(err.get("message").and_then(|m| m.as_str()).unwrap_or("acp error").to_string())
            } else {
                Ok(msg.get("result").cloned().unwrap_or(Value::Null))
            };
            if let Some(tx) = self.pending.lock().remove(&id) {
                let _ = tx.send(res);
            }
            return;
        }

        if is_notify {
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            if method == "session/update" {
                let _ = self.app.emit(
                    &format!("acp://update/{}", self.id),
                    params.clone(),
                );
            } else {
                let _ = self.app.emit(
                    &format!("acp://notify/{}", self.id),
                    json!({ "method": method, "params": params }),
                );
            }
            return;
        }

        if is_request {
            let id = msg["id"].clone();
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            match method.as_str() {
                "session/request_permission" => {
                    if self.auto_approve.load(Ordering::SeqCst) {
                        let options = params
                            .get("options")
                            .and_then(|o| o.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let pick = options
                            .iter()
                            .find(|o| {
                                o.get("kind").and_then(|k| k.as_str()) == Some("allow_always")
                            })
                            .or_else(|| {
                                options.iter().find(|o| {
                                    o.get("kind")
                                        .and_then(|k| k.as_str())
                                        .map(|k| k.starts_with("allow"))
                                        .unwrap_or(false)
                                })
                            })
                            .or_else(|| options.first());
                        if let Some(opt) = pick {
                            let oid = opt
                                .get("optionId")
                                .cloned()
                                .or_else(|| opt.get("id").cloned())
                                .unwrap_or(Value::Null);
                            let resp = json!({
                                "jsonrpc": "2.0", "id": id,
                                "result": { "outcome": { "outcome": "selected", "optionId": oid } }
                            });
                            let _ = self.write_line(&resp.to_string()).await;
                            let _ = self.app.emit(
                                &format!("acp://auto-permission/{}", self.id),
                                json!({ "params": params, "picked": oid }),
                            );
                            return;
                        }
                    }
                    let _ = self.app.emit(
                        &format!("acp://permission/{}", self.id),
                        json!({ "rpcId": id, "params": params }),
                    );
                }
                "elicitation/create" => {
                    let _ = self.app.emit(
                        &format!("acp://elicitation/{}", self.id),
                        json!({ "rpcId": id, "params": params }),
                    );
                }
                _ => {
                    // Not handled (fs/*, terminal/*) — we didn't advertise them; refuse politely.
                    let resp = json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": { "code": -32601, "message": format!("method not implemented by client: {method}") }
                    });
                    let _ = self.write_line(&resp.to_string()).await;
                }
            }
        }
    }

    pub async fn write_line(&self, line: &str) -> anyhow::Result<()> {
        let mut w = self.stdin.lock().await;
        w.write_all(line.as_bytes()).await?;
        w.write_all(b"\n").await?;
        w.flush().await?;
        Ok(())
    }

    /// JSON-RPC call → await response.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_line(&req.to_string())
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|_| "agent closed".to_string())?
    }

    /// Raw respond to an agent→client request (permission etc).
    pub async fn respond(&self, rpc_id: Value, result: Value) -> anyhow::Result<()> {
        let resp = json!({ "jsonrpc": "2.0", "id": rpc_id, "result": result });
        self.write_line(&resp.to_string()).await
    }

    #[allow(dead_code)]
    pub async fn session_new(&self, cwd: &str) -> Result<Value, String> {
        self.session_new_mcp(cwd, vec![]).await
    }

    pub async fn session_new_mcp(&self, cwd: &str, mcp_servers: Vec<Value>) -> Result<Value, String> {
        let r = self
            .call("session/new", json!({ "cwd": cwd, "mcpServers": mcp_servers }))
            .await?;
        if let Some(sid) = r.get("sessionId").and_then(|s| s.as_str()) {
            self.sessions.lock().insert(sid.to_string());
        }
        Ok(r)
    }

    /// True if the error looks like an auth_required / not-logged-in response.
    pub fn is_auth_error(e: &str) -> bool {
        let l = e.to_lowercase();
        l.contains("auth") || l.contains("login") || l.contains("credential")
    }

    /// Trigger the `authenticate` flow with the first advertised method
    /// (Devin: `devin-browser` — opens the user's browser).
    pub async fn authenticate(&self) -> Result<Value, String> {
        let method_id = self
            .init
            .lock()
            .as_ref()
            .and_then(|i| i.get("authMethods")?.as_array()?.first()?.get("id")?.as_str())
            .unwrap_or("devin-browser")
            .to_string();
        self.call("authenticate", json!({ "methodId": method_id })).await
    }

    /// Resume an existing session (replays history via session/update events).
    pub async fn session_load(&self, session_id: &str, cwd: &str) -> Result<Value, String> {
        let r = self
            .call(
                "session/load",
                json!({ "sessionId": session_id, "cwd": cwd, "mcpServers": [] }),
            )
            .await?;
        self.sessions.lock().insert(session_id.to_string());
        Ok(r)
    }

    /// session/new, and if it fails with an auth error, authenticate (session
    /// token first, browser fallback) then retry.
    /// `trigger_auth` controls whether any auth flow may be launched.
    pub async fn session_new_authed(
        &self,
        cwd: &str,
        mcp_servers: Vec<Value>,
        trigger_auth: bool,
    ) -> Result<Value, String> {
        match self.session_new_mcp(cwd, mcp_servers.clone()).await {
            Ok(v) => Ok(v),
            Err(e) if trigger_auth && Self::is_auth_error(&e) => {
                ensure_authed(self).await?;
                self.session_new_mcp(cwd, mcp_servers).await
            }
            Err(e) => Err(e),
        }
    }

    pub async fn prompt(&self, session_id: &str, text: &str) -> Result<Value, String> {
        self.call(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": text }]
            }),
        )
        .await
    }

    pub async fn cancel(&self, session_id: &str) -> anyhow::Result<()> {
        self.write_line(
            &json!({
                "jsonrpc": "2.0", "method": "session/cancel",
                "params": { "sessionId": session_id }
            })
            .to_string(),
        )
        .await
    }

    pub async fn kill(&self) {
        let _ = self.child.lock().await.kill().await;
    }
}

/// Read the IDE-shared session token (`windsurf_api_key` in credentials.toml).
/// ACP accepts it via `authenticate._meta.api_key` — no browser needed.
pub fn read_session_api_key() -> Option<String> {
    let p = dirs::config_dir()?.join(r"devin\credentials.toml");
    let t = std::fs::read_to_string(p).ok()?;
    for line in t.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "windsurf_api_key" {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Authenticate via `_meta.api_key` (session token — no browser popup).
pub async fn authenticate_apikey(agent: &AcpAgent, key: &str) -> Result<Value, String> {
    agent
        .call(
            "authenticate",
            json!({ "methodId": "devin-browser", "_meta": { "api_key": key } }),
        )
        .await
}

/// Best-effort ACP auth: session token first, browser PKCE as fallback.
/// Call right after `initialize` (or lazily on the first auth error).
pub async fn ensure_authed(agent: &AcpAgent) -> Result<(), String> {
    if agent.authed.load(Ordering::SeqCst) {
        return Ok(());
    }
    if let Some(k) = read_session_api_key() {
        match authenticate_apikey(agent, &k).await {
            Ok(_) => {
                agent.authed.store(true, Ordering::SeqCst);
                return Ok(());
            }
            Err(e) => {
                let _ = agent.app.emit(
                    &format!("acp://log/{}", agent.id),
                    json!({ "line": format!("api_key auth failed: {e}") }),
                );
            }
        }
    }
    agent.authenticate().await?;
    agent.authed.store(true, Ordering::SeqCst);
    Ok(())
}

fn get_agent(state: &crate::state::AppState, id: &str) -> Result<Arc<AcpAgent>, String> {
    state
        .acp
        .lock()
        .get(id)
        .cloned()
        .ok_or_else(|| format!("no agent {id}"))
}

// ---------- commands ----------

#[tauri::command]
pub async fn acp_spawn(
    state: tauri::State<'_, crate::state::AppState>,
    app: AppHandle,
    opts: SpawnOpts,
) -> Result<Value, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let agent = AcpAgent::spawn(&state, app, id.clone(), opts)
        .await
        .map_err(|e| e.to_string())?;
    let init = agent.init.lock().clone();
    state.acp.lock().insert(id.clone(), agent);
    Ok(json!({ "agentId": id, "init": init }))
}

#[tauri::command]
pub async fn acp_call(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    method: String,
    params: Value,
) -> Result<Value, String> {
    get_agent(&state, &agent)?.call(&method, params).await
}

#[tauri::command]
pub async fn acp_notify(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    method: String,
    params: Value,
) -> Result<(), String> {
    let a = get_agent(&state, &agent)?;
    a.write_line(&json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn acp_session_new(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    cwd: String,
    mcp_servers: Option<Vec<Value>>,
    trigger_auth: Option<bool>,
) -> Result<Value, String> {
    get_agent(&state, &agent)?
        .session_new_authed(&cwd, mcp_servers.unwrap_or_default(), trigger_auth.unwrap_or(true))
        .await
}

#[tauri::command]
pub async fn acp_session_load(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    session: String,
    cwd: String,
) -> Result<Value, String> {
    get_agent(&state, &agent)?.session_load(&session, &cwd).await
}

#[tauri::command]
pub async fn acp_prompt(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    session: String,
    text: String,
) -> Result<Value, String> {
    get_agent(&state, &agent)?.prompt(&session, &text).await
}

#[tauri::command]
pub async fn acp_cancel(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    session: String,
) -> Result<(), String> {
    get_agent(&state, &agent)?
        .cancel(&session)
        .await
        .map_err(|e| e.to_string())
}

/// Answer a pending `session/request_permission` (or elicitation) from the agent.
#[tauri::command]
pub async fn acp_permission_respond(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
    rpc_id: Value,
    result: Value,
) -> Result<(), String> {
    get_agent(&state, &agent)?
        .respond(rpc_id, result)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn acp_kill(
    state: tauri::State<'_, crate::state::AppState>,
    agent: String,
) -> Result<(), String> {
    let removed = state.acp.lock().remove(&agent);
    if let Some(a) = removed {
        a.kill().await;
    }
    Ok(())
}

#[tauri::command]
pub fn acp_list(state: tauri::State<'_, crate::state::AppState>) -> Value {
    let g = state.acp.lock();
    json!(g
        .iter()
        .map(|(id, a)| {
            json!({
                "id": id, "cwd": a.cwd, "model": a.model,
                "sessions": a.sessions.lock().iter().cloned().collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>())
}
