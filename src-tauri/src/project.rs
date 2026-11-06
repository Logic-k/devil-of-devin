use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, Project};

/// The seven harness stages. `artifacts` are checked relative to the project
/// root; a directory artifact counts as present when it contains any .md file.
pub const STAGES: &[(&str, &str, &str, &[&str])] = &[
    ("idea", "아이디어", "idea-brainstorm", &["docs/IDEA.md"]),
    ("prd", "요구(PRD)", "prd-generator", &["docs/PRD.md"]),
    ("trd", "기술(TRD)", "trd-generator", &["docs/TRD.md"]),
    ("architecture", "설계", "architecture-generator", &["docs/ARCHITECTURE/"]),
    ("plan", "계획", "plan-generator", &["plan_setup/PLAN.md", "plan_setup/STATE.md"]),
    ("harness", "하네스", "harness-setup", &["harness_setup/HARNESS.md", "harness_setup/quality_gates/"]),
    ("loop", "루프", "loop-setup", &["loop_setup/LOOP.md"]),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: String,
    pub exists: bool,
    pub modified: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub id: String,
    pub name: String,
    pub skill: String,
    pub artifacts: Vec<Artifact>,
    /// missing | partial | done
    pub status: String,
}

fn mtime(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = m.into();
    Some(dt.format("%Y-%m-%d %H:%M").to_string())
}

fn dir_has_md(p: &Path) -> bool {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten().any(|e| {
                let path = e.path();
                path.extension().map(|x| x == "md").unwrap_or(false) || (path.is_dir() && dir_has_md(&path))
            })
        })
        .unwrap_or(false)
}

pub fn stages_for(root: &Path) -> Vec<Stage> {
    STAGES
        .iter()
        .map(|(id, name, skill, arts)| {
            let artifacts: Vec<Artifact> = arts
                .iter()
                .map(|a| {
                    let p = root.join(a.trim_end_matches('/'));
                    let is_dir = a.ends_with('/');
                    let exists = if is_dir { p.is_dir() && dir_has_md(&p) } else { p.is_file() };
                    Artifact { path: a.to_string(), exists, modified: if exists { mtime(&p) } else { None }, is_dir }
                })
                .collect();
            let n = artifacts.iter().filter(|a| a.exists).count();
            let status = if n == 0 {
                "missing"
            } else if n == artifacts.len() {
                "done"
            } else {
                "partial"
            };
            Stage {
                id: id.to_string(),
                name: name.to_string(),
                skill: skill.to_string(),
                artifacts,
                status: status.into(),
            }
        })
        .collect()
}

fn emit_projects(app: &AppHandle, state: &AppState) {
    let s = state.settings.lock();
    let _ = app.emit(
        "projects://changed",
        json!({ "projects": s.projects, "active": s.active_project }),
    );
}

#[tauri::command]
pub fn project_list(state: State<'_, AppState>) -> Value {
    let s = state.settings.lock();
    json!({ "projects": s.projects, "active": s.active_project })
}

#[tauri::command]
pub fn project_add(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> Result<Project, String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("폴더가 없습니다: {path}"));
    }
    let canon = p.display().to_string();
    let mut s = state.settings.lock();
    if let Some(existing) = s.projects.iter().find(|x| x.path.eq_ignore_ascii_case(&canon)) {
        let e = existing.clone();
        s.active_project = Some(e.id.clone());
        s.save();
        drop(s);
        emit_projects(&app, &state);
        return Ok(e);
    }
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        p.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| canon.clone())
    });
    let proj = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        path: canon,
        created_at: crate::workers::now_iso(),
    };
    s.projects.push(proj.clone());
    s.active_project = Some(proj.id.clone());
    s.save();
    drop(s);
    emit_projects(&app, &state);
    Ok(proj)
}

#[tauri::command]
pub fn project_remove(state: State<'_, AppState>, app: AppHandle, id: String) {
    {
        let mut s = state.settings.lock();
        s.projects.retain(|p| p.id != id);
        if s.active_project.as_deref() == Some(&id) {
            s.active_project = s.projects.first().map(|p| p.id.clone());
        }
        s.save();
    }
    emit_projects(&app, &state);
}

#[tauri::command]
pub fn project_set_active(state: State<'_, AppState>, app: AppHandle, id: String) {
    {
        let mut s = state.settings.lock();
        if s.projects.iter().any(|p| p.id == id) {
            s.active_project = Some(id);
            s.save();
        }
    }
    emit_projects(&app, &state);
}

#[tauri::command]
pub fn project_stages(path: String) -> Vec<Stage> {
    stages_for(Path::new(&path))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
    pub modified: Option<String>,
    pub children: Vec<DocEntry>,
}

const DOC_ROOTS: &[&str] = &["docs", "plan_setup", "harness_setup", "loop_setup", "research", ".devil/loop"];
const ROOT_FILES: &[&str] = &["README.md", "AGENTS.md", "CLAUDE.md"];

fn walk_docs(root: &Path, rel: &str, depth: u8) -> Vec<DocEntry> {
    let mut out = Vec::new();
    let dir = root.join(rel);
    let Ok(rd) = std::fs::read_dir(&dir) else { return out };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && rel != ".devil" {
            continue;
        }
        let p = e.path();
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if p.is_dir() {
            if depth >= 5 || name == "node_modules" || name == "worktrees" {
                continue;
            }
            let children = walk_docs(root, &child_rel, depth + 1);
            if !children.is_empty() {
                out.push(DocEntry { rel: child_rel, name, is_dir: true, modified: None, children });
            }
        } else if name.ends_with(".md") || name.ends_with(".json") || name.ends_with(".txt") {
            out.push(DocEntry { rel: child_rel, name, is_dir: false, modified: mtime(&p), children: vec![] });
        }
    }
    out
}

/// Markdown/document tree for the Docs view.
#[tauri::command]
pub fn project_docs(path: String) -> Vec<DocEntry> {
    let root = Path::new(&path);
    let mut out = Vec::new();
    for f in ROOT_FILES {
        let p = root.join(f);
        if p.is_file() {
            out.push(DocEntry { rel: f.to_string(), name: f.to_string(), is_dir: false, modified: mtime(&p), children: vec![] });
        }
    }
    for r in DOC_ROOTS {
        if root.join(r).is_dir() {
            let children = walk_docs(root, r, 1);
            if !children.is_empty() {
                out.push(DocEntry { rel: r.to_string(), name: r.to_string(), is_dir: true, modified: None, children });
            }
        }
    }
    out
}

fn safe_join(root: &str, rel: &str) -> Result<PathBuf, String> {
    let r = PathBuf::from(root);
    let p = r.join(rel);
    let canon_root = r.canonicalize().map_err(|e| e.to_string())?;
    let canon = p.canonicalize().map_err(|e| format!("{rel}: {e}"))?;
    if !canon.starts_with(&canon_root) {
        return Err("프로젝트 밖 경로입니다".into());
    }
    Ok(canon)
}

#[tauri::command]
pub fn project_file_read(path: String, rel: String) -> Result<String, String> {
    let p = safe_join(&path, &rel)?;
    let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.len() > 2_000_000 {
        return Err("파일이 너무 큽니다 (2MB 초과)".into());
    }
    std::fs::read_to_string(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn project_file_write(path: String, rel: String, content: String) -> Result<(), String> {
    // allow creating new files under the project (canonicalize needs existence → check parent)
    let r = PathBuf::from(&path).canonicalize().map_err(|e| e.to_string())?;
    let p = r.join(&rel);
    let parent = p.parent().ok_or("bad path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let cp = parent.canonicalize().map_err(|e| e.to_string())?;
    if !cp.starts_with(&r) {
        return Err("프로젝트 밖 경로입니다".into());
    }
    std::fs::write(&p, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn project_git_info(path: String) -> Value {
    let root = Path::new(&path);
    let is_git = root.join(".git").exists();
    let branch = if is_git {
        std::process::Command::new("git")
            .args(["-C", &path, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        None
    };
    let dirty = if is_git {
        std::process::Command::new("git")
            .args(["-C", &path, "status", "--porcelain"])
            .output()
            .ok()
            .map(|o| !o.stdout.is_empty())
    } else {
        None
    };
    json!({ "isGit": is_git, "branch": branch, "dirty": dirty })
}
