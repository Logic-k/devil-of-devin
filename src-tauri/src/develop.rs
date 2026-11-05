use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::workers::run_oneshot_worker;

/// One-shot "개발 모드" pipeline: research md → plan → implement → review → verify.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopSpec {
    pub target_dir: String,
    /// free-form goal; when empty the pipeline derives it from research/INDEX.md
    pub goal: String,
    /// implement→review→fix cycles
    pub max_cycles: u32,
    /// verification command (build/test) run by the verifier worker
    pub verify_command: Option<String>,
    pub model: Option<String>,
    pub auto_approve: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopRun {
    pub id: String,
    pub spec: DevelopSpec,
    pub phase: String,
    pub progress: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
}

fn note(state: &AppState, app: &AppHandle, id: &str, msg: impl Into<String>) {
    let m = msg.into();
    let mut g = state.develop.lock();
    if let Some(r) = g.get_mut(id) {
        r.progress.push(m.clone());
        let _ = app.emit(
            &format!("develop://progress/{id}"),
            json!({ "phase": r.phase, "msg": m }),
        );
    }
}

fn set_phase(state: &AppState, app: &AppHandle, id: &str, p: &str) {
    {
        let mut g = state.develop.lock();
        if let Some(r) = g.get_mut(id) {
            r.phase = p.to_string();
        }
    }
    note(state, app, id, format!("=== phase: {p} ==="));
}

#[tauri::command]
pub async fn develop_start(
    state: State<'_, AppState>,
    app: AppHandle,
    spec: DevelopSpec,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    state.develop.lock().insert(
        id.clone(),
        DevelopRun {
            id: id.clone(),
            spec: spec.clone(),
            phase: "queued".into(),
            progress: vec![],
            done: false,
            error: None,
        },
    );
    let app2 = app.clone();
    let id2 = id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        let r = run_develop(state.inner(), app2.clone(), id2.clone(), spec).await;
        let mut g = state.develop.lock();
        if let Some(run) = g.get_mut(&id2) {
            run.done = true;
            if let Err(e) = r {
                run.error = Some(e.clone());
                run.phase = "error".into();
            } else {
                run.phase = "done".into();
            }
        }
    });
    Ok(id)
}

#[tauri::command]
pub fn develop_status(state: State<'_, AppState>, id: String) -> Option<DevelopRun> {
    state.develop.lock().get(&id).cloned()
}

async fn run_develop(
    state: &AppState,
    app: AppHandle,
    id: String,
    spec: DevelopSpec,
) -> Result<(), String> {
    let model = spec
        .model
        .clone()
        .unwrap_or_else(|| state.settings.lock().default_model.clone());
    let auto = spec.auto_approve.unwrap_or(true);
    let dir = spec.target_dir.clone();

    // PLAN — research/ + goal → .devil/plan.md
    set_phase(state, &app, &id, "plan");
    std::fs::create_dir_all(format!("{dir}/.devil")).map_err(|e| e.to_string())?;
    let plan_prompt = format!(
        "You are the lead engineer. Read research/INDEX.md and every research/*.md it links \
         (skip missing files). Goal: {goal}\n\
         Write .devil/plan.md: numbered implementation steps, each with files to touch, \
         acceptance check, and estimated risk. Keep it executable by a fresh worker that \
         only sees this file. Then reply DONE.",
        goal = if spec.goal.trim().is_empty() {
            "Infer the goal from the research documents.".to_string()
        } else {
            spec.goal.clone()
        }
    );
    run_oneshot_worker(state, app.clone(), "dev-planner", &dir, &model, &plan_prompt, auto).await?;
    if !std::path::Path::new(&format!("{dir}/.devil/plan.md")).exists() {
        return Err("planner did not write .devil/plan.md".into());
    }
    note(state, &app, &id, "plan written → .devil/plan.md");

    // IMPLEMENT × REVIEW × FIX cycles
    for cycle in 1..=spec.max_cycles.max(1) {
        set_phase(state, &app, &id, &format!("implement-{cycle}"));
        let impl_prompt = if cycle == 1 {
            "Read .devil/plan.md and implement it fully. Work through every step; run builds/tests \
             as needed. When finished, write .devil/implement-log.md summarizing what changed \
             and any deviations. Reply DONE.".to_string()
        } else {
            format!(
                "Read .devil/review-{prev}.md and fix every issue it lists, then re-check \
                 .devil/plan.md for unimplemented steps. Update .devil/implement-log.md. Reply DONE.",
                prev = cycle - 1
            )
        };
        run_oneshot_worker(
            state,
            app.clone(),
            &format!("implementer-{cycle}"),
            &dir,
            &model,
            &impl_prompt,
            auto,
        )
        .await?;

        set_phase(state, &app, &id, &format!("review-{cycle}"));
        let verify_line = spec
            .verify_command
            .as_deref()
            .map(|c| format!("Also run the verification command `{c}` and record its result."))
            .unwrap_or_default();
        let review_prompt = format!(
            "Review the implementation in this project against .devil/plan.md. \
             {verify_line} Write .devil/review-{cycle}.md with `## Pass` (what works) and \
             `## Issues` (each a concrete fix). If there are no real issues, write \
             `## Issues` containing exactly `NONE`. Reply DONE.",
        );
        run_oneshot_worker(state, app.clone(), "dev-reviewer", &dir, &model, &review_prompt, auto)
            .await?;

        // stop early when the reviewer reports no issues
        if let Ok(review) = std::fs::read_to_string(format!("{dir}/.devil/review-{cycle}.md")) {
            let lower = review.to_lowercase();
            if lower.contains("## issues") && lower.matches("none").count() > 0
                && !lower.contains("### issue")
            {
                note(state, &app, &id, format!("review-{cycle}: clean — stopping early"));
                break;
            }
        }
    }

    note(state, &app, &id, "개발 파이프라인 완료");
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("Devil of Devin — 개발 완료")
        .body(&spec.target_dir)
        .show();
    Ok(())
}
