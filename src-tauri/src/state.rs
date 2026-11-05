use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub struct AppState {
    pub acp: Mutex<HashMap<String, Arc<crate::acp::AcpAgent>>>,
    pub ptys: Mutex<HashMap<String, Arc<crate::pty::PtySession>>>,
    pub workers: Mutex<HashMap<String, crate::workers::Worker>>,
    pub research: Mutex<HashMap<String, crate::research::ResearchRun>>,
    pub develop: Mutex<HashMap<String, crate::develop::DevelopRun>>,
    pub hook_server: Mutex<Option<crate::hooks::HookServer>>,
    pub settings: Mutex<Settings>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            acp: Mutex::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
            research: Mutex::new(HashMap::new()),
            develop: Mutex::new(HashMap::new()),
            hook_server: Mutex::new(None),
            settings: Mutex::new(Settings::load()),
            http: reqwest::Client::builder()
                .user_agent("devil-of-devin/0.1")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub devin_path: String,
    pub default_model: String,
    pub default_permission_mode: String,
    pub hook_port: u16,
    pub auto_install_hooks: bool,
    pub skills_dirs: Vec<String>,
    pub github_token: Option<String>,
    /// Extra MCP server configs injected into every worker's session/new.
    /// (ACP `mcpServers` array — stdio `{name,command,args,env}` or http `{name,url,headers}`)
    pub mcp_servers: Vec<serde_json::Value>,
    /// Extra env vars injected into every spawned worker process.
    pub worker_env: std::collections::HashMap<String, String>,
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
            github_token: None,
            mcp_servers: Vec::new(),
            worker_env: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    pub fn path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("devil-of-devin")
            .join("settings.json")
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
