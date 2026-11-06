//! Loop engine — the app plays the role the harness assigns to Orca/the
//! coordinator session: pick ready tasks from `plan_setup/`, run an
//! implementer worker per task (in its own git worktree), run the gates
//! `lint → unit → e2e` from `harness_setup/quality_gates/`, hand the result
//! to an independent validator session (`task_validation`), integrate on PASS,
//! keep `plan_setup/STATE.md` / `LOG.md` current, and never ask the user.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::workers::{now_iso, run_oneshot, OneshotOpts};

// ---------------------------------------------------------------- models

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopConfig {
    pub project: String,
    #[serde(default)]
    pub concurrency: Option<u32>,
    #[serde(default)]
    pub use_worktrees: Option<bool>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub implementer_model: Option<String>,
    #[serde(default)]
    pub validator_model: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub budget_minutes: Option<u64>,
    /// Only run tasks of this phase (e.g. "01").
    #[serde(default)]
    pub phase: Option<String>,
    /// Only run these task ids (and nothing else).
    #[serde(default)]
    pub task_ids: Option<Vec<String>>,
    #[serde(default)]
    pub resume: bool,
    /// Skip the independent validator (not harness-compliant; for dry runs).
    #[serde(default)]
    pub skip_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub concurrency: u32,
    pub use_worktrees: bool,
    pub max_attempts: u32,
    pub implementer_model: String,
    pub validator_model: String,
    pub mode: String,
    pub gate_timeout_secs: u64,
    pub budget_minutes: Option<u64>,
    pub is_git: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub id: String,
    pub rel_path: String,
    pub phase: String,
    pub title: String,
    pub status: String,
    pub depends_on: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub verification: Vec<String>,
    pub files: Vec<String>,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub gate: String,
    pub pass: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub output_tail: String,
    pub duration_ms: u64,
    pub at: String,
    pub attempt: u32,
    pub agent_judged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub pass: bool,
    pub report_path: String,
    pub summary: String,
    pub attempt: u32,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunState {
    pub id: String,
    pub title: String,
    pub status: String,
    pub attempts: u32,
    pub strategy_switch: bool,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub agent_id: Option<String>,
    pub gates: Vec<GateResult>,
    pub validation: Option<ValidationResult>,
    pub attempt_notes: Vec<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub note: Option<String>,
    pub usage_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub at: String,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopRun {
    pub id: String,
    pub project: String,
    pub config: LoopConfig,
    pub resolved: Resolved,
    /// preflight | running | stopping | done | stopped | failed
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub tasks: BTreeMap<String, TaskRunState>,
    pub all_tasks: Vec<TaskInfo>,
    pub gates: Vec<String>,
    pub log: Vec<LogLine>,
    pub summary: Option<String>,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopControl {
    /// Finish running tasks, assign nothing new.
    Stop,
    /// Kill workers now.
    Abort,
}

// ---------------------------------------------------------------- task files

fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let t = text.trim_start_matches('\u{feff}');
    let rest = t.strip_prefix("---")?;
    let rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))?;
    let end = rest.find("\n---")?;
    let fm = &rest[..end];
    let body = &rest[end + 4..];
    Some((fm, body))
}

fn yaml_list(v: Option<&serde_yaml::Value>) -> Vec<String> {
    match v {
        Some(serde_yaml::Value::Sequence(s)) => s
            .iter()
            .filter_map(|x| match x {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                other => serde_yaml::to_string(other).ok().map(|s| s.trim().to_string()),
            })
            .collect(),
        Some(serde_yaml::Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

fn yaml_str(v: Option<&serde_yaml::Value>) -> Option<String> {
    match v {
        Some(serde_yaml::Value::String(s)) => Some(s.clone()),
        Some(serde_yaml::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_task(root: &Path, path: &Path) -> Option<TaskInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    let (fm, body) = split_frontmatter(&text)?;
    let y: serde_yaml::Value = serde_yaml::from_str(fm).ok()?;
    let m = y.as_mapping()?;
    let get = |k: &str| m.get(serde_yaml::Value::String(k.into()));
    let id = yaml_str(get("id"))?;
    let status = yaml_str(get("status")).unwrap_or_else(|| "pending".into());
    let title = body
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| id.clone());
    let phase = yaml_str(get("phase")).unwrap_or_else(|| {
        id.strip_prefix("phase-")
            .and_then(|r| r.split('-').next())
            .unwrap_or("")
            .to_string()
    });
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    Some(TaskInfo {
        id,
        rel_path: rel,
        phase,
        title,
        status,
        depends_on: yaml_list(get("depends_on")),
        acceptance_criteria: yaml_list(get("acceptance_criteria")),
        verification: yaml_list(get("verification")),
        files: yaml_list(get("files")),
        blocked_by: yaml_list(get("blocked_by")),
    })
}

fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    if depth > 5 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, out);
        } else if p.extension().map(|x| x == "md").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// All task files under plan_setup/ (any depth) that carry `id:` frontmatter.
pub fn scan_tasks(project: &str) -> Vec<TaskInfo> {
    let root = Path::new(project);
    let mut files = Vec::new();
    walk(&root.join("plan_setup"), 0, &mut files);
    let mut tasks: Vec<TaskInfo> = files
        .iter()
        .filter_map(|p| parse_task(root, p))
        .filter(|t| t.id.contains("task"))
        .collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    tasks
}

/// Rewrite the `status:` line inside the frontmatter, preserving everything else.
fn set_task_status(project: &str, rel: &str, status: &str) -> Result<(), String> {
    let p = Path::new(project).join(rel);
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_fm = false;
    let mut done = false;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if i == 0 && trimmed.trim_start_matches('\u{feff}') == "---" {
            in_fm = true;
            out.push_str(line);
            continue;
        }
        if in_fm && trimmed == "---" {
            in_fm = false;
        }
        if in_fm && !done && trimmed.starts_with("status:") {
            let eol = if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("status: {status}{eol}"));
            done = true;
            continue;
        }
        out.push_str(line);
    }
    if !done {
        return Err(format!("{rel}: frontmatter에 status가 없습니다"));
    }
    std::fs::write(&p, out).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- gates

#[derive(Debug, Clone)]
pub struct GateDef {
    pub name: String,
    pub rel_path: String,
    pub command: Option<String>,
    pub shell: String,
    pub cwd: Option<String>,
    pub body: String,
}

fn canonical_gate_name(stem: &str) -> Option<&'static str> {
    let s = stem.to_lowercase();
    if s.contains("task_validation") || s.contains("validation") {
        Some("task_validation")
    } else if s.contains("e2e") {
        Some("e2e")
    } else if s.contains("unit") {
        Some("unit")
    } else if s.contains("lint") {
        Some("lint")
    } else {
        None
    }
}

/// Extract an executable command from a gate definition: frontmatter
/// `command:` (string or list) + optional `shell:`/`cwd:`, else the first
/// fenced ```bash|sh|shell|powershell|pwsh|cmd block in the body.
fn parse_gate(root: &Path, path: &Path) -> Option<GateDef> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let name = canonical_gate_name(&stem)?;
    let text = std::fs::read_to_string(path).ok()?;
    let (fm, body) = split_frontmatter(&text).unwrap_or(("", text.as_str()));
    let mut command: Option<String> = None;
    let mut shell = String::new();
    let mut cwd: Option<String> = None;
    if !fm.is_empty() {
        if let Ok(serde_yaml::Value::Mapping(m)) = serde_yaml::from_str::<serde_yaml::Value>(fm) {
            let get = |k: &str| m.get(serde_yaml::Value::String(k.into()));
            let cmds = yaml_list(get("command"));
            if !cmds.is_empty() {
                command = Some(cmds.join("\n"));
            }
            shell = yaml_str(get("shell")).unwrap_or_default();
            cwd = yaml_str(get("cwd"));
        }
    }
    if command.is_none() {
        let mut lines = body.lines();
        while let Some(l) = lines.next() {
            let t = l.trim();
            if let Some(lang) = t.strip_prefix("```") {
                let lang = lang.trim().to_lowercase();
                let lang = lang.split_whitespace().next().unwrap_or("").to_string();
                let known = ["bash", "sh", "shell", "powershell", "pwsh", "cmd", "bat", "console"];
                if !known.contains(&lang.as_str()) {
                    // skip this block
                    for l2 in lines.by_ref() {
                        if l2.trim().starts_with("```") {
                            break;
                        }
                    }
                    continue;
                }
                let mut buf = Vec::new();
                for l2 in lines.by_ref() {
                    if l2.trim().starts_with("```") {
                        break;
                    }
                    let c = l2.trim_start().trim_start_matches("$ ").trim_start_matches("> ");
                    if c.trim().is_empty() || c.trim_start().starts_with('#') {
                        continue;
                    }
                    buf.push(c.to_string());
                }
                if !buf.is_empty() {
                    command = Some(buf.join("\n"));
                    if shell.is_empty() {
                        shell = match lang.as_str() {
                            "powershell" | "pwsh" => "pwsh".into(),
                            "cmd" | "bat" => "cmd".into(),
                            _ => "bash".into(),
                        };
                    }
                    break;
                }
            }
        }
    }
    if shell.is_empty() {
        shell = "bash".into();
    }
    Some(GateDef {
        name: name.to_string(),
        rel_path: path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"),
        command,
        shell,
        cwd,
        body: body.to_string(),
    })
}

pub fn load_gates(project: &str) -> HashMap<String, GateDef> {
    let root = Path::new(project);
    let dir = root.join("harness_setup").join("quality_gates");
    let mut out = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.extension().map(|x| x == "md" || x == "yaml" || x == "yml").unwrap_or(false) {
                if let Some(g) = parse_gate(root, &p) {
                    out.entry(g.name.clone()).or_insert(g);
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn hide_window(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
fn hide_window(_cmd: &mut tokio::process::Command) {}

fn tail(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let start = s.len() - n;
        let start = (start..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(start);
        format!("…{}", &s[start..])
    }
}

/// Run a shell snippet with a timeout; returns (exit code, combined output).
async fn run_shell(
    shell: &str,
    script: &str,
    cwd: &Path,
    timeout: Duration,
) -> (Option<i32>, String, bool) {
    // scripts live inside the workspace (.devil/tmp) — %TEMP% may contain a
    // non-ASCII user name that gets mangled on the way into the child shell
    let tmp_dir = cwd.join(".devil").join("tmp");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let tmp = tmp_dir.join(format!("gate-{}", &uuid::Uuid::new_v4().to_string()[..8]));
    let (file, mut cmd) = match shell {
        "pwsh" | "powershell" => {
            let f = tmp.with_extension("ps1");
            let exe = if which("pwsh") { "pwsh" } else { "powershell" };
            let mut c = tokio::process::Command::new(exe);
            c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]);
            c.arg(&f);
            (f, c)
        }
        "cmd" => {
            let f = tmp.with_extension("cmd");
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C");
            c.arg(&f);
            (f, c)
        }
        _ => {
            let f = tmp.with_extension("sh");
            // never a bare "bash": on Windows that resolves to the WSL launcher
            // (System32\bash.exe) where C:/… paths do not exist
            let mut c = tokio::process::Command::new(git_bash());
            // Git Bash mangles backslash paths ("C:Users…") — hand it C:/… instead
            c.arg(f.to_string_lossy().replace('\\', "/"));
            (f, c)
        }
    };
    let content = match shell {
        "pwsh" | "powershell" => format!("$ErrorActionPreference='Stop'\n{script}\n"),
        "cmd" => format!("@echo off\r\n{}\r\n", script.replace('\n', "\r\n")),
        _ => format!("set -e\n{script}\n"),
    };
    if let Err(e) = std::fs::write(&file, content) {
        return (None, format!("스크립트 작성 실패: {e}"), false);
    }
    cmd.current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env("CI", "1")
        .kill_on_drop(true);
    hide_window(&mut cmd);
    let res = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(o)) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.code(), s, false)
        }
        Ok(Err(e)) => (None, format!("실행 실패: {e}"), false),
        Err(_) => (None, format!("시간 초과 ({}s)", timeout.as_secs()), true),
    };
    let _ = std::fs::remove_file(&file);
    res
}

/// Locate Git for Windows' bash (the one devin hooks run under), skipping the
/// WSL launcher in System32. Falls back to plain `bash` if nothing is found.
pub fn git_bash() -> String {
    if let Ok(p) = std::env::var("DOD_GIT_BASH") {
        if Path::new(&p).exists() {
            return p;
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for base in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(v) = std::env::var(base) {
            candidates.push(PathBuf::from(&v).join("Git").join("bin").join("bash.exe"));
        }
    }
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(v).join("Programs").join("Git").join("bin").join("bash.exe"));
    }
    if let Ok(o) = std::process::Command::new("git").args(["--exec-path"]).output() {
        // …/Git/mingw64/libexec/git-core → …/Git/bin/bash.exe
        let p = String::from_utf8_lossy(&o.stdout).trim().replace('/', "\\");
        if let Some(root) = Path::new(&p).ancestors().nth(3) {
            candidates.push(root.join("bin").join("bash.exe"));
        }
    }
    if let Ok(paths) = std::env::var("PATH") {
        for d in paths.split(';') {
            if d.to_lowercase().contains("system32") {
                continue;
            }
            candidates.push(Path::new(d).join("bash.exe"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "bash".into())
}

fn which(exe: &str) -> bool {
    std::env::var("PATH")
        .map(|p| {
            p.split(';').any(|d| {
                let d = Path::new(d);
                d.join(format!("{exe}.exe")).exists() || d.join(exe).exists()
            })
        })
        .unwrap_or(false)
}

fn parse_result_line(text: &str) -> Option<bool> {
    for line in text.lines().rev() {
        let t = line.trim().trim_start_matches('*').trim_end_matches('*').trim();
        let t = t.trim_start_matches('`').trim_end_matches('`');
        if let Some(r) = t.strip_prefix("RESULT:") {
            let r = r.trim().to_uppercase();
            if r.starts_with("PASS") {
                return Some(true);
            }
            if r.starts_with("FAIL") {
                return Some(false);
            }
        }
    }
    None
}

// ---------------------------------------------------------------- git helpers

/// Commit everything except `.devil/` (coordinator artifacts) in `cwd`, if anything changed.
async fn auto_commit(cwd: &Path, msg: &str) {
    let _ = git(cwd, &["add", "-A", "--", ".", ":(exclude).devil"]).await;
    if let Ok(st) = git(cwd, &["diff", "--cached", "--quiet"]).await {
        let _ = st; // nothing staged
        return;
    }
    let _ = git(cwd, &["commit", "-q", "-m", msg]).await;
}

/// Keep `.devil/` out of the project's git status without touching its .gitignore.
fn exclude_devil_dir(root: &Path) {
    let p = root.join(".git").join("info").join("exclude");
    if let Ok(cur) = std::fs::read_to_string(&p) {
        if cur.lines().any(|l| l.trim() == ".devil/") {
            return;
        }
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, ".devil/");
    }
}

async fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut c = tokio::process::Command::new("git");
    c.args(args).current_dir(cwd).stdin(std::process::Stdio::null());
    hide_window(&mut c);
    let o = c.output().await.map_err(|e| format!("git 실행 실패: {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    if o.status.success() {
        Ok(out)
    } else {
        Err(format!("git {}: {}{}", args.join(" "), out.trim(), err.trim()))
    }
}

async fn is_git_repo(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"])
        .await
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

// ---------------------------------------------------------------- run bookkeeping

fn run_dir(project: &str) -> PathBuf {
    Path::new(project).join(".devil").join("loop")
}

fn emit_run(app: &AppHandle, run: &LoopRun) {
    let _ = app.emit("loop://event", json!({ "kind": "snapshot", "run": run }));
    let dir = run_dir(&run.project);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(s) = serde_json::to_string_pretty(run) {
        let _ = std::fs::write(dir.join(format!("run-{}.json", run.id)), &s);
        let _ = std::fs::write(dir.join("run-latest.json"), s);
    }
}

fn with_run<F: FnOnce(&mut LoopRun)>(state: &AppState, app: &AppHandle, id: &str, f: F) {
    let mut g = state.loops.lock();
    if let Some(r) = g.get_mut(id) {
        f(r);
        let snap = r.clone();
        drop(g);
        emit_run(app, &snap);
    }
}

fn log(state: &AppState, app: &AppHandle, id: &str, msg: impl Into<String>) {
    let m = msg.into();
    tracing::info!(target: "loop", "{m}");
    let project = state.loops.lock().get(id).map(|r| r.project.clone());
    if let Some(p) = project {
        append_log_md(&p, &m);
    }
    with_run(state, app, id, |r| {
        r.log.push(LogLine { at: now_iso(), msg: m });
        if r.log.len() > 2000 {
            r.log.drain(0..500);
        }
    });
}

fn append_log_md(project: &str, msg: &str) {
    let p = Path::new(project).join("plan_setup").join("LOG.md");
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "- {} [devil-loop] {}", now_iso(), msg);
    }
}

/// Rewrite `plan_setup/STATE.md` from the run snapshot (single writer: the coordinator).
fn write_state_md(run: &LoopRun) {
    let p = Path::new(&run.project).join("plan_setup").join("STATE.md");
    let mut s = String::new();
    s.push_str("# STATE\n\n");
    s.push_str(&format!(
        "> Devil of Devin Loop가 갱신함 · run `{}` · 상태 **{}** · 갱신 {}\n\n",
        run.id, run.status, now_iso()
    ));
    s.push_str("## 현재 Loop\n\n");
    s.push_str(&format!("- 시작: {}\n", run.started_at));
    s.push_str(&format!(
        "- 실행 수단: Devil of Devin (devin acp 워커, 워크트리 {} , 동시 {})\n",
        if run.resolved.use_worktrees { "사용" } else { "미사용" },
        run.resolved.concurrency
    ));
    s.push_str(&format!(
        "- 모델: 구현 `{}` / 검증 `{}` · 모드 `{}`\n",
        run.resolved.implementer_model, run.resolved.validator_model, run.resolved.mode
    ));
    if let Some(b) = run.resolved.budget_minutes {
        s.push_str(&format!("- 예산: {b}분\n"));
    }
    s.push_str(&format!("- 누적 토큰: {}\n\n", run.total_tokens));

    s.push_str("## 진행 중 태스크\n\n");
    let mut any = false;
    for t in run.tasks.values().filter(|t| t.status == "in_progress") {
        any = true;
        s.push_str(&format!(
            "- `{}` {} — 시도 {} · 워크트리 `{}` · 브랜치 `{}` · 시작 {}\n",
            t.id,
            t.title,
            t.attempts,
            t.worktree.as_deref().unwrap_or("-"),
            t.branch.as_deref().unwrap_or("-"),
            t.started_at.as_deref().unwrap_or("-")
        ));
    }
    if !any {
        s.push_str("- 없음\n");
    }
    s.push_str("\n## 태스크 상태\n\n| id | 상태 | 시도 | 게이트 | 검증 | 비고 |\n|---|---|---|---|---|---|\n");
    for t in run.all_tasks.iter() {
        let rs = run.tasks.get(&t.id);
        let gates = rs
            .map(|r| {
                let last_attempt = r.gates.iter().map(|g| g.attempt).max().unwrap_or(0);
                r.gates
                    .iter()
                    .filter(|g| g.attempt == last_attempt)
                    .map(|g| format!("{}:{}", g.gate, if g.pass { "PASS" } else { "FAIL" }))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let val = rs
            .and_then(|r| r.validation.as_ref())
            .map(|v| if v.pass { "PASS".to_string() } else { "FAIL".to_string() })
            .unwrap_or_default();
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            t.id,
            rs.map(|r| r.status.clone()).unwrap_or(t.status.clone()),
            rs.map(|r| r.attempts).unwrap_or(0),
            gates,
            val,
            rs.and_then(|r| r.note.clone()).unwrap_or_default().replace('|', "/")
        ));
    }
    s.push_str("\n## 다음 행동\n\n");
    match run.status.as_str() {
        "running" | "preflight" | "stopping" => s.push_str("- Loop 실행 중. 앱이 의존이 풀린 태스크를 계속 배정한다.\n"),
        "done" => s.push_str("- 모든 대상 태스크가 종료됨. 최종 보고서는 `.devil/loop/report-*.md`.\n"),
        _ => s.push_str("- 중단됨. 재개: Devil of Devin 루프 보드에서 **재개** (실패 태스크는 시도 횟수를 이어받는다).\n"),
    }
    s.push_str("\n## 재개 지점\n\n");
    s.push_str(&format!(
        "- 런 기록: `.devil/loop/run-{}.json` (시도·게이트·검증 증거 포함)\n",
        run.id
    ));
    s.push_str("- 태스크 파일의 `status:`가 완료 판정의 정본이다. STATE는 재개 기록이다.\n");
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(p, s);
}

// ---------------------------------------------------------------- prompts

fn implementer_prompt(
    run: &LoopRun,
    task: &TaskInfo,
    task_text: &str,
    cwd: &str,
    branch: Option<&str>,
    attempt: u32,
    prev_failure: &str,
) -> String {
    let mut s = String::new();
    s.push_str("당신은 이 프로젝트의 Loop 구현 워커입니다. 사용자는 없습니다 — 질문하지 말고 끝까지 진행하세요.\n\n");
    s.push_str(&format!("- 작업 폴더: `{cwd}`"));
    if let Some(b) = branch {
        s.push_str(&format!(" (git 워크트리, 브랜치 `{b}`)"));
    }
    s.push('\n');
    if cwd != run.project {
        s.push_str(&format!(
            "- 문서 정본(읽기 전용): `{}` (plan_setup/, harness_setup/, loop_setup/, docs/). 워크트리에는 미커밋 문서가 없을 수 있으니 정본을 **읽기만** 하세요. 그 폴더에서 파일을 바꾸거나 git 명령을 실행하지 마세요 — 모든 작업은 작업 폴더에서만 합니다.\n",
            run.project
        ));
    }
    s.push_str("- 먼저 읽을 것: `loop_setup/LOOP.md`, `harness_setup/HARNESS.md`, `docs/PRD.md`의 `## 자율운영 합의`, 그리고 아래 태스크 파일.\n");
    s.push_str("- 수정 금지: `harness_setup/`(게이트 정의 포함), `plan_setup/`, `loop_setup/`, `docs/`. 게이트가 환경 문제로 실패한다고 판단되면 정의를 고치지 말고 attempt 파일에 근거를 적는다. 조정자(앱)가 다음 시도에서 처리한다.\n\n");
    s.push_str(&format!("## 태스크 `{}` ({})\n\n", task.id, task.rel_path));
    s.push_str("```markdown\n");
    s.push_str(task_text.trim());
    s.push_str("\n```\n\n");
    s.push_str("## 지시\n\n");
    s.push_str("1. `acceptance_criteria`를 모두 만족하도록 구현한다. 태스크 범위 밖의 변경은 하지 않는다.\n");
    s.push_str("2. `harness_setup/quality_gates/`의 lint → unit → e2e 검사를 직접 실행해 통과시킨다. 통과를 위해 검사·수용 기준·단언을 약화하거나 대상을 줄이지 않는다.\n");
    s.push_str("3. 변경을 git에 커밋한다 (메시지 예: `loop(");
    s.push_str(&task.id);
    s.push_str("): <요약>`). 커밋되지 않은 변경은 통합되지 않는다. `plan_setup/STATE.md`·`LOG.md`·태스크 `status:`는 조정자(앱)가 관리하므로 수정하지 않는다.\n");
    s.push_str(&format!(
        "4. 마지막으로 `.devil/loop/{}/attempt-{}.md`에 변경 요약·실행한 검증과 결과·남은 문제를 짧게 쓴다.\n",
        task.id, attempt
    ));
    s.push_str("5. 합의 밖 결정이 필요하면 가장 보수적인 선택을 하고 attempt 파일에 근거를 남긴다. 비밀값을 파일·로그에 쓰지 않는다.\n");
    if attempt > 1 {
        s.push_str(&format!("\n## 이전 시도({})의 실패 내용\n\n", attempt - 1));
        s.push_str(prev_failure.trim());
        s.push_str("\n");
    }
    if attempt >= 3 {
        s.push_str(&format!(
            "\n## 전략 전환 (같은 방식의 수정이 {}회 실패)\n\n",
            attempt - 1
        ));
        s.push_str("멈추지 말고 심층 분석으로 전환한다: 실패 관찰 → 가설 → 가설을 가르는 실험 → 근거 있는 다른 접근. ");
        s.push_str("이름만 바꾼 같은 조치, 검사 약화, 재시도 횟수 늘리기는 새 전략이 아니다. 분석 과정을 attempt 파일에 남긴다.\n");
    }
    s.push_str("\n완료하면 응답의 마지막 줄에 정확히 `IMPLEMENT_DONE`을 출력한다.\n");
    s
}

fn validator_prompt(run: &LoopRun, task: &TaskInfo, cwd: &str, attempt: u32) -> String {
    let mut s = String::new();
    s.push_str("당신은 독립 검증자(task_validation)입니다. 이 변경을 구현하지 않았고 구현 대화를 모릅니다. ");
    s.push_str("제품 코드·테스트·수용 기준·통과 단언을 수정하지 않습니다. 사용자는 없습니다 — 질문하지 마세요.\n\n");
    s.push_str(&format!("- 검증 대상 폴더: `{cwd}`\n"));
    if cwd != run.project {
        s.push_str(&format!("- 문서 정본: `{}`\n", run.project));
    }
    s.push_str("- 실행·관찰 방법: `harness_setup/HARNESS.md`, `harness_setup/quality_gates/task_validation.md`(있으면)\n");
    s.push_str(&format!("- 태스크 파일: `{}`\n", task.rel_path));
    s.push_str(&format!("- 현재 시도: {attempt}\n\n"));
    s.push_str("## 수용 기준\n\n");
    for (i, c) in task.acceptance_criteria.iter().enumerate() {
        s.push_str(&format!("{}. {}\n", i + 1, c));
    }
    if task.acceptance_criteria.is_empty() {
        s.push_str("(태스크 파일의 acceptance_criteria를 읽어 사용)\n");
    }
    s.push_str("\n## 할 일\n\n");
    s.push_str("각 기준을 **실제로 실행·관찰**해 기준별 PASS/FAIL과 재현 증거(실행한 명령, 관찰한 출력·화면·저장값)를 적는다. ");
    s.push_str("코드 리뷰, 구현자의 보고, 테스트가 존재한다는 사실만으로 PASS하지 않는다. 종료 코드 0·테스트 0건·전부 skip·서비스 미실행은 PASS가 아니다. ");
    s.push_str("환경 문제로 판정할 수 없으면 그 기준은 FAIL로 두고 이유를 쓴다.\n\n");
    s.push_str("보고 형식: `## 기준별 판정` (번호, PASS/FAIL, 증거) → `## 종합`. ");
    s.push_str("응답의 **마지막 줄**은 정확히 `RESULT: PASS` 또는 `RESULT: FAIL` 이어야 한다 (하나라도 FAIL이면 FAIL).\n");
    s
}

fn gate_agent_prompt(gate: &GateDef, cwd: &str) -> String {
    format!(
        "당신은 검증 게이트 `{}` 실행자입니다. 아래 정의에 적힌 검사를 `{}`에서 실제로 실행하고 결과를 판정하세요. \
         제품 코드·테스트·기준을 수정하지 않습니다. 사용자는 없습니다.\n\n```markdown\n{}\n```\n\n\
         실행한 명령과 출력 요약을 적고, 응답의 마지막 줄에 정확히 `RESULT: PASS` 또는 `RESULT: FAIL`을 출력하세요.",
        gate.name, cwd, gate.body.trim()
    )
}

// ---------------------------------------------------------------- engine

#[derive(Debug)]
struct TaskOutcome {
    id: String,
    ok: bool,
    note: String,
}

fn control(state: &AppState, id: &str) -> Option<LoopControl> {
    state.loop_ctl.lock().get(id).copied()
}

/// Entry: build the run, preflight, then drive the scheduler until nothing is left.
pub async fn run_loop(state: &AppState, app: AppHandle, run_id: String) {
    let outcome = run_loop_inner(state, app.clone(), &run_id).await;
    let final_status = match &outcome {
        Ok(status) => status.clone(),
        Err(e) => {
            log(state, &app, &run_id, format!("Loop 실패: {e}"));
            "failed".to_string()
        }
    };
    let project = state.loops.lock().get(&run_id).map(|r| r.project.clone()).unwrap_or_default();
    let summary = write_report(state, &run_id, &final_status);
    with_run(state, &app, &run_id, |r| {
        r.status = final_status.clone();
        r.finished_at = Some(now_iso());
        r.summary = Some(summary.clone());
    });
    if let Some(r) = state.loops.lock().get(&run_id) {
        write_state_md(r);
    }
    state.loop_ctl.lock().remove(&run_id);
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(format!("Devil of Devin — Loop {final_status}"))
        .body(project)
        .show();
}

fn write_report(state: &AppState, run_id: &str, status: &str) -> String {
    let Some(run) = state.loops.lock().get(run_id).cloned() else { return String::new() };
    let done = run.tasks.values().filter(|t| t.status == "completed").count();
    let failed = run.tasks.values().filter(|t| t.status == "failed").count();
    let other = run.tasks.len() - done - failed;
    let mut s = String::new();
    s.push_str(&format!("# Loop 보고서 — run {}\n\n", run.id));
    s.push_str(&format!("- 상태: **{status}** · 시작 {} · 종료 {}\n", run.started_at, now_iso()));
    s.push_str(&format!("- 완료 {done} · 실패 {failed} · 미처리 {other} · 누적 토큰 {}\n\n", run.total_tokens));
    s.push_str("## 태스크별 결과\n\n");
    for t in run.tasks.values() {
        s.push_str(&format!("### {} — {} ({})\n\n", t.id, t.title, t.status));
        s.push_str(&format!("- 시도 {} · 브랜치 `{}`\n", t.attempts, t.branch.as_deref().unwrap_or("-")));
        if let Some(n) = &t.note {
            s.push_str(&format!("- 비고: {n}\n"));
        }
        for g in &t.gates {
            s.push_str(&format!(
                "- 게이트 {} (시도 {}): {} · `{}` · {}ms\n",
                g.gate,
                g.attempt,
                if g.pass { "PASS" } else { "FAIL" },
                g.command.lines().next().unwrap_or(""),
                g.duration_ms
            ));
        }
        if let Some(v) = &t.validation {
            s.push_str(&format!(
                "- 독립 검증 (시도 {}): {} → `{}`\n",
                v.attempt,
                if v.pass { "PASS" } else { "FAIL" },
                v.report_path
            ));
        }
        s.push('\n');
    }
    s.push_str("## 남은 미검증·미완료\n\n");
    let mut any = false;
    for t in run.tasks.values().filter(|t| t.status != "completed") {
        any = true;
        s.push_str(&format!("- {}: {} — {}\n", t.id, t.status, t.note.as_deref().unwrap_or("")));
    }
    if !any {
        s.push_str("- 없음\n");
    }
    let dir = run_dir(&run.project);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("report-{}.md", run.id)), &s);
    s
}

async fn run_loop_inner(state: &AppState, app: AppHandle, run_id: &str) -> Result<String, String> {
    let (project, resolved, cfg) = {
        let g = state.loops.lock();
        let r = g.get(run_id).ok_or("run not found")?;
        (r.project.clone(), r.resolved.clone(), r.config.clone())
    };
    let root = PathBuf::from(&project);

    // ---- preflight
    log(state, &app, run_id, format!("Loop 시작 (run {run_id})"));
    if resolved.is_git {
        exclude_devil_dir(&root);
    }
    let gates = load_gates(&project);
    let gate_names: Vec<String> = ["lint", "unit", "e2e", "task_validation"]
        .iter()
        .filter(|g| gates.contains_key(**g))
        .map(|g| g.to_string())
        .collect();
    if gates.is_empty() {
        log(state, &app, run_id, "경고: harness_setup/quality_gates/ 에 게이트 정의가 없습니다 — 구현 워커의 자체 검증과 독립 검증만 수행합니다");
    }
    for g in ["lint", "unit", "e2e"] {
        if let Some(d) = gates.get(g) {
            log(
                state,
                &app,
                run_id,
                match &d.command {
                    Some(c) => format!("게이트 {g}: `{}` ({})", c.lines().next().unwrap_or(""), d.shell),
                    None => format!("게이트 {g}: 실행 명령 없음 → 에이전트 판정"),
                },
            );
        }
    }
    let mut tasks = scan_tasks(&project);
    if tasks.is_empty() {
        return Err("plan_setup/ 에서 태스크 파일을 찾지 못했습니다 (plan-generator 단계가 필요)".into());
    }
    // crashed-run leftovers: in_progress → pending
    for t in tasks.iter_mut() {
        if t.status == "in_progress" {
            log(state, &app, run_id, format!("{}: 이전 실행의 in_progress → pending 으로 복구", t.id));
            let _ = set_task_status(&project, &t.rel_path, "pending");
            t.status = "pending".into();
        }
    }
    let selected: Vec<String> = tasks
        .iter()
        .filter(|t| cfg.phase.as_ref().map(|p| &t.phase == p).unwrap_or(true))
        .filter(|t| cfg.task_ids.as_ref().map(|ids| ids.contains(&t.id)).unwrap_or(true))
        .map(|t| t.id.clone())
        .collect();
    // restore attempt counts on resume
    let prev: Option<LoopRun> = if cfg.resume {
        std::fs::read_to_string(run_dir(&project).join("run-latest.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };
    with_run(state, &app, run_id, |r| {
        r.all_tasks = tasks.clone();
        r.gates = gate_names.clone();
        for t in tasks.iter().filter(|t| selected.contains(&t.id)) {
            let mut st = TaskRunState {
                id: t.id.clone(),
                title: t.title.clone(),
                status: t.status.clone(),
                ..Default::default()
            };
            if let Some(p) = prev.as_ref().and_then(|p| p.tasks.get(&t.id)) {
                st.attempts = p.attempts;
                st.strategy_switch = p.strategy_switch;
                st.gates = p.gates.clone();
                st.validation = p.validation.clone();
                st.attempt_notes = p.attempt_notes.clone();
                st.branch = p.branch.clone();
                st.worktree = p.worktree.clone();
                st.usage_tokens = p.usage_tokens;
            }
            r.tasks.insert(t.id.clone(), st);
        }
        r.status = "running".into();
    });
    if let Some(r) = state.loops.lock().get(run_id) {
        write_state_md(r);
    }
    log(
        state,
        &app,
        run_id,
        format!(
            "대상 태스크 {}개 (전체 {}개) · 동시 {} · 워크트리 {}",
            selected.len(),
            tasks.len(),
            resolved.concurrency,
            if resolved.use_worktrees { "사용" } else { "미사용" }
        ),
    );

    let deadline = resolved
        .budget_minutes
        .map(|m| std::time::Instant::now() + Duration::from_secs(m * 60));

    // ---- scheduler
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TaskOutcome>();
    let mut running: usize = 0;
    let mut final_status = "done".to_string();
    loop {
        let ctl = control(state, run_id);
        if ctl == Some(LoopControl::Abort) {
            final_status = "stopped".into();
            abort_workers(state, run_id).await;
            release_in_progress(state, &app, run_id);
            break;
        }
        let over_budget = deadline.map(|d| std::time::Instant::now() >= d).unwrap_or(false);
        let may_assign = ctl.is_none() && !over_budget;

        if may_assign {
            // completed set from the task files (the canonical status)
            let statuses: HashMap<String, String> = scan_tasks(&project)
                .into_iter()
                .map(|t| (t.id, t.status))
                .collect();
            let ready: Vec<TaskInfo> = {
                let g = state.loops.lock();
                let r = g.get(run_id).ok_or("run vanished")?;
                r.all_tasks
                    .iter()
                    .filter(|t| r.tasks.contains_key(&t.id))
                    .filter(|t| {
                        let st = &r.tasks[&t.id];
                        let file_status = statuses.get(&t.id).cloned().unwrap_or_default();
                        let retryable = file_status == "failed" && st.attempts < resolved.max_attempts * 2 && cfg.resume;
                        (file_status == "pending" || retryable)
                            && st.status != "in_progress"
                            && t.blocked_by.is_empty()
                            && t.depends_on.iter().all(|d| statuses.get(d).map(|s| s == "completed").unwrap_or(false))
                    })
                    .cloned()
                    .collect()
            };
            let cap = if resolved.use_worktrees && resolved.is_git { resolved.concurrency as usize } else { 1 };
            for t in ready.into_iter().take(cap.saturating_sub(running)) {
                running += 1;
                let _ = set_task_status(&project, &t.rel_path, "in_progress");
                with_run(state, &app, run_id, |r| {
                    if let Some(st) = r.tasks.get_mut(&t.id) {
                        st.status = "in_progress".into();
                        st.started_at = Some(now_iso());
                        st.finished_at = None;
                        st.note = None;
                    }
                });
                if let Some(r) = state.loops.lock().get(run_id) {
                    write_state_md(r);
                }
                log(state, &app, run_id, format!("{}: 배정 ({})", t.id, t.title));
                let app2 = app.clone();
                let tx2 = tx.clone();
                let rid = run_id.to_string();
                let gates2 = gates.clone();
                tauri::async_runtime::spawn(async move {
                    let st = app2.state::<AppState>();
                    let res = run_task(st.inner(), app2.clone(), &rid, t.clone(), &gates2).await;
                    let outcome = match res {
                        Ok(note) => TaskOutcome { id: t.id.clone(), ok: true, note },
                        Err(e) => TaskOutcome { id: t.id.clone(), ok: false, note: e },
                    };
                    let _ = tx2.send(outcome);
                });
            }
        }

        if running == 0 {
            if ctl == Some(LoopControl::Stop) {
                final_status = "stopped".into();
            } else if over_budget {
                final_status = "stopped".into();
                log(state, &app, run_id, "예산(시간) 소진 — 새 배정 없이 종료");
            }
            break;
        }

        // wait for a task to finish (or re-check controls every 2s)
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Some(outcome)) => {
                running -= 1;
                integrate(state, &app, run_id, &root, outcome, &gates).await;
                if let Some(r) = state.loops.lock().get(run_id) {
                    write_state_md(r);
                }
            }
            Ok(None) => break,
            Err(_) => {}
        }
    }
    Ok(final_status)
}

async fn abort_workers(state: &AppState, run_id: &str) {
    let agents: Vec<String> = state
        .loops
        .lock()
        .get(run_id)
        .map(|r| r.tasks.values().filter_map(|t| t.agent_id.clone()).collect())
        .unwrap_or_default();
    for a in agents {
        let removed = state.acp.lock().remove(&a);
        if let Some(ag) = removed {
            ag.kill().await;
        }
    }
}

/// Coordinator-side: merge the task branch, run post-integration gates, update files.
async fn integrate(
    state: &AppState,
    app: &AppHandle,
    run_id: &str,
    root: &Path,
    outcome: TaskOutcome,
    gates: &HashMap<String, GateDef>,
) {
    let (project, rel, worktree, branch, resolved) = {
        let g = state.loops.lock();
        let r = match g.get(run_id) {
            Some(r) => r,
            None => return,
        };
        let t = r.all_tasks.iter().find(|t| t.id == outcome.id);
        let st = r.tasks.get(&outcome.id);
        (
            r.project.clone(),
            t.map(|t| t.rel_path.clone()).unwrap_or_default(),
            st.and_then(|s| s.worktree.clone()),
            st.and_then(|s| s.branch.clone()),
            r.resolved.clone(),
        )
    };
    let mut final_ok = outcome.ok;
    let mut note = outcome.note.clone();

    if final_ok {
        if let (Some(wt), Some(br)) = (&worktree, &branch) {
            // make sure everything in the worktree is committed
            auto_commit(Path::new(wt), &format!("loop({}): auto-commit before integrate", outcome.id)).await;
            // the branch must not rewrite the contract it is judged by
            let protected = ["harness_setup/", "plan_setup/", "loop_setup/"];
            if let Ok(changed) = git(root, &["diff", "--name-only", &format!("HEAD...{br}")]).await {
                let bad: Vec<&str> = changed
                    .lines()
                    .filter(|f| protected.iter().any(|p| f.starts_with(p)))
                    .collect();
                if !bad.is_empty() {
                    let msg = format!("보호 경로 변경으로 통합 거부: {} — 브랜치 `{br}` 보존", bad.join(", "));
                    log(state, app, run_id, format!("{}: {msg}", outcome.id));
                    finish_task(state, app, run_id, &project, &rel, "failed", &msg).await;
                    return;
                }
            }
            log(state, app, run_id, format!("{}: 통합 — `{br}` 병합", outcome.id));
            match git(root, &["merge", "--no-ff", "--no-edit", "-m", &format!("loop({}): integrate", outcome.id), br]).await {
                Ok(_) => {
                    let _ = git(root, &["worktree", "remove", "--force", wt]).await;
                    // post-integration impact check: lint + unit in the main checkout
                    for g in ["lint", "unit"] {
                        if let Some(def) = gates.get(g) {
                            if def.command.is_some() {
                                let res = run_gate(state, def, root, resolved.gate_timeout_secs, 0).await;
                                let pass = res.pass;
                                with_run(state, app, run_id, |r| {
                                    if let Some(st) = r.tasks.get_mut(&outcome.id) {
                                        let mut res2 = res.clone();
                                        res2.gate = format!("post-merge:{g}");
                                        st.gates.push(res2);
                                    }
                                });
                                if !pass {
                                    final_ok = false;
                                    note = format!("통합 뒤 {g} 검사 실패 (병합은 유지됨, 수동 확인 필요)");
                                    log(state, app, run_id, format!("{}: {note}", outcome.id));
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = git(root, &["merge", "--abort"]).await;
                    final_ok = false;
                    note = format!("통합 충돌: {} — 브랜치 `{br}`와 워크트리 `{wt}`는 보존됨", tail(&e, 400));
                    log(state, app, run_id, format!("{}: {note}", outcome.id));
                }
            }
        }
    }

    let status = if final_ok { "completed" } else { "failed" };
    finish_task(state, app, run_id, &project, &rel, status, &note).await;
}

async fn finish_task(state: &AppState, app: &AppHandle, run_id: &str, project: &str, rel: &str, status: &str, note: &str) {
    let id = Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    if !rel.is_empty() {
        if let Err(e) = set_task_status(project, rel, status) {
            log(state, app, run_id, format!("{id}: 상태 기록 실패: {e}"));
        }
    }
    with_run(state, app, run_id, |r| {
        if let Some(st) = r.tasks.get_mut(&id) {
            st.status = status.into();
            st.finished_at = Some(now_iso());
            st.note = Some(note.to_string());
            st.agent_id = None;
        }
    });
    log(state, app, run_id, format!("{id}: {} — {note}", if status == "completed" { "완료" } else { "실패" }));
}

/// After stop/abort: tasks the scheduler left `in_progress` go back to `pending`
/// (attempt counts stay in the run record for resume).
fn release_in_progress(state: &AppState, app: &AppHandle, run_id: &str) {
    let items: Vec<(String, String)> = state
        .loops
        .lock()
        .get(run_id)
        .map(|r| {
            r.tasks
                .values()
                .filter(|t| t.status == "in_progress")
                .filter_map(|t| r.all_tasks.iter().find(|a| a.id == t.id).map(|a| (t.id.clone(), a.rel_path.clone())))
                .collect()
        })
        .unwrap_or_default();
    let project = state.loops.lock().get(run_id).map(|r| r.project.clone()).unwrap_or_default();
    for (id, rel) in items {
        let _ = set_task_status(&project, &rel, "pending");
        with_run(state, app, run_id, |r| {
            if let Some(st) = r.tasks.get_mut(&id) {
                st.status = "pending".into();
                st.note = Some(format!("중단됨 (시도 {}) — 재개 시 이어서 진행", st.attempts));
                st.agent_id = None;
            }
        });
        log(state, app, run_id, format!("{id}: 중단 → pending 으로 복구"));
    }
}

/// Worker-side: implement → gates → independent validation, with retries.
async fn run_task(
    state: &AppState,
    app: AppHandle,
    run_id: &str,
    task: TaskInfo,
    gates: &HashMap<String, GateDef>,
) -> Result<String, String> {
    let (run, resolved, cfg) = {
        let g = state.loops.lock();
        let r = g.get(run_id).ok_or("run vanished")?.clone();
        let res = r.resolved.clone();
        let cfg = r.config.clone();
        (r, res, cfg)
    };
    let project = run.project.clone();
    let root = PathBuf::from(&project);
    let task_text = std::fs::read_to_string(root.join(&task.rel_path)).unwrap_or_default();

    // ---- workspace
    let (cwd, branch) = if resolved.use_worktrees && resolved.is_git {
        let wt_dir = root.join(".devil").join("worktrees").join(&task.id);
        let br = format!("loop/{}", task.id);
        if !wt_dir.exists() {
            let _ = git(&root, &["worktree", "prune"]).await;
            let wt_s = wt_dir.to_string_lossy().to_string();
            // -B: (re)create the branch at HEAD if it exists from an earlier run
            git(&root, &["worktree", "add", "-B", &br, &wt_s]).await.map_err(|e| format!("워크트리 생성 실패: {e}"))?;
        }
        (wt_dir.to_string_lossy().to_string(), Some(br))
    } else {
        (project.clone(), None)
    };
    let prev_attempts = run.tasks.get(&task.id).map(|t| t.attempts).unwrap_or(0);
    with_run(state, &app, run_id, |r| {
        if let Some(st) = r.tasks.get_mut(&task.id) {
            st.worktree = if cwd != project { Some(cwd.clone()) } else { None };
            st.branch = branch.clone();
        }
    });
    let _ = std::fs::create_dir_all(root.join(".devil").join("loop").join(&task.id));

    let impl_timeout = Duration::from_secs(resolved.gate_timeout_secs.max(600) * 4);
    let mut prev_failure = String::new();
    let max_total = resolved.max_attempts * 2; // 3 regular + 3 after strategy switch
    let mut attempt = prev_attempts;
    while attempt < max_total {
        attempt += 1;
        if control(state, run_id) == Some(LoopControl::Abort) {
            return Err("중단됨".into());
        }
        with_run(state, &app, run_id, |r| {
            if let Some(st) = r.tasks.get_mut(&task.id) {
                st.attempts = attempt;
                st.strategy_switch = attempt > resolved.max_attempts;
            }
        });
        log(state, &app, run_id, format!("{}: 구현 시도 {attempt}/{max_total}", task.id));

        // ---- implement
        let prompt = implementer_prompt(&run, &task, &task_text, &cwd, branch.as_deref(), attempt, &prev_failure);
        let label = format!("loop:{}:impl-{attempt}", task.id);
        let impl_res = run_oneshot(
            state,
            app.clone(),
            OneshotOpts {
                label: &label,
                cwd: &cwd,
                model: &resolved.implementer_model,
                prompt: &prompt,
                mode: Some(&resolved.mode),
                agent_type: None,
                auto_approve: true,
                timeout: Some(impl_timeout),
            },
        )
        .await;
        let impl_text = match impl_res {
            Ok(r) => {
                with_run(state, &app, run_id, |rr| {
                    rr.total_tokens += r.usage.total_tokens;
                    if let Some(st) = rr.tasks.get_mut(&task.id) {
                        st.usage_tokens += r.usage.total_tokens;
                        st.attempt_notes.push(format!("시도 {attempt}: 구현 종료 ({}), {} tokens", r.stop_reason, r.usage.total_tokens));
                    }
                });
                r.text
            }
            Err(e) => {
                log(state, &app, run_id, format!("{}: 구현 워커 오류: {e}", task.id));
                prev_failure = format!("구현 워커가 비정상 종료: {e}");
                continue;
            }
        };
        let _ = std::fs::write(
            root.join(".devil").join("loop").join(&task.id).join(format!("implementer-{attempt}.md")),
            &impl_text,
        );
        // auto-commit so the validator and the merge see the same tree
        if branch.is_some() {
            auto_commit(Path::new(&cwd), &format!("loop({}): attempt {attempt} (auto-commit)", task.id)).await;
        }

        // ---- gates (lint → unit → e2e), stop at the first failure
        let mut gate_failed: Option<GateResult> = None;
        for g in ["lint", "unit", "e2e"] {
            if !task.verification.is_empty() && !task.verification.iter().any(|v| v == g) {
                continue;
            }
            let Some(def) = gates.get(g) else { continue };
            let res = if def.command.is_some() {
                run_gate(state, def, Path::new(&cwd), resolved.gate_timeout_secs, attempt).await
            } else {
                run_gate_by_agent(state, app.clone(), def, &cwd, &resolved, attempt).await
            };
            let pass = res.pass;
            log(
                state,
                &app,
                run_id,
                format!("{}: 게이트 {g} {} ({}ms)", task.id, if pass { "PASS" } else { "FAIL" }, res.duration_ms),
            );
            with_run(state, &app, run_id, |r| {
                if let Some(st) = r.tasks.get_mut(&task.id) {
                    st.gates.push(res.clone());
                }
            });
            if !pass {
                gate_failed = Some(res);
                break;
            }
        }
        if let Some(f) = gate_failed {
            prev_failure = format!(
                "게이트 `{}` 실패 (exit {:?})\n명령:\n```\n{}\n```\n출력(끝부분):\n```\n{}\n```",
                f.gate, f.exit_code, f.command, f.output_tail
            );
            continue;
        }

        // ---- independent validation
        let wants_validation =
            task.verification.is_empty() || task.verification.iter().any(|v| v == "task_validation");
        if cfg.skip_validation || !wants_validation {
            log(state, &app, run_id, format!("{}: 독립 검증 생략 (설정/verification)", task.id));
            return Ok(format!("게이트 통과 (독립 검증 생략), 시도 {attempt}"));
        }
        log(state, &app, run_id, format!("{}: 독립 검증 세션 시작", task.id));
        let vprompt = validator_prompt(&run, &task, &cwd, attempt);
        let vlabel = format!("loop:{}:validate-{attempt}", task.id);
        let vres = run_oneshot(
            state,
            app.clone(),
            OneshotOpts {
                label: &vlabel,
                cwd: &cwd,
                model: &resolved.validator_model,
                prompt: &vprompt,
                mode: Some("smart"),
                agent_type: Some("review"),
                auto_approve: true,
                timeout: Some(impl_timeout),
            },
        )
        .await;
        let (vpass, vtext) = match vres {
            Ok(r) => {
                with_run(state, &app, run_id, |rr| {
                    rr.total_tokens += r.usage.total_tokens;
                    if let Some(st) = rr.tasks.get_mut(&task.id) {
                        st.usage_tokens += r.usage.total_tokens;
                    }
                });
                (parse_result_line(&r.text), r.text)
            }
            Err(e) => (None, format!("검증 워커 오류: {e}")),
        };
        let rel_report = format!(".devil/loop/{}/validation-{attempt}.md", task.id);
        let _ = std::fs::write(root.join(&rel_report), &vtext);
        let pass = vpass.unwrap_or(false);
        let summary = tail(&vtext, 600);
        with_run(state, &app, run_id, |r| {
            if let Some(st) = r.tasks.get_mut(&task.id) {
                st.validation = Some(ValidationResult {
                    pass,
                    report_path: rel_report.clone(),
                    summary: summary.clone(),
                    attempt,
                    at: now_iso(),
                });
            }
        });
        log(
            state,
            &app,
            run_id,
            format!(
                "{}: 독립 검증 {}",
                task.id,
                match vpass {
                    Some(true) => "PASS",
                    Some(false) => "FAIL",
                    None => "판정 불가 (RESULT 줄 없음 → FAIL 처리)",
                }
            ),
        );
        if pass {
            return Ok(format!("게이트·독립 검증 PASS, 시도 {attempt}"));
        }
        prev_failure = format!("독립 검증 FAIL. 검증 보고 (`{rel_report}`):\n\n{}", tail(&vtext, 3000));
    }
    Err(format!("{}회 시도 후에도 통과하지 못함 — 마지막 실패: {}", attempt, tail(&prev_failure, 300).lines().next().unwrap_or("")))
}

async fn run_gate(state: &AppState, def: &GateDef, cwd: &Path, timeout_secs: u64, attempt: u32) -> GateResult {
    let cmd = def.command.clone().unwrap_or_default();
    let dir = match &def.cwd {
        Some(c) => cwd.join(c),
        None => cwd.to_path_buf(),
    };
    // heavy gates (e2e/builds) are serialized app-wide
    let _permit = if def.name == "e2e" { state.heavy.acquire().await.ok() } else { None };
    let start = std::time::Instant::now();
    let (code, out, timed_out) = run_shell(&def.shell, &cmd, &dir, Duration::from_secs(timeout_secs)).await;
    let pass = code == Some(0) && !timed_out;
    GateResult {
        gate: def.name.clone(),
        pass,
        command: cmd,
        exit_code: code,
        output_tail: tail(&out, 4000),
        duration_ms: start.elapsed().as_millis() as u64,
        at: now_iso(),
        attempt,
        agent_judged: false,
    }
}

async fn run_gate_by_agent(
    state: &AppState,
    app: AppHandle,
    def: &GateDef,
    cwd: &str,
    resolved: &Resolved,
    attempt: u32,
) -> GateResult {
    let start = std::time::Instant::now();
    let label = format!("loop:gate-{}", def.name);
    let r = run_oneshot(
        state,
        app,
        OneshotOpts {
            label: &label,
            cwd,
            model: &resolved.validator_model,
            prompt: &gate_agent_prompt(def, cwd),
            mode: Some("smart"),
            agent_type: Some("review"),
            auto_approve: true,
            timeout: Some(Duration::from_secs(resolved.gate_timeout_secs)),
        },
    )
    .await;
    let (pass, out) = match r {
        Ok(r) => (parse_result_line(&r.text).unwrap_or(false), r.text),
        Err(e) => (false, format!("게이트 에이전트 오류: {e}")),
    };
    GateResult {
        gate: def.name.clone(),
        pass,
        command: format!("(에이전트 판정) {}", def.rel_path),
        exit_code: None,
        output_tail: tail(&out, 4000),
        duration_ms: start.elapsed().as_millis() as u64,
        at: now_iso(),
        attempt,
        agent_judged: true,
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn loop_start(
    state: State<'_, AppState>,
    app: AppHandle,
    config: LoopConfig,
) -> Result<LoopRun, String> {
    let project = config.project.clone();
    if !Path::new(&project).is_dir() {
        return Err(format!("프로젝트 폴더가 없습니다: {project}"));
    }
    if state
        .loops
        .lock()
        .values()
        .any(|r| r.project == project && matches!(r.status.as_str(), "running" | "preflight" | "stopping"))
    {
        return Err("이 프로젝트에서 이미 Loop가 실행 중입니다".into());
    }
    let s = state.settings.lock().clone();
    let is_git = is_git_repo(Path::new(&project)).await;
    let pick = |a: &Option<String>, b: &str, c: &str| {
        a.clone()
            .filter(|x| !x.trim().is_empty())
            .unwrap_or_else(|| if b.trim().is_empty() { c.to_string() } else { b.to_string() })
    };
    let resolved = Resolved {
        concurrency: config.concurrency.unwrap_or(s.loop_concurrency).max(1),
        use_worktrees: config.use_worktrees.unwrap_or(s.loop_use_worktrees) && is_git,
        max_attempts: config.max_attempts.unwrap_or(s.loop_max_attempts).max(1),
        implementer_model: pick(&config.implementer_model, &s.loop_implementer_model, &s.default_model),
        validator_model: pick(&config.validator_model, &s.loop_validator_model, &s.default_model),
        mode: pick(&config.mode, &s.loop_mode, "smart"),
        gate_timeout_secs: s.loop_gate_timeout_secs.max(60),
        budget_minutes: config.budget_minutes,
        is_git,
    };
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let run = LoopRun {
        id: id.clone(),
        project: project.clone(),
        config,
        resolved,
        status: "preflight".into(),
        started_at: now_iso(),
        finished_at: None,
        tasks: BTreeMap::new(),
        all_tasks: vec![],
        gates: vec![],
        log: vec![],
        summary: None,
        total_tokens: 0,
    };
    state.loops.lock().insert(id.clone(), run.clone());
    emit_run(&app, &run);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let st = app2.state::<AppState>();
        run_loop(st.inner(), app2.clone(), id).await;
    });
    Ok(run)
}

#[tauri::command]
pub fn loop_status(state: State<'_, AppState>, id: String) -> Option<LoopRun> {
    state.loops.lock().get(&id).cloned()
}

#[tauri::command]
pub fn loop_list(state: State<'_, AppState>) -> Vec<LoopRun> {
    state.loops.lock().values().cloned().collect()
}

/// `kind`: "stop" (finish running tasks) | "abort" (kill workers now)
#[tauri::command]
pub fn loop_stop(state: State<'_, AppState>, app: AppHandle, id: String, kind: Option<String>) -> Result<(), String> {
    let ctl = if kind.as_deref() == Some("abort") { LoopControl::Abort } else { LoopControl::Stop };
    state.loop_ctl.lock().insert(id.clone(), ctl);
    with_run(&state, &app, &id, |r| {
        if matches!(r.status.as_str(), "running" | "preflight") {
            r.status = "stopping".into();
        }
    });
    Ok(())
}

/// Tasks + gate definitions for the board (works without a running loop).
#[tauri::command]
pub fn loop_board(path: String) -> Value {
    let tasks = scan_tasks(&path);
    let gates = load_gates(&path);
    let mut gate_list: Vec<Value> = ["lint", "unit", "e2e", "task_validation"]
        .iter()
        .filter_map(|g| gates.get(*g))
        .map(|g| {
            json!({
                "name": g.name, "path": g.rel_path, "command": g.command, "shell": g.shell,
                "agentJudged": g.command.is_none()
            })
        })
        .collect();
    gate_list.sort_by_key(|g| g["name"].as_str().unwrap_or("").to_string());
    let latest: Option<Value> = std::fs::read_to_string(run_dir(&path).join("run-latest.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    json!({ "tasks": tasks, "gates": gate_list, "latestRun": latest })
}

/// Manually set a task's status (e.g. unblock, reset failed → pending).
#[tauri::command]
pub fn loop_task_set_status(path: String, rel: String, status: String) -> Result<(), String> {
    if !["pending", "in_progress", "blocked", "failed", "completed"].contains(&status.as_str()) {
        return Err("잘못된 상태".into());
    }
    set_task_status(&path, &rel, &status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_split_and_status_rewrite() {
        let text = "---\nid: phase-01-task-01\nstatus: pending\ndepends_on: []\nacceptance_criteria:\n  - a\nverification: [lint, unit]\n---\n# Title\n";
        let (fm, body) = split_frontmatter(text).unwrap();
        assert!(fm.contains("id: phase-01-task-01"));
        assert!(body.contains("# Title"));
        let dir = std::env::temp_dir().join(format!("dod-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("plan_setup")).unwrap();
        std::fs::write(dir.join("plan_setup/t.md"), text).unwrap();
        set_task_status(&dir.to_string_lossy(), "plan_setup/t.md", "completed").unwrap();
        let after = std::fs::read_to_string(dir.join("plan_setup/t.md")).unwrap();
        assert!(after.contains("status: completed"));
        assert!(after.contains("verification: [lint, unit]"));
        let t = parse_task(&dir, &dir.join("plan_setup/t.md")).unwrap();
        assert_eq!(t.verification, vec!["lint", "unit"]);
        assert_eq!(t.title, "Title");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn gate_block_extraction() {
        let dir = std::env::temp_dir().join(format!("dod-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("unit.md");
        std::fs::write(&p, "# unit\n\n설명\n\n```text\nnot a command\n```\n\n```bash\n# comment\n$ npm test -- --run\n```\n").unwrap();
        let g = parse_gate(&dir, &p).unwrap();
        assert_eq!(g.name, "unit");
        assert_eq!(g.command.as_deref(), Some("npm test -- --run"));
        assert_eq!(g.shell, "bash");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn result_line() {
        assert_eq!(parse_result_line("...\n**RESULT: PASS**\n"), Some(true));
        assert_eq!(parse_result_line("RESULT: FAIL\n"), Some(false));
        assert_eq!(parse_result_line("nothing"), None);
    }
}
