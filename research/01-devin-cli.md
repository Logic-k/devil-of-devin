# 01 — Devin CLI 실측 분석

> 출처: 로컬 설치 `devin 3000.11.234` (`C:\Users\김민오\AppData\Local\devin\cli\bin\devin.exe`)
> + 번들 문서 `D:\Devin\resources\app\extensions\windsurf\devin\share\devin\docs\*.mdx`
> + 직접 실행 검증 (2026-11-06)

## 1. 워커 구동에 쓸 수 있는 인터페이스 3가지

### A. `devin acp` — ACP 서버 (핵심 채택)

JSON-RPC 2.0 over stdio. **에디터/IDE가 서브프로세스로 띄우는 용도** — 우리 앱이 정확히 이 위치.

```
devin acp [--model <MODEL>] [--agent-type <summarizer|review>] [--cloud]
          [--refusal-fallback <MODEL,...>]
```

- `--model`: `/model`과 동일한 퍼지 매칭 (family slug, alias, partial name). `DEVIN_MODEL` env도 가능
- `--cloud`: 로컬 에이전트 대신 Devin Cloud ACP WebSocket으로 릴레이
- `--agent-type review`: 읽기전용+셸 코드리뷰 에이전트 (검토 라운드에 활용 가능!)

**실측한 credential 정책 (중요)** — ACP 서버 로그:
> "ACP host is the sole source of credentials. Local CLI credentials (env vars, on-disk
> REPL store) will NOT be used. Waiting for the ACP host to call `authenticate`."

→ `devin acp`는 디스크의 credentials.toml을 **무시**하고 호스트(우리 앱)의 `authenticate`
호출만 기다린다. authMethods는 `[{"id":"devin-browser"}]` 하나(브라우저 OAuth).
`authenticate` 호출 시 즉시 `{}` 응답 + 브라우저가 열림 (비동기 완료).

- 문서상 `WINDSURF_API_KEY` env 지원 언급이 있으나 위 로그와 상충 → 구현 시 실측 필요.
  Plan B: 앱이 `authenticate`를 브로커하고, 로그인 상태는 `devin auth status`로 공유 확인.
- 참고: `%APPDATA%\devin\credentials.toml` 키 = `windsurf_api_key`, `api_server_url`,
  `devin_webapp_host`, `devin_api_url` (현재 머신은 `devin auth status` = Not logged in)

**실측한 initialize 응답** (전체는 dev 기록): capability 하이라이트
- `loadSession: true` — 세션 재개(리플레이) 가능
- `promptCapabilities`: image ✓, embeddedContext ✓ (audio ✗)
- `sessionCapabilities`: `list`, `delete`, `additionalDirectories`, `close`
- Cognition `_meta` 확장 (initialize/agentCapabilities._meta):
  `sessionDefaultConfigOptions`, `multiRootWorkspace`, `sessionListContentSearch`,
  `sessionListOrderBy`, `sessionRename`, `persistOnCreate`, `sessionShare`,
  `documentLifecycle`, `userEdits`, `terminalLifecycle`, `userConfig`,
  `userShellCommand`, `editableCommands`, `commandRevision`, `chains`, `megaplan`,
  `ruleMentions`, `midTurnPrompts`,
  `mentionTypes`: file / directory / git / rule / skill / plan / conversation / terminal / codemap
- 커스텀 알림 예시: `{"method":"_cognition.ai/mcp/serversChanged"}`

### B. `devin -p "<prompt>"` — 단발 프린트 모드

스크립트/자동화용. 응답을 stdout에 출력하고 종료.
`--respect-workspace-trust false` 필요 (비대화형이라 trust prompt 불가).
→ 리서치 모드의 "일회성 질의 워커", 요약/검토 패스에 적합.

### C. PTY로 `devin` TUI 구동

완전한 대화형 REPL을 ConPTY로 띄워 xterm.js에 붙이는 방식 (시제품이 이미 구현한 패턴).
ACP로 부족한 TUI 전용 기능 대비용. 주력은 아니지만 "풀 파워 모드"로 유지 가치.

## 2. 세션/모드/모델

- 세션: `devin -c`(최근 재개), `-r <id>`(지정 재개), `devin list --format json`
  (현 디렉터리 세션 나열 — 앱에서 작업이력 UI에 활용)
- 권한 모드: `normal` | `accept-edits` | `smart` | `dangerous`(bypass) | `autonomous`(--sandbox 필요)
  - 플래그: `--permission-mode <MODE>` / env `DEVIN_PERMISSION_MODE`
  - **smart**: 워크스페이스 편집 자동승인 + fast 모델이 나머지 안전성 판정 → "인증/승인 최소화" 요구에 핵심
  - ⚠ Windows는 OS샌드박스 미지원 → `autonomous` 불가 (WSL만). 대안: smart/bypass + permissions deny 규칙
- 모델 지정: `--model`, `DEVIN_MODEL`, `/model`, config `agent.model`
  - `devin models list --format json`으로 계정별 가용 모델 조회 (로그인 필요)
  - ACP에서 모델/thinking/speed 선택은 "session configuration controls"로 제공
    (changelog) → `session/new` 응답의 configOptions + `session/set_config_option` (실측 예정)
- `/session-stats`, `/usage` → 크레딧/토큰 사용량 (앱의 비용 표시용)

## 3. Hooks — 앱으로 이벤트 탭하기

`.devin/hooks.v1.json` (프로젝트) 또는 `%APPDATA%\devin\config.json` `hooks` 키 (글로벌).

- 이벤트: `PreToolUse`, `PostToolUse`, `PermissionRequest`, `UserPromptSubmit`,
  `Stop`, `PostCompaction`, `SessionStart`, `SessionEnd`
- command hook: 이벤트 JSON을 **stdin**으로 받음 (`session_id`, `prompt_id` 상관관계 ID 포함,
  `DEVIN_PROJECT_DIR` env)
- stdout JSON으로 제어: `{"decision":"block|approve","reason":...}`,
  `hookSpecificOutput.additionalContext`(컨텍스트 주입), `updatedInput`(인자 재작성),
  `updatedToolOutput`(결과 재작성)
- exit code 2 = 차단

→ **앱 통합 패턴 (시제품과 동일)**: 앱이 로컬 HTTP 리스너를 띄우고(127.0.0.1), 워커 시작 시
프로젝트 `.devin/hooks.v1.json`에 `curl POST http://127.0.0.1:<port>/hook` 훅을 주입 →
워커의 모든 도구 이벤트를 앱이 실시간 수집/표시. ACP의 session/update와 상호보완
(ACP=대화/도구카드, hooks=머신이벤트/감사로그).

## 4. 설정 파일

- 사용자: `%APPDATA%\devin\config.json` (JSONC). `agent.model`, `agent.subagent_model`,
  `permission_mode`, `permissions{allow,deny,ask}`, `hooks`, `read_config_from{claude,cursor,...}`
- 프로젝트: `.devin/config.json` — `permissions`, `read_config_from`, `hooks`만 가능
- MCP: `~/.config/devin/mcp_config.json` / `.devin/mcp_config.json` — `devin mcp add`
  (stdio|http, OAuth login 지원 — Vercel/Supabase MCP 연결점)
- 스킬 디렉터리 (`devin skills paths` 실측):
  - 글로벌: `%APPDATA%\devin\skills\`, `%APPDATA%\cognition\skills\`, `~\.agents\skills\`
  - 프로젝트: `.devin/skills\`, `.cognition\skills\`, `.agents\skills\`
  - 플러그인 캐시: `%APPDATA%\devin\cli\plugins\cache\...\skills\`
- rules: `devin rules paths`, 서브에이전트: `.devin/agents/*.md` (AGENT.md frontmatter)
- `devin doctor --json`: 서브에이전트 프로필 진단

## 5. 기타 활용 명령

| 명령 | 용도 |
|------|------|
| `devin auth login` / `status` / `logout` | 인증 (`--force-manual-token-flow` = 수동 토큰 붙여넣기: 헤드리스 대비) |
| `devin mcp add/login` | Vercel·Supabase MCP + OAuth |
| `devin plugins install <owner/repo>` | GitHub에서 플러그인(스킬+훅+룰 묶음) 설치 → 스킬 탭의 GitHub 설치 경로 |
| `devin update` / `version` | CLI 자체 업데이트 |
| `devin cloud drs ...` | 클라우드 세션/시크릿 관리 (`secret-create --from-env|--stdin` — 키가 커맨드라인에 안 남음) |
| `devin --export <path>` | 대화를 ATIF JSON으로 export (트랜스크립트 뷰에 활용) |

## 6. 시제품과의 대응표

| CommandCode 기능 | Devil of Devin 대응 |
|---|---|
| `acp_spawn(cwd, profileDir)` → `(cmd acp)` | `devin acp --model swe-2-max` |
| `pty_spawn` (ConPTY) + xterm | 동일 (portable-pty) — `devin` TUI |
| `session/request_permission` | ACP 동일 메서드 → 앱 승인 UI |
| `read_transcript(sessionId)` | `devin list --format json` + `session/load` 리플레이 / `--export` |
| accounts.json (멀티계정+billing) | 단일 Devin 계정 + `/session-stats` 크레딧 표시 |
| `..commandcode/cache/model-catalog` | `devin models list --format json` 캐시 |
| hooks → orca HTTP POST | hooks → Devil-of-Devin HTTP POST (127.0.0.1) |
