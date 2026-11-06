use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub struct AppState {
    pub acp: Mutex<HashMap<String, Arc<crate::acp::AcpAgent>>>,
    pub ptys: Mutex<HashMap<String, Arc<crate::pty::PtySession>>>,
    pub workers: Mutex<HashMap<String, crate::workers::Worker>>,
    pub research: Mutex<HashMap<String, crate::research::ResearchRun>>,
    pub loops: Mutex<HashMap<String, crate::looprun::LoopRun>>,
    /// Stop flags for running loops (run id → requested stop kind).
    pub loop_ctl: Mutex<HashMap<String, crate::looprun::LoopControl>>,
    pub hook_server: Mutex<Option<crate::hooks::HookServer>>,
    pub settings: Mutex<Settings>,
    pub http: reqwest::Client,
    /// Serializes heavy gate commands (builds, browsers) across loops.
    pub heavy: tokio::sync::Semaphore,
}

impl AppState {
    pub fn new() -> Self {
        let settings = Settings::load();
        let heavy = settings.loop_heavy_concurrency.max(1) as usize;
        Self {
            acp: Mutex::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
            research: Mutex::new(HashMap::new()),
            loops: Mutex::new(HashMap::new()),
            loop_ctl: Mutex::new(HashMap::new()),
            hook_server: Mutex::new(None),
            settings: Mutex::new(settings),
            http: reqwest::Client::builder()
                .user_agent("devil-of-devin/0.2")
                .build()
                .unwrap_or_default(),
            heavy: tokio::sync::Semaphore::new(heavy),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub devin_path: String,
    pub default_model: String,
    /// ACP session mode id: accept-edits | smart | ask | plan | bypass
    pub default_permission_mode: String,
    pub hook_port: u16,
    pub auto_install_hooks: bool,
    pub skills_dirs: Vec<String>,
    /// Extra MCP server configs injected into every worker's session/new.
    /// (ACP `mcpServers` array — stdio `{name,command,args,env}` or http `{name,url,headers}`)
    /// String values may use `$secret:NAME`.
    pub mcp_servers: Vec<serde_json::Value>,
    /// Extra env vars injected into every spawned worker process.
    pub worker_env: std::collections::HashMap<String, String>,
    pub projects: Vec<Project>,
    pub active_project: Option<String>,
    // ---- loop engine ----
    pub loop_concurrency: u32,
    pub loop_heavy_concurrency: u32,
    pub loop_use_worktrees: bool,
    pub loop_max_attempts: u32,
    pub loop_gate_timeout_secs: u64,
    pub loop_implementer_model: String,
    pub loop_validator_model: String,
    pub loop_mode: String,
    /// Pin for the harness skill pack (git ref), empty = default branch HEAD.
    pub harness_ref: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            devin_path: String::new(),
            default_model: "swe-2-max".into(),
            default_permission_mode: "smart".into(),
            hook_port: 0,
            auto_install_hooks: true,
            skills_dirs: Vec::new(),
            mcp_servers: Vec::new(),
            worker_env: std::collections::HashMap::new(),
            projects: Vec::new(),
            active_project: None,
            loop_concurrency: 2,
            loop_heavy_concurrency: 1,
            loop_use_worktrees: true,
            loop_max_attempts: 3,
            loop_gate_timeout_secs: 1800,
            loop_implementer_model: String::new(),
            loop_validator_model: String::new(),
            loop_mode: "smart".into(),
            harness_ref: String::new(),
        }
    }
}

impl Settings {
    pub fn dir() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("devil-of-devin")
    }
    pub fn path() -> std::path::PathBuf {
        Self::dir().join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
}

#[tauri::command]
pub fn settings_get(state: tauri::State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn settings_set(state: tauri::State<'_, AppState>, settings: Settings) {
    settings.save();
    *state.settings.lock() = settings;
}
