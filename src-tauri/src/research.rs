use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::workers::run_oneshot_worker;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSpec {
    pub topic: String,
    pub target_dir: String,
    pub researchers: u32,
    pub questions_per_researcher: u32,
    pub review_rounds: u32,
    pub refine_rounds: u32,
    pub synthesize: bool,
    pub model: Option<String>,
    pub auto_approve: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRun {
    pub id: String,
    pub spec: ResearchSpec,
    pub phase: String,
    pub progress: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
    pub artifacts: Vec<String>,
}

impl ResearchRun {
    fn note(&mut self, app: &AppHandle, msg: impl Into<String>) {
        let m = msg.into();
        self.progress.push(m.clone());
        let _ = app.emit(
            &format!("research://progress/{}", self.id),
            json!({ "phase": self.phase, "msg": m }),
        );
    }
    fn set_phase(&mut self, app: &AppHandle, p: &str) {
        self.phase = p.to_string();
        self.note(app, format!("=== phase: {p} ==="));
    }
}

#[tauri::command]
pub async fn research_start(
    state: State<'_, AppState>,
    app: AppHandle,
    spec: ResearchSpec,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    state.research.lock().insert(
        id.clone(),
        ResearchRun {
            id: id.clone(),
            spec: spec.clone(),
            phase: "queued".into(),
            progress: vec![],
            done: false,
            error: None,
            artifacts: vec![],
        },
    );

    let app2 = app.clone();
    let id2 = id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        if let Err(e) = run_pipeline(state.inner(), app2.clone(), id2.clone(), spec).await {
            if let Some(r) = state.research.lock().get_mut(&id2) {
                r.error = Some(e.clone());
                r.done = true;
            }
            let _ = app2.emit(&format!("research://error/{id2}"), json!({ "error": e }));
        }
    });
    Ok(id)
}

#[tauri::command]
pub fn research_status(state: State<'_, AppState>, id: String) -> Option<ResearchRun> {
    state.research.lock().get(&id).cloned()
}

fn run_note(state: &AppState, app: &AppHandle, id: &str, msg: impl Into<String>) {
    if let Some(r) = state.research.lock().get_mut(id) {
        r.note(app, msg);
    }
}

fn set_phase(state: &AppState, app: &AppHandle, id: &str, p: &str) {
    if let Some(r) = state.research.lock().get_mut(id) {
        r.set_phase(app, p);
    }
}

async fn run_pipeline(
    state: &AppState,
    app: AppHandle,
    id: String,
    spec: ResearchSpec,
) -> Result<(), String> {
    let model = spec
        .model
        .clone()
        .unwrap_or_else(|| state.settings.lock().default_model.clone());
    let auto = spec.auto_approve.unwrap_or(true);
    let dir = spec.target_dir.clone();
    std::fs::create_dir_all(format!("{dir}/research/raw")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(format!("{dir}/research/review")).map_err(|e| e.to_string())?;

    // ---- PLAN ----
    set_phase(state, &app, &id, "plan");
    let n_q = (spec.researchers * spec.questions_per_researcher).max(1);
    let plan_path = format!("{dir}/research/plan.json");
    let plan_prompt = format!(
        "You are the research planner for topic: \"{topic}\".\n\
         Write a JSON file at {path} shaped \
         {{\"questions\":[\"...\",...]}} containing {n_q} concrete, independent research \
         questions (cover different angles: state of the art, pitfalls, APIs, competitors, \
         benchmarks, costs). Write the file, then reply with just DONE.",
        topic = spec.topic,
        path = plan_path.replace('\\', "/")
    );
    run_oneshot_worker(state, app.clone(), "planner", &dir, &model, &plan_prompt, auto).await?;
    let questions: Vec<String> = std::fs::read_to_string(&plan_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("questions")?
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|q| q.as_str().map(|s| s.to_string()))
                        .collect()
                })
        })
        .unwrap_or_else(|| {
            (0..n_q)
                .map(|i| format!("{} — research question {}", spec.topic, i + 1))
                .collect()
        });
    run_note(state, &app, &id, format!("plan: {} questions", questions.len()));

    // ---- SWEEP (parallel, capped at spec.researchers in flight) ----
    set_phase(state, &app, &id, "sweep");
    let indexed: Vec<(usize, String)> = questions.iter().cloned().enumerate().collect();
    for chunk in indexed.chunks(spec.researchers.max(1) as usize) {
        let mut handles = Vec::new();
        for (idx, q2) in chunk.iter().cloned() {
            let app2 = app.clone();
            let dir2 = dir.clone();
            let m2 = model.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                let st = app2.state::<AppState>();
                let prompt = format!(
                    "Research this question thoroughly using web_search and web_fetch:\n\"{q2}\"\n\
                     Write findings to research/raw/q{idx:02}.md: a claims table \
                     (| claim | confidence | source url | fetched date |), notes, then a \
                     `## Sources` list. Cite only pages you actually fetched. Then reply DONE."
                );
                run_oneshot_worker(
                    st.inner(),
                    app2.clone(),
                    &format!("researcher-{idx}"),
                    &dir2,
                    &m2,
                    &prompt,
                    true,
                )
                .await
            }));
        }
        for h in handles {
            match h.await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => run_note(state, &app, &id, format!("researcher error: {e}")),
                Err(e) => run_note(state, &app, &id, format!("researcher join error: {e}")),
            }
        }
    }
    run_note(state, &app, &id, "sweep complete → research/raw/*.md");

    // ---- REVIEW × REFINE ----
    for round in 1..=spec.review_rounds.max(1) {
        set_phase(state, &app, &id, &format!("review-{round}"));
        let review_prompt = format!(
            "You are reviewer round {round} for research on \"{topic}\".\n\
             Read every file under research/raw/. Cross-check claims; flag contradictions, \
             missing angles, and unsourced claims. Write research/review/round-{round}.md \
             with `## Verified`, `## Contradictions`, `## Gaps` (each gap = one concrete \
             follow-up question). Then reply DONE.",
            topic = spec.topic
        );
        run_oneshot_worker(state, app.clone(), "reviewer", &dir, &model, &review_prompt, auto)
            .await?;

        if round <= spec.refine_rounds {
            set_phase(state, &app, &id, &format!("refine-{round}"));
            let refine_prompt = format!(
                "Read the `## Gaps` section of research/review/round-{round}.md. Research each \
                 gap with web_search/web_fetch and append findings to \
                 research/raw/refine-{round}.md (same claims-table format). Then reply DONE."
            );
            run_oneshot_worker(state, app.clone(), "refiner", &dir, &model, &refine_prompt, auto)
                .await?;
        }
    }

    // ---- SYNTH ----
    if spec.synthesize {
        set_phase(state, &app, &id, "synth");
        let slug: String = spec
            .topic
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .take(6)
            .collect::<Vec<_>>()
            .join("-");
        let synth_prompt = format!(
            "Synthesize ALL verified research under research/raw/ and research/review/ for \
             \"{topic}\" into research/{slug}.md.\n\
             Format for future AI readers: frontmatter (topic, updated, confidence), \
             `## Decisions`, `## Evidence`, `## Sources` (markdown table), `## Open questions`.\n\
             Then update research/INDEX.md adding a one-line link to research/{slug}.md. \
             Reply DONE when written.",
            topic = spec.topic, slug = slug
        );
        run_oneshot_worker(state, app.clone(), "synth", &dir, &model, &synth_prompt, auto).await?;
        if let Some(r) = state.research.lock().get_mut(&id) {
            r.artifacts.push(format!("research/{slug}.md"));
            r.note(&app, format!("synthesized → research/{slug}.md"));
        }
    }

    set_phase(state, &app, &id, "done");
    if let Some(r) = state.research.lock().get_mut(&id) {
        r.done = true;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("Devil of Devin — 리서치 완료")
        .body(&spec.topic)
        .show();
    Ok(())
}
