# Devil of Devin (데빈만 쓰는 악마)

Tauri v2 데스크톱 앱 — Devin CLI(`devin acp`) 워커 오케스트레이터.
Windows 전용 (ConPTY 사용). 목표 모델: SWE-2 Max / SWE-2 Flash Max.

**기본 워크플로우 = [harness_loop_full_korean](https://github.com/GangJaeYu/harness_loop_full_korean)**
(IDEA → PRD → TRD → ARCHITECTURE → PLAN → HARNESS → LOOP). 앱은 그 흐름에서 Orca/조정자 역할을
맡는다: 단계는 프로젝트의 `coordinator` 워커에 `/스킬명`을 보내 진행하고, Loop(배정·워크트리·
게이트·독립 검증·STATE/LOG)는 앱의 Rust 엔진이 돌린다.

## 프로젝트 구조

```
research/            초기 설계/리서치 문서 (00~05)
report/              점검 보고서 (2026-11-06-assessment.md), 스크린샷
src-tauri/src/       Rust 백엔드
  main.rs            명령 등록
  state.rs           AppState + Settings(%APPDATA%\devil-of-devin\settings.json) + Project
  acp.rs             AcpAgent — devin acp spawn + JSON-RPC, 전역 `acp://event` 스트림, 사용량 집계
  workers.rs         워커 레지스트리(workers.json 영속) + run_oneshot(비상속 일회성 세션)
  project.rs         프로젝트 레지스트리, 7단계 산출물 상태, 문서 트리/읽기/쓰기
  harness.rs         하네스 스킬 팩 설치(~/.agents/skills) / 상태
  looprun.rs         Loop 엔진: plan_setup 태스크 파싱, 게이트 러너, 워크트리, 검증, STATE/LOG
  research.rs        리서치 파이프라인 plan→sweep→review→refine→synth (한국어 프롬프트)
  hooks.rs           127.0.0.1 axum 수신서버 + .devin/hooks.v1.json 주입
  skills.rs          로컬 스킬 스캔/토글 + GitHub 검색/설치
  pty.rs             portable-pty ConPTY (devin TUI 임베드)
  devin_cli.rs       devin.exe 탐색, auth status, models list, 로그인 PTY
  secrets.rs         Windows 자격증명 관리자 ($secret:NAME — env·MCP JSON 치환)
src/                 React+TS 프론트
  store.ts           zustand 전역 상태 (뷰 전환에도 대화/런 상태 유지)
  lib/ipc.ts         invoke 래퍼 · lib/events.ts 이벤트 브리지(한 번만 구독) · lib/coordinator.ts
  components/        Sidebar, PermissionModal, ActivityPanel(훅/로그/권한), Markdown, fields
  views/             Pipeline Loop Docs Workers Skills Terminal Settings
tools/               smoke-v2.mjs(CDP 스모크), loop-e2e.mjs(Loop 실전), gen-icon.mjs 등
```

## 빌드/실행

```bash
npm install
export PATH="$HOME/.cargo/bin:$PATH"
npx tauri dev                       # 개발 (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 로 CDP 검증 가능)
npx tauri build                     # 릴리즈 → src-tauri/target/release/
cd src-tauri && cargo check && cargo test   # Rust 검사 + looprun 파서 테스트
npx tsc -b && npx vite build        # 프론트
node tools/smoke-v2.mjs --prompt    # 실행 중인 앱에 CDP로 스모크 (워커 생성 + PONG)
node tools/loop-e2e.mjs D:\dod-looptest   # 스크래치 프로젝트로 Loop 전체 실행
```

MSVC는 `C:\Program Files\Microsoft Visual Studio\2022\Community`.

릴리즈: 버전을 `package.json`·`tauri.conf.json`·`Cargo.toml` 세 곳에서 올리고 `v*` 태그를 푸시하면
`.github/workflows/release.yml`(tauri-action)이 NSIS 빌드·서명·GitHub Release·`latest.json`을 만든다.
앱의 `tauri-plugin-updater`(`src/lib/updater.ts`, `components/UpdateBanner|UpdateCard`)가 그 엔드포인트를 확인한다.
서명 개인키 `~/.tauri/devil-of-devin.key` 는 저장소 시크릿 `TAURI_SIGNING_PRIVATE_KEY` 로만 쓰고 절대 커밋하지 않는다.

## Devin CLI 실측 메모

- 바이너리: `%LOCALAPPDATA%\devin\cli\bin\devin.exe` (3000.11.234)
- `devin acp`는 디스크 자격증명을 무시 → `credentials.toml`의 `windsurf_api_key`를
  `authenticate._meta.api_key`로 넘기면 바로 인증 (브라우저 불필요). `acp.rs ensure_authed`.
- `session/new` 응답의 `modes`: accept-edits | smart | ask | plan | bypass →
  `session/set_mode {sessionId, modeId}` 로 전환 (설정의 default_permission_mode / loop_mode).
- `session/update` 의 `available_commands_update` 에 `~/.agents/skills/*/SKILL.md` 가 `/이름` 으로
  노출되며, 프롬프트 텍스트 `/이름 …` 으로 호출된다 (실측). 하네스 13개 스킬은 이 경로로 설치.
- `session/prompt` 결과의 `usage {totalTokens,inputTokens,outputTokens}` 가 턴별 사용량 정본.
- `--agent-type review` = 읽기전용+셸 리뷰어 → Loop 의 독립 검증(task_validation) 워커.
- stderr 에 tracing 로그가 많다 → WARN/ERROR 만 `acp://event kind=log` 로 전달.
- Windows는 OS 샌드박스 미지원 → smart 모드 + PRD 자율운영 합의로 범위 제한.
- **게이트 셸 함정**: 앱 프로세스에서 bare `bash`는 WSL 런처(`System32ash.exe`)로 풀려 `C:/…` 경로가
  없다고 실패한다(exit 127). `looprun::git_bash()`가 Git for Windows의 bash를 찾아 쓴다 (`DOD_GIT_BASH`로 덮어쓰기 가능).
  게이트 스크립트는 `<작업폴더>/.devil/tmp/`에 쓴다 (%TEMP%의 한글 사용자명 경로가 깨짐).
- 개발 중 HMR 뒤 `store.ts`와 `store.ts?t=…` 두 인스턴스가 생길 수 있다 — CDP 스크립트가 이상한 상태를
  보면 페이지를 새로고침한다 (실제 앱 버그 아님).

## Loop 엔진 규약 (looprun.rs)

- 태스크: `plan_setup/**/*.md` 중 frontmatter `id:`(task 포함)·`status:`·`depends_on`·
  `acceptance_criteria`·`verification` — 상태 정본은 태스크 파일, 앱이 `status:` 줄만 고쳐 쓴다.
- 게이트: `harness_setup/quality_gates/{lint,unit,e2e,task_validation}.md` — frontmatter `command:`
  또는 첫 ```bash|powershell|cmd 블록을 그대로 실행. 명령이 없으면 review 에이전트가 판정.
- 시작 전 메인 체크아웃의 미커밋 변경을 스냅샷 커밋한다(`loop(<run>): 시작 전 작업 트리 스냅샷`, 설정으로 끌 수 있음).
  워크트리는 HEAD에서 갈라지므로 이 커밋이 없으면 harness_setup/plan_setup 이 워크트리에 없다. `.devil/`과
  `.devin/hooks.v1.json`은 조정자 파일이라 커밋하지 않는다(.git/info/exclude).
- 보호 경로(harness_setup/, plan_setup/, loop_setup/)를 바꾼 브랜치는 통합 거부. 다음 배정 때 그 브랜치는
  `loop/<id>-rejected-<ts>`로 보관하고 새 워크트리로 다시 시작한다.
- 흐름: 워크트리 `.devil/worktrees/<id>` (브랜치 `loop/<id>`) → 구현 워커 → lint→unit→e2e →
  별도 세션 검증(`RESULT: PASS|FAIL` 마지막 줄) → `--no-ff` 병합 → 통합 후 lint/unit.
- 시도: max_attempts(3) 초과 시 "전략 전환" 프롬프트, 총 2배까지. 실패/증거는
  `.devil/loop/<id>/`, 런 기록 `.devil/loop/run-*.json`, 보고서 `report-*.md`.
- STATE.md 는 앱이 전체 재작성, LOG.md 는 append. 두 파일은 조정자(앱)만 쓴다.

## 코딩 컨벤션

- 백엔드 명령명: snake_case (`worker_create`). 프론트 invoke는 camelCase args (serde rename_all camelCase).
- 전역 이벤트: `acp://event`, `workers://changed`, `projects://changed`, `loop://event`,
  `research://event`, `hook://event`, `pty://out/{id}`. 프론트는 `lib/events.ts` 에서 한 번만 구독.
- 새 Tauri 명령 추가 시: `main.rs` invoke_handler + `lib/ipc.ts` 래퍼 동시 추가.
- 사용자 노출 문자열·프롬프트는 한국어.
