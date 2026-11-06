//! 리서치 파이프라인: plan → sweep(병렬) → review × N → refine → synth.
//! 산출물은 `<project>/research/` 아래 AI 참조용 md. IDEA/TRD 단계의 보조 도구.

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
    pub started_at: String,
    // ---- 진행 지표 (실시간 현황용) ----
    /// 전체 단계 수 = 계획 1 + 질문 수 + 검토 회차 + 보강 회차 + 종합(선택)
    #[serde(default)]
    pub steps_total: u32,
    #[serde(default)]
    pub steps_done: u32,
    /// 수집(sweep) 단계의 질문 수 / 완료 수
    #[serde(default)]
    pub sweep_total: u32,
    #[serde(default)]
    pub sweep_done: u32,
    /// 현재 검토 회차 / 총 회차
    #[serde(default)]
    pub round: u32,
    #[serde(default)]
    pub rounds_total: u32,
    /// 지금 하고 있는 일 한 줄
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub phase_started_at: String,
}

/// 파이프라인 전체 단계 수 (질문 수가 확정되면 다시 계산).
fn total_steps(spec: &ResearchSpec, questions: u32) -> u32 {
    let rounds = spec.review_rounds.max(1);
    1 + questions + rounds + spec.refine_rounds.min(rounds) + if spec.synthesize { 1 } else { 0 }
}

/// 런을 고치고 스냅샷을 이벤트로 내보낸다 (progress 로그는 남기지 않음).
fn mutate(state: &AppState, app: &AppHandle, id: &str, f: impl FnOnce(&mut ResearchRun)) {
    let snap = {
        let mut g = state.research.lock();
        if let Some(r) = g.get_mut(id) {
            f(r);
        }
        g.get(id).cloned()
    };
    if let Some(r) = snap {
        emit(app, &r, "");
    }
}

fn emit(app: &AppHandle, run: &ResearchRun, msg: &str) {
    let _ = app.emit(
        &format!("research://progress/{}", run.id),
        json!({ "phase": run.phase, "msg": msg }),
    );
    let _ = app.emit("research://event", json!({ "run": run, "msg": msg }));
}

#[tauri::command]
pub async fn research_start(
    state: State<'_, AppState>,
    app: AppHandle,
    spec: ResearchSpec,
) -> Result<String, String> {
    if !std::path::Path::new(&spec.target_dir).is_dir() {
        return Err(format!("대상 폴더가 없습니다: {}", spec.target_dir));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let run = ResearchRun {
        id: id.clone(),
        spec: spec.clone(),
        phase: "queued".into(),
        progress: vec![],
        done: false,
        error: None,
        artifacts: vec![],
        started_at: crate::workers::now_iso(),
        steps_total: total_steps(&spec, (spec.researchers * spec.questions_per_researcher).max(1)),
        steps_done: 0,
        sweep_total: 0,
        sweep_done: 0,
        round: 0,
        rounds_total: spec.review_rounds.max(1),
        current: "대기열".into(),
        phase_started_at: crate::workers::now_iso(),
    };
    state.research.lock().insert(id.clone(), run.clone());
    emit(&app, &run, "대기열에 추가");

    let app2 = app.clone();
    let id2 = id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        if let Err(e) = run_pipeline(state.inner(), app2.clone(), id2.clone(), spec).await {
            let stopped = stop_requested(state.inner(), &id2) || e == "__STOPPED__";
            let snap = {
                let mut g = state.research.lock();
                if let Some(r) = g.get_mut(&id2) {
                    r.done = true;
                    if stopped {
                        r.phase = "stopped".into();
                        r.current = "사용자가 중단함 — 이미 쓰인 research/ 파일은 남아 있음".into();
                    } else {
                        r.error = Some(e.clone());
                        r.phase = "error".into();
                    }
                }
                g.get(&id2).cloned()
            };
            if let Some(r) = snap {
                emit(&app2, &r, &if stopped { "중단됨".to_string() } else { format!("오류: {e}") });
            }
        }
        state.research_stop.lock().remove(&id2);
    });
    Ok(id)
}

fn stop_requested(state: &AppState, id: &str) -> bool {
    state.research_stop.lock().contains(id)
}

/// 단계 경계마다 호출: 중단 요청이 있으면 파이프라인을 끝낸다.
fn check_stop(state: &AppState, id: &str) -> Result<(), String> {
    if stop_requested(state, id) {
        Err("__STOPPED__".into())
    } else {
        Ok(())
    }
}

/// 리서치 중단: 플래그를 세우고 이 런의 일회성 워커(research-*)를 모두 종료한다.
/// 이미 쓰인 research/ 파일은 그대로 남는다.
#[tauri::command]
pub async fn research_stop(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let run = state.research.lock().get(&id).cloned().ok_or("해당 리서치 런이 없습니다")?;
    if run.done {
        return Err("이미 끝난 런입니다".into());
    }
    state.research_stop.lock().insert(id.clone());
    mutate(state.inner(), &app, &id, |r| r.current = "중단 요청 — 리서치 워커 종료 중".into());
    note(state.inner(), &app, &id, "사용자가 중단을 요청했습니다");
    let target = run.spec.target_dir.clone();
    let victims: Vec<_> = state
        .acp
        .lock()
        .values()
        .filter(|a| a.label.starts_with("research-") && a.cwd == target)
        .cloned()
        .collect();
    for a in victims {
        a.kill().await;
    }
    Ok(())
}

#[tauri::command]
pub fn research_status(state: State<'_, AppState>, id: String) -> Option<ResearchRun> {
    state.research.lock().get(&id).cloned()
}

#[tauri::command]
pub fn research_list(state: State<'_, AppState>) -> Vec<ResearchRun> {
    state.research.lock().values().cloned().collect()
}

fn note(state: &AppState, app: &AppHandle, id: &str, msg: impl Into<String>) {
    let m = msg.into();
    let snap = {
        let mut g = state.research.lock();
        if let Some(r) = g.get_mut(id) {
            r.progress.push(m.clone());
        }
        g.get(id).cloned()
    };
    if let Some(r) = snap {
        emit(app, &r, &m);
    }
}

fn set_phase(state: &AppState, app: &AppHandle, id: &str, p: &str) {
    if let Some(r) = state.research.lock().get_mut(id) {
        r.phase = p.to_string();
        r.phase_started_at = crate::workers::now_iso();
        r.current = String::new();
    }
    note(state, app, id, format!("=== 단계: {p} ==="));
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
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| state.settings.lock().default_model.clone());
    let auto = spec.auto_approve.unwrap_or(true);
    let dir = spec.target_dir.clone();
    std::fs::create_dir_all(format!("{dir}/research/raw")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(format!("{dir}/research/review")).map_err(|e| e.to_string())?;

    // ---- PLAN ----
    check_stop(state, &id)?;
    set_phase(state, &app, &id, "plan");
    let n_q = (spec.researchers * spec.questions_per_researcher).max(1);
    mutate(state, &app, &id, |r| r.current = format!("플래너가 리서치 질문 {n_q}개를 만드는 중"));
    let plan_path = format!("{dir}/research/plan.json");
    let plan_prompt = format!(
        "당신은 리서치 플래너입니다. 주제: \"{topic}\".\n\
         {path} 에 {{\"questions\":[\"...\",...]}} 형태의 JSON 파일을 작성하세요. 서로 겹치지 않는 구체적인 \
         리서치 질문 {n_q}개를 담습니다(최신 동향, 함정, API/도구, 경쟁/대안, 벤치마크, 비용 등 다양한 각도). \
         질문은 한국어로 씁니다. 파일을 쓴 뒤 DONE 이라고만 답하세요.",
        topic = spec.topic,
        path = plan_path.replace('\\', "/")
    );
    run_oneshot_worker(state, app.clone(), "research-planner", &dir, &model, &plan_prompt, auto).await?;
    let questions: Vec<String> = std::fs::read_to_string(&plan_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("questions")?.as_array().map(|a| {
                a.iter()
                    .filter_map(|q| q.as_str().map(|s| s.to_string()))
                    .collect()
            })
        })
        .unwrap_or_else(|| {
            (0..n_q)
                .map(|i| format!("{} — 리서치 질문 {}", spec.topic, i + 1))
                .collect()
        });
    let q_n = questions.len() as u32;
    mutate(state, &app, &id, |r| {
        r.steps_done += 1;
        r.sweep_total = q_n;
        r.steps_total = total_steps(&r.spec, q_n);
    });
    note(state, &app, &id, format!("계획: 질문 {}개", questions.len()));

    // ---- SWEEP (parallel, capped at spec.researchers in flight) ----
    set_phase(state, &app, &id, "sweep");
    let indexed: Vec<(usize, String)> = questions.iter().cloned().enumerate().collect();
    for chunk in indexed.chunks(spec.researchers.max(1) as usize) {
        check_stop(state, &id)?;
        let (first, last) = (chunk.first().map(|c| c.0 + 1).unwrap_or(1), chunk.last().map(|c| c.0 + 1).unwrap_or(1));
        let n_in_flight = chunk.len();
        mutate(state, &app, &id, |r| {
            r.current = format!("리서처 {n_in_flight}명이 질문 {first}~{last} 조사 중 ({}/{} 완료)", r.sweep_done, r.sweep_total)
        });
        let mut handles = Vec::new();
        for (idx, q2) in chunk.iter().cloned() {
            let app2 = app.clone();
            let dir2 = dir.clone();
            let m2 = model.clone();
            let id2 = id.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                let st = app2.state::<AppState>();
                let prompt = format!(
                    "다음 질문을 web_search / web_fetch 로 철저히 조사하세요:\n\"{q2}\"\n\
                     결과를 research/raw/q{idx:02}.md 에 한국어로 씁니다: 주장 표(| 주장 | 확신도 | 출처 URL | 확인 날짜 |), \
                     메모, 그리고 `## 출처` 목록. 실제로 가져온(fetch한) 페이지만 인용합니다. 끝나면 DONE 이라고 답하세요."
                );
                let res = run_oneshot_worker(
                    st.inner(),
                    app2.clone(),
                    &format!("research-{idx}"),
                    &dir2,
                    &m2,
                    &prompt,
                    true,
                )
                .await;
                mutate(st.inner(), &app2, &id2, |r| {
                    r.sweep_done += 1;
                    r.steps_done += 1;
                    r.current = format!("질문 {}/{} 수집 완료 — 질문 {} 끝", r.sweep_done, r.sweep_total, idx + 1);
                });
                res
            }));
        }
        for h in handles {
            match h.await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => note(state, &app, &id, format!("리서처 오류: {e}")),
                Err(e) => note(state, &app, &id, format!("리서처 join 오류: {e}")),
            }
        }
    }
    note(state, &app, &id, "수집 완료 → research/raw/*.md");

    // ---- REVIEW × REFINE ----
    for round in 1..=spec.review_rounds.max(1) {
        check_stop(state, &id)?;
        set_phase(state, &app, &id, &format!("review-{round}"));
        mutate(state, &app, &id, |r| {
            r.round = round;
            r.current = format!("{round}차 검토: 수집 자료 교차 검증 (모순·공백 찾기)");
        });
        let review_prompt = format!(
            "당신은 \"{topic}\" 리서치의 {round}차 검토자입니다. research/raw/ 아래 모든 파일을 읽고 주장을 교차 검증하세요. \
             모순, 빠진 관점, 출처 없는 주장을 찾아 research/review/round-{round}.md 에 한국어로 씁니다: \
             `## 검증됨`, `## 모순`, `## 공백`(각 공백은 구체적인 후속 질문 하나). 끝나면 DONE 이라고 답하세요.",
            topic = spec.topic
        );
        run_oneshot_worker(state, app.clone(), "research-reviewer", &dir, &model, &review_prompt, auto).await?;
        mutate(state, &app, &id, |r| r.steps_done += 1);

        if round <= spec.refine_rounds {
            check_stop(state, &id)?;
            set_phase(state, &app, &id, &format!("refine-{round}"));
            mutate(state, &app, &id, |r| r.current = format!("{round}차 보강: 검토에서 나온 공백을 추가 조사"));
            let refine_prompt = format!(
                "research/review/round-{round}.md 의 `## 공백` 절을 읽고 각 공백을 web_search/web_fetch 로 추가 조사한 뒤 \
                 research/raw/refine-{round}.md 에 같은 주장 표 형식으로 덧붙이세요. 끝나면 DONE 이라고 답하세요."
            );
            run_oneshot_worker(state, app.clone(), "research-refiner", &dir, &model, &refine_prompt, auto).await?;
            mutate(state, &app, &id, |r| r.steps_done += 1);
        }
    }

    // ---- SYNTH ----
    if spec.synthesize {
        check_stop(state, &id)?;
        set_phase(state, &app, &id, "synth");
        mutate(state, &app, &id, |r| r.current = "종합 보고서 작성 + research/INDEX.md 갱신".into());
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
        let slug = if slug.is_empty() { "topic".to_string() } else { slug };
        let synth_prompt = format!(
            "research/raw/ 와 research/review/ 의 검증된 내용을 모두 종합해 \"{topic}\" 에 대한 research/{slug}.md 를 작성하세요.\n\
             AI가 나중에 참조하기 쉬운 형식: frontmatter(topic, updated, confidence), `## 결정`, `## 근거`, `## 출처`(표), `## 미결`. \
             한국어로 씁니다. 그런 다음 research/INDEX.md 에 research/{slug}.md 로 가는 한 줄 링크를 추가하세요. 끝나면 DONE 이라고 답하세요.",
            topic = spec.topic, slug = slug
        );
        run_oneshot_worker(state, app.clone(), "research-synth", &dir, &model, &synth_prompt, auto).await?;
        if let Some(r) = state.research.lock().get_mut(&id) {
            r.artifacts.push(format!("research/{slug}.md"));
            r.steps_done += 1;
        }
        note(state, &app, &id, format!("종합 → research/{slug}.md"));
    }

    check_stop(state, &id)?;
    set_phase(state, &app, &id, "done");
    mutate(state, &app, &id, |r| {
        r.current = "완료".into();
        r.steps_done = r.steps_total;
    });
    let snap = {
        let mut g = state.research.lock();
        if let Some(r) = g.get_mut(&id) {
            r.done = true;
        }
        g.get(&id).cloned()
    };
    if let Some(r) = snap {
        emit(&app, &r, "완료");
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
