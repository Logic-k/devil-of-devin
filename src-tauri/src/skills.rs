use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSkill {
    pub name: String,
    pub description: String,
    pub dir: String,
    pub source: String, // which root it was found under
    pub enabled: bool,
}

fn skill_roots(extra: &[String]) -> Vec<(PathBuf, &'static str)> {
    let mut roots: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(appdata) = dirs::config_dir() {
        roots.push((appdata.join(r"devin\skills"), "devin-global"));
        roots.push((appdata.join(r"cognition\skills"), "cognition-global"));
        roots.push((appdata.join(r"devin\cli\plugins\cache"), "plugin-cache"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push((home.join(r".agents\skills"), "agents-global"));
        roots.push((home.join(r".claude\skills"), "claude-global"));
    }
    for e in extra {
        roots.push((PathBuf::from(e), "custom"));
    }
    roots
}

fn parse_frontmatter(text: &str) -> (String, String) {
    let mut name = String::new();
    let mut desc = String::new();
    if let Some(fm) = text.strip_prefix("---") {
        if let Some(end) = fm.find("\n---") {
            let block = &fm[..end];
            let mut cur_key = "";
            for line in block.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    match k {
                        "name" => { name = v.to_string(); cur_key = "name"; }
                        "description" => { desc = v.to_string(); cur_key = "description"; }
                        _ => cur_key = "",
                    }
                } else if cur_key == "description" && !line.trim().is_empty() {
                    if !desc.is_empty() { desc.push(' '); }
                    desc.push_str(line.trim());
                }
            }
        }
    }
    (name, desc)
}

fn collect_skills_in(root: &Path, source: &str, depth: u8, out: &mut Vec<LocalSkill>) {
    if depth > 5 || !root.is_dir() {
        return;
    }
    let skill_md = root.join("SKILL.md");
    if skill_md.exists() {
        let disabled = root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .ends_with(".disabled");
        let (name, desc) = std::fs::read_to_string(&skill_md)
            .map(|t| parse_frontmatter(&t))
            .unwrap_or_default();
        out.push(LocalSkill {
            name: if name.is_empty() {
                root.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .trim_end_matches(".disabled")
                    .to_string()
            } else { name },
            description: desc,
            dir: root.display().to_string(),
            source: source.to_string(),
            enabled: !disabled,
        });
        return;
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                collect_skills_in(&e.path(), source, depth + 1, out);
            }
        }
    }
}

#[tauri::command]
pub fn skills_scan_local(state: State<'_, AppState>) -> Vec<LocalSkill> {
    let extra = state.settings.lock().skills_dirs.clone();
    let mut out = Vec::new();
    for (root, src) in skill_roots(&extra) {
        if root.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&root) {
                for e in rd.flatten() {
                    if e.path().is_dir() {
                        collect_skills_in(&e.path(), src, 1, &mut out);
                    }
                }
            }
            // root itself may be a skill dir (e.g. a custom dir pointing at a skill)
            if root.join("SKILL.md").exists() {
                collect_skills_in(&root, src, 1, &mut out);
            }
        }
    }
    out
}

/// Toggle a skill: Devin loads any SKILL.md under the scan dirs; disabling
/// renames the directory to `<name>.disabled` (re-enable strips it back).
#[tauri::command]
pub fn skills_set_enabled(dir: String, enabled: bool) -> Result<String, String> {
    let p = PathBuf::from(&dir);
    let parent = p.parent().unwrap_or(Path::new("."));
    let fname = p.file_name().unwrap_or_default().to_string_lossy();
    if enabled {
        if !fname.ends_with(".disabled") {
            return Ok(dir);
        }
        let candidate = parent.join(fname.trim_end_matches(".disabled"));
        std::fs::rename(&p, &candidate).map_err(|e| e.to_string())?;
        Ok(candidate.display().to_string())
    } else {
        if fname.ends_with(".disabled") {
            return Ok(dir);
        }
        let new = parent.join(format!("{fname}.disabled"));
        std::fs::rename(&p, &new).map_err(|e| e.to_string())?;
        Ok(new.display().to_string())
    }
}

// ---------- GitHub ----------

#[tauri::command]
pub async fn skills_github_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Value, String> {
    let token = state.settings.lock().github_token.clone();
    let mut req = state
        .http
        .get("https://api.github.com/search/code")
        .query(&[
            ("q", format!("{query} filename:SKILL.md")),
            ("per_page", "20".into()),
        ])
        .header("Accept", "application/vnd.github+json");
    if let Some(t) = &token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        // fall back to repo search (no token needed)
        let resp2 = state
            .http
            .get("https://api.github.com/search/repositories")
            .query(&[("q", format!("{query} skills")), ("per_page", "20".into())])
            .header("Accept", "application/vnd.github+json");
        let r2 = if let Some(t) = &token { resp2.bearer_auth(t) } else { resp2 };
        let body2: Value = r2.send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        return Ok(json!({ "kind": "repos", "fallback": true, "body": body2 }));
    }
    Ok(json!({ "kind": "code", "body": body }))
}

/// Install a skill from GitHub: `git clone --depth 1` then copy the skill dir.
/// `src` = "owner/repo" or "owner/repo#path/to/skill" or a github URL.
#[tauri::command]
pub async fn skills_install(
    _state: State<'_, AppState>,
    src: String,
    dest_root: String,
) -> Result<String, String> {
    let (repo, subpath) = if let Some((r, p)) = src.split_once('#') {
        (r.to_string(), p.trim_matches('/').to_string())
    } else {
        (src.trim_end_matches('/').to_string(), String::new())
    };
    let repo = repo
        .trim_start_matches("https://github.com/")
        .trim_start_matches("github.com/")
        .trim_end_matches(".git")
        .to_string();

    let tmp = std::env::temp_dir().join(format!("dod-skill-{}", uuid::Uuid::new_v4()));
    let url = format!("https://github.com/{repo}.git");
    let out = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", &url, tmp.to_str().unwrap()])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    // find the skill dir: requested subpath, or first dir containing SKILL.md
    let mut candidates: Vec<PathBuf> = Vec::new();
    let base = if subpath.is_empty() { tmp.clone() } else { tmp.join(&subpath) };
    collect_skills_in(&base, "gh", 1, &mut Vec::new());
    if base.join("SKILL.md").exists() {
        candidates.push(base.clone());
    } else if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten().filter(|e| e.path().is_dir()) {
            if e.path().join("SKILL.md").exists() {
                candidates.push(e.path());
            }
        }
    }
    let src_dir = candidates
        .first()
        .cloned()
        .ok_or("no SKILL.md found in repo/path")?;
    let name = src_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
    // empty dest_root → devin global skills dir (auto-loaded by the CLI)
    let dest_root = if dest_root.trim().is_empty() {
        dirs::config_dir()
            .ok_or("no config dir")?
            .join(r"devin\skills")
    } else {
        PathBuf::from(&dest_root)
    };
    let dest = dest_root.join(&name);
    copy_dir(&src_dir, &dest).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(dest.display().to_string())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}
