//! Install / inspect the `harness_loop_full_korean` skill pack for Devin.
//!
//! The pack is 13 host-agnostic SKILL.md directories. Devin loads user skills
//! from `~/.agents/skills/<name>/SKILL.md` and exposes them as `/name` slash
//! commands in every ACP session (verified: `available_commands_update`).

use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;

pub const HARNESS_REPO: &str = "https://github.com/GangJaeYu/harness_loop_full_korean.git";
pub const HARNESS_SKILLS: &[&str] = &[
    "idea-brainstorm",
    "prd-generator",
    "trd-generator",
    "architecture-generator",
    "plan-generator",
    "harness-setup",
    "harness-update",
    "loop-setup",
    "loop-update",
    "code-to-docs",
    "feature-add",
    "bug-fix",
    "refactor-plan",
];
const MARKER: &str = ".harness-loop-fullstack-kor.json";

pub fn user_skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
        .join("skills")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessStatus {
    pub skills_dir: String,
    pub installed: Vec<String>,
    pub missing: Vec<String>,
    pub version: Option<String>,
    pub commit: Option<String>,
    pub installed_at: Option<String>,
    pub complete: bool,
}

pub fn status() -> HarnessStatus {
    let dir = user_skills_dir();
    let mut installed = Vec::new();
    let mut missing = Vec::new();
    for s in HARNESS_SKILLS {
        if dir.join(s).join("SKILL.md").is_file() {
            installed.push(s.to_string());
        } else {
            missing.push(s.to_string());
        }
    }
    let marker: Option<Value> = std::fs::read_to_string(dir.join(MARKER))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let g = |k: &str| marker.as_ref()?.get(k)?.as_str().map(|s| s.to_string());
    HarnessStatus {
        skills_dir: dir.display().to_string(),
        complete: missing.is_empty(),
        installed,
        missing,
        version: g("version"),
        commit: g("commit"),
        installed_at: g("installedAt"),
    }
}

#[tauri::command]
pub fn harness_status() -> HarnessStatus {
    status()
}

/// Clone the pack (optionally at a tag/branch) and copy its skills into the
/// user skill dir. Existing skill dirs of the same name are replaced.
#[tauri::command]
pub async fn harness_install(
    state: State<'_, AppState>,
    git_ref: Option<String>,
) -> Result<HarnessStatus, String> {
    let git_ref = git_ref
        .filter(|r| !r.trim().is_empty())
        .or_else(|| {
            let r = state.settings.lock().harness_ref.clone();
            (!r.trim().is_empty()).then_some(r)
        });
    let tmp = std::env::temp_dir().join(format!("dod-harness-{}", uuid::Uuid::new_v4()));
    let mut args = vec!["clone".to_string(), "--depth".into(), "1".into()];
    if let Some(r) = &git_ref {
        args.push("--branch".into());
        args.push(r.clone());
    }
    args.push(HARNESS_REPO.into());
    args.push(tmp.to_string_lossy().to_string());
    let out = tokio::process::Command::new("git")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("git 실행 실패: {e}"))?;
    if !out.status.success() {
        return Err(format!("git clone 실패: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let commit = tokio::process::Command::new("git")
        .args(["-C", &tmp.to_string_lossy(), "rev-parse", "--short", "HEAD"])
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let version = std::fs::read_to_string(tmp.join("plugin.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("version")?.as_str().map(|s| s.to_string()));

    let dest_root = user_skills_dir();
    std::fs::create_dir_all(&dest_root).map_err(|e| e.to_string())?;
    let mut copied = 0;
    for s in HARNESS_SKILLS {
        let src = tmp.join("skills").join(s);
        if !src.join("SKILL.md").is_file() {
            continue;
        }
        let dest = dest_root.join(s);
        let _ = std::fs::remove_dir_all(&dest);
        copy_dir(&src, &dest).map_err(|e| e.to_string())?;
        copied += 1;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if copied == 0 {
        return Err("리포지토리에서 SKILL.md를 찾지 못했습니다".into());
    }
    let marker = json!({
        "repo": HARNESS_REPO, "ref": git_ref, "version": version, "commit": commit,
        "installedAt": crate::workers::now_iso(), "skills": copied
    });
    let _ = std::fs::write(
        dest_root.join(MARKER),
        serde_json::to_string_pretty(&marker).unwrap_or_default(),
    );
    Ok(status())
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
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
