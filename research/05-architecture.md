# 05 — 확정 아키텍처

## 스택

| 계층 | 선택 | 이유 |
|------|------|------|
| 셸 | **Tauri v2** (Rust + WebView2) | 시제품과 동일. 가볍고 설치형 |
| 백엔드 | Rust (tokio) | ACP JSON-RPC 직접 구현 (SDK 대신) — Cognition `_meta` 확장 자유도 |
| PTY | `portable-pty` (ConPTY) | 시제품 검증된 선택 — `devin` TUI 임베드용 |
| 프론트 | Vite + React + TS + Tailwind | 빠른 UI 반복 |
| 상태 | JSON 파일 + 인메모리 (`%APPDATA%\devil-of-devin\`) | SQLite는 v2에서 |
| 이벤트 탭 | `.devin/hooks.v1.json` → `curl 127.0.0.1:<port>` | 시제품 orca 패턴 재사용 |

## 모듈 구성 (src-tauri/src/)

```
main.rs            빌드/셋업, 플러그인
devin_cli.rs       devin.exe 경로 탐색, version/models/auth status, -p oneshot 래퍼
acp.rs             AcpManager: spawn devin acp, JSON-RPC 프레이밍, 요청/응답/알림 라우팅,
                   session 생명주기 (new/prompt/cancel/load/close),
                   권한요청→이벤트, config_option 변경
pty.rs             PtyManager: ConPTY spawn devin (TUI 모드), write/resize/kill
workers.rs         Worker/Task/Run 모델, DAG 스케줄러, 레인 동시성, worktree 관리
research.rs        리서치 파이프라인(PLAN→SWEEP→REVIEW→REFINE→SYNTH) 드라이버
hooks.rs           로컬 HTTP 리스너 + hooks.v1.json 생성/주입
skills.rs          로컬 스킬 스캔/토글 + GitHub 검색/다운로드/설치
accounts.rs        인증 상태(devin auth status), ACP authenticate 브로커, API키 보관
secrets.rs         Windows Credential Manager (keyring) — Vercel/SB 토큰
settings.rs        앱 설정 (모델 기본값, 자동승인 정책, 포트)
```

## 프론트 (src/)

```
App.tsx            탭 셸: Workers | Chat | Research | Tasks | Skills | Settings
workers/           워커 카드 그리드 + 생성 다이얼로그(프로필/모델/모드/cwd)
chat/              ACP 트랜스크립트 렌더 (메시지/생각/툴콜/플랜/usage) + 입력 + 권한 모달
research/          파이프라인 실행 UI (횟수/라운드 슬라이더, 진행도, md 산출물 뷰)
tasks/             태스크 DAG 보드 (간단 칸반)
skills/            로컬 스킬 목록 + GitHub 검색 + 설치
terminal/          xterm.js + pty 이벤트 바인딩 (풀 TUI 탭)
```

## 데이터 흐름

```
[Frontend] --invoke--> [Tauri cmd] --JSON-RPC--> [devin acp stdin]
[devin acp stdout] --notify/resp--> [acp.rs] --emit--> [Frontend]
[devin hooks] --HTTP POST--> [hooks.rs 127.0.0.1] --emit--> [Frontend 감사로그]
[devin (TUI)] --ConPTY--> [pty.rs] --pty out--> [xterm.js]
```

## 프로세스/인증 전략

- 워커당 `devin acp` 프로세스 1개 (세션 다수도 같은 프로세스 내 가능 — 검증 후 결정)
- 인증: 앱 시작 시 `devin auth status` 확인 → 미인증이면 "로그인" 버튼만 표시
  (lazy: 첫 session/new가 auth_required 반환 시 authenticate(devin-browser) 자동 호출)
- API키 수동 경로: `devin auth login --force-manual-token-flow`를 PTY에 띄워 토큰 입력,
  또는 credentials.toml 직접 기록(고급 옵션)

## 디렉터리 규약

- 앱 데이터: `%APPDATA%\devil-of-devin\` (settings.json, runs/, secrets proxy, cache/models.json)
- 프로젝트별: `<repo>\.devil\` (runs/<runId>/, worktrees/, research/) — 워커가 읽는 산출물
- hooks 주입 파일: `<repo>\.devin\hooks.v1.json` — 없으면 생성(기존 내용 보존 merge)

## 리스크/미결

- `devin acp` 세션당 1개의 활성 prompt (직렬) — 워커당 세션 1개 원칙
- session configOptions 실제 필드명 → 첫 로그인 후 `session/new` 응답으로 확정
- Windows sandbox 없음 → 자동화는 smart mode + deny 규칙 조합
- `devin acp` 미인증 상태에서도 spawn은 됨(확인됨) → 세션 생성 시점에 auth_required 기대
