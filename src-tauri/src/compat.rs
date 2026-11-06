//! Devin CLI 호환성 자가 점검: 앱이 기대는 비공식 동작(토큰 인증, `--agent-type review`,
//! 세션 모드, ACP 핸드셰이크)이 설치된 Devin 에서 아직 유효한지 시작 시 확인한다.

use crate::acp::{AcpAgent, SpawnOpts};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    pub name: String,
    /// None = 건너뜀 (선행 점검 실패)
    pub ok: Option<bool>,
    /// 실패하면 핵심 흐름(워커·Loop)이 깨지는 항목
    pub critical: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReport {
    pub ran_at: String,
    pub devin_version: Option<String>,
    pub tested_version: String,
    pub version_status: String,
    pub checks: Vec<Check>,
    /// 핵심 항목이 모두 통과
    pub ok: bool,
    pub failures: usize,
}

fn check(id: &str, name: &str, ok: Option<bool>, critical: bool, detail: impl Into<String>) -> Check {
    Check { id: id.into(), name: name.into(), ok, critical, detail: detail.into() }
}

#[tauri::command]
pub async fn devin_selfcheck(
    state: tauri::State<'_, crate::state::AppState>,
    app: AppHandle,
) -> Result<CompatReport, String> {
    tokio::time::timeout(Duration::from_secs(240), run(&state, app))
        .await
        .map_err(|_| "자가 점검 시간 초과 (240초)".to_string())?
}

async fn run(state: &crate::state::AppState, app: AppHandle) -> Result<CompatReport, String> {
    let mut checks: Vec<Check> = Vec::new();
    let (configured, loop_mode, default_mode) = {
        let s = state.settings.lock();
        (s.devin_path.clone(), s.loop_mode.clone(), s.default_permission_mode.clone())
    };

    // 1. 바이너리
    let bin = crate::devin_cli::find_devin(&configured);
    let mut devin_version: Option<String> = None;
    match &bin {
        Some(p) => {
            let out = tokio::process::Command::new(p)
                .arg("version")
                .stdin(std::process::Stdio::null())
                .output()
                .await;
            let v = out
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty());
            devin_version = v.clone();
            checks.push(check(
                "binary",
                "devin.exe 탐색",
                Some(true),
                true,
                format!("{} · {}", p.display(), v.unwrap_or_else(|| "버전 불명".into())),
            ));
        }
        None => {
            checks.push(check("binary", "devin.exe 탐색", Some(false), true, "찾지 못함 — 설정에서 devin_path 지정"));
            return Ok(finish(devin_version, checks));
        }
    }

    // 작업 디렉터리: 앱 설정 폴더 (항상 존재)
    let cwd = crate::state::Settings::dir();
    let _ = std::fs::create_dir_all(&cwd);
    let cwd_s = cwd.display().to_string();
    let opts = |agent_type: Option<&str>, label: &str| SpawnOpts {
        cwd: cwd_s.clone(),
        model: None,
        agent_type: agent_type.map(|s| s.to_string()),
        cloud: false,
        auto_approve: true,
        env: HashMap::new(),
        label: Some(label.into()),
    };

    // 2. ACP 핸드셰이크 (+ spawn 안에서 토큰 인증 시도)
    let main = match AcpAgent::spawn(state, app.clone(), "selfcheck-main".into(), opts(None, "selfcheck")).await {
        Ok(a) => a,
        Err(e) => {
            checks.push(check("acp", "ACP 핸드셰이크 (initialize)", Some(false), true, format!("{e}")));
            checks.push(check("token", "세션 토큰 인증 (_meta.api_key)", None, false, "건너뜀"));
            checks.push(check("modes", "세션 모드 (session/new)", None, true, "건너뜀"));
            checks.push(check("review", "리뷰 에이전트 (--agent-type review)", None, true, "건너뜀"));
            return Ok(finish(devin_version, checks));
        }
    };
    let init = main.init_info().unwrap_or(Value::Null);
    let proto = init.get("protocolVersion").and_then(|v| v.as_u64());
    let methods: Vec<String> = init
        .get("authMethods")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|m| m.get("id")?.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let load_session = init
        .pointer("/agentCapabilities/loadSession")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proto_ok = proto == Some(1);
    checks.push(check(
        "acp",
        "ACP 핸드셰이크 (initialize)",
        Some(proto_ok),
        true,
        format!(
            "protocolVersion={} · authMethods=[{}] · loadSession={}",
            proto.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
            methods.join(","),
            load_session
        ),
    ));
    if !methods.iter().any(|m| m == "devin-browser") {
        checks.push(check(
            "authmethod",
            "브라우저 인증 방식 (devin-browser)",
            Some(false),
            false,
            format!("광고된 방식: [{}] — 폴백 로그인이 다른 방식을 쓸 수 있음", methods.join(",")),
        ));
    }

    // 3. 토큰 인증
    let has_token = crate::acp::read_session_api_key().is_some();
    let authed = main.is_authed();
    checks.push(if !has_token {
        check(
            "token",
            "세션 토큰 인증 (_meta.api_key)",
            Some(false),
            false,
            "credentials.toml 에 세션 토큰 없음 — 워커 생성 시 브라우저 로그인으로 폴백",
        )
    } else if authed {
        check("token", "세션 토큰 인증 (_meta.api_key)", Some(true), false, "credentials.toml 토큰으로 무인 인증 성공")
    } else {
        check(
            "token",
            "세션 토큰 인증 (_meta.api_key)",
            Some(false),
            false,
            "토큰이 있지만 ACP 가 거부함 — Devin 이 비공식 인증 경로를 바꿨을 수 있음. 브라우저 로그인 폴백으로 동작",
        )
    });

    // 4. 세션 모드
    if authed {
        match main.session_new_mcp(&cwd_s, vec![]).await {
            Ok(v) => {
                let modes: Vec<String> = v
                    .pointer("/modes/availableModes")
                    .and_then(|m| m.as_array())
                    .map(|a| a.iter().filter_map(|m| m.get("id")?.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let need = [loop_mode.as_str(), default_mode.as_str()];
                let missing: Vec<&str> = need
                    .iter()
                    .copied()
                    .filter(|m| !m.is_empty() && !modes.iter().any(|x| x == m))
                    .collect();
                let ok = missing.is_empty() && !modes.is_empty();
                checks.push(check(
                    "modes",
                    "세션 모드 (session/new)",
                    Some(ok),
                    true,
                    if ok {
                        format!("사용 가능: [{}]", modes.join(","))
                    } else {
                        format!("설정된 모드 [{}] 가 없음 · 사용 가능: [{}]", missing.join(","), modes.join(","))
                    },
                ));
                if let Some(sid) = v.get("sessionId").and_then(|s| s.as_str()) {
                    let _ = main
                        .call_timeout("session/close", serde_json::json!({ "sessionId": sid }), Duration::from_secs(10))
                        .await;
                }
            }
            Err(e) => checks.push(check("modes", "세션 모드 (session/new)", Some(false), true, format!("{e}"))),
        }
    } else {
        checks.push(check("modes", "세션 모드 (session/new)", None, true, "인증되지 않아 건너뜀 — 로그인 후 다시 점검"));
    }
    main.kill().await;

    // 5. 리뷰 에이전트 플래그 (Loop 독립 검증)
    match AcpAgent::spawn(state, app.clone(), "selfcheck-review".into(), opts(Some("review"), "selfcheck-review")).await {
        Ok(a) => {
            checks.push(check("review", "리뷰 에이전트 (--agent-type review)", Some(true), true, "플래그 수용 · initialize 성공"));
            a.kill().await;
        }
        Err(e) => checks.push(check(
            "review",
            "리뷰 에이전트 (--agent-type review)",
            Some(false),
            true,
            format!("{e} — Loop 의 독립 검증 워커를 띄울 수 없음"),
        )),
    }

    Ok(finish(devin_version, checks))
}

fn finish(devin_version: Option<String>, checks: Vec<Check>) -> CompatReport {
    let failures = checks.iter().filter(|c| c.ok == Some(false)).count();
    let ok = !checks.iter().any(|c| c.critical && c.ok == Some(false));
    CompatReport {
        ran_at: crate::workers::now_iso(),
        tested_version: crate::devin_cli::TESTED_DEVIN_VERSION.into(),
        version_status: crate::devin_cli::version_status(devin_version.as_deref()),
        devin_version,
        checks,
        ok,
        failures,
    }
}
