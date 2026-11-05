# Devil of Devin (데빈만 쓰는 악마)

Tauri v2 데스크톱 앱 — Devin CLI(`devin acp`) 워커 오케스트레이터.
Windows 전용 (ConPTY 사용). 목표 모델: SWE-2 Max / SWE-2 Flash Max.

## 프로젝트 구조

```
research/            설계/리서치 문서 (00~05) — 반드시 먼저 읽을 것
src-tauri/src/       Rust 백엔드
  main.rs            명령 등록
  state.rs           AppState + Settings(%APPDATA%\devil-of-devin\settings.json)
  devin_cli.rs       devin.exe 탐색, auth status, models list, -p oneshot
  acp.rs             AcpAgent — devin acp spawn + JSON-RPC(stdio, 줄단위 JSON)
  pty.rs             portable-pty ConPTY (devin TUI 임베드)
  workers.rs         워커 레지스트리 + run_oneshot_worker(파이프라인용)
  research.rs        리서치 파이프라인 plan→sweep→review→refine→synth
  hooks.rs           127.0.0.1 axum 수신서버 + .devin/hooks.v1.json 주입
  skills.rs          로컬 스킬 스캔/토글 + GitHub 검색/설치
src/                 React+TS 프론트
  lib/ipc.ts         invoke/event 래퍼
  tabs/              WorkersTab ResearchTab SkillsTab TerminalTab SettingsTab
tools/gen-icon.mjs   아이콘 생성기 (node)
```

## 빌드/실행

```bash
# 한 번만: node_modules 설치
npm install

# 개발 실행 (vite + tauri)
export PATH="$HOME/.cargo/bin:$PATH"
npx tauri dev

# 릴리즈 빌드
npx tauri build            # → src-tauri/target/release/devil-of-devin.exe + installer

# Rust만 체크
cd src-tauri && cargo check
```

주의: 이 머신은 PATH에 cargo가 아직 없을 수 있음 → 명령 전 `export PATH="$HOME/.cargo/bin:$PATH"`.
MSVC는 `C:\Program Files\Microsoft Visual Studio\2022\Community` (VS Community, cl/link 있음).

## Devin CLI 실측 메모

- 바이너리: `%LOCALAPPDATA%\devin\cli\bin\devin.exe` (현재 3000.11.234)
- `devin acp` 는 디스크 자격증명을 **무시** → 호스트가 `authenticate` 호출해야 함
  (authMethods: `devin-browser` 하나). 미인증이면 `session/new`이 `auth_required` 에러.
- 인증 공유: `devin auth login`(브라우저) 후 REPL/print 경로는 credentials.toml 사용.
  ACP 경로는 호스트 authenticate 필요 → 앱은 lazy-auth: session/new 실패 시 authenticate 호출.
- 모델: `--model`(퍼지매칭), `DEVIN_MODEL`. 세션 중 모델/사고량 변경은 session config options.
- `devin acp --agent-type review` = 읽기전용 리뷰어 워커.
- Windows는 OS 샌드박스 미지원 → autonomous 불가; smart/bypass + permissions.deny 사용.

## 코딩 컨벤션

- 백엔드 명령명: snake_case (`worker_create`). 프론트 invoke는 camelCase args (serde rename_all camelCase).
- 프론트 이벤트: `acp://update/{agentId}`, `acp://permission/{agentId}`,
  `pty://out/{id}`, `research://progress/{id}`, `hook://event`.
- ACP 커스텀 메서드는 `_` 접두(`_cognition.ai/...`) — acp_call로 임의 호출 가능.
- 새 Tauri 명령 추가 시: `main.rs` invoke_handler + `lib/ipc.ts` 래퍼 동시 추가.
