# 03 — 레퍼런스 구현체 분석

## 1. CommandCode-Desktop-app (사용자 시제품, 최우선 참고)

설치본: `D:\Command Code Desktop\cmdc-gui.exe` (7.8MB PE32+, 2025-10-22 빌드, `C:\Users\kimmi` 머신).
앱 ID `ai.commandcode.cmdcgui`. AppData에 `EBWebView` = **WebView2 = Tauri 확인**.
설명 문자열: "Desktop GUI shell for the Command Code CLI".

### 바이너리 스트링 분석으로 복원한 구조

```
crates: tauri 2.x, tauri-runtime-wry 2.12.3, wry 0.57, webview2-com 0.39.1,
        portable-pty 0.9 (ConPTY: CreatePseudoConsole), tokio 1.54,
        tauri_plugin_notification, tauri_plugin_opener
모듈:  src/acp.rs  src/pty.rs  src/accounts.rs  src/updates.rs  src/billing.rs
매니저: cmdc_gui_lib::acp::AcpManager, ::pty::PtyManager,
        ::accounts::PendingLogins, ::updates::UpdateBuild
```

**Tauri 커맨드 (역추출)**:
```
acp_spawn(cwd, profileDir, onMessage)   acp_send / acp_sendline   acp_kill
pty_spawn(args, onOutput)   pty_write   pty_resize(cols,rows)     pty_kill
read_transcript(sessionId)  read_image  dir_exists
account_login_begin  account_login_cancel  account_remove  account_billing
cli_update(checkOnly)  app_update_apply(newExe)  app_update_build
```
**프론트 이벤트**: `pty://exit`, `acp://exit`, `accounts://changed`,
`accounts://login-complete`, `accounts://login-failed`
**ACP 메서드 흔적**: `session/request_permission`, `session/load`, `(cmd acp)`
**계정 모델**: `accounts.json`, `auth.json`, `struct Account{3}` +
billing 필드 `planId planStatus monthlyCredits purchasedCredits freeCredits periodEnd windowLimits`
**기타**: `..commandcode/cache/model-catalog/v1` (모델 카탈로그 캐시),
환경 `TERM=xterm-256color COLORTERM=truecolor`, `.window-state.json` (창 상태 복원)

### 배울 점 / 개선할 점

- ✅ ACP + PTY 이중 드라이브, 계정/빌링, 자체 업데이터 — 동일 패턴 채택
- ✅ 워커별 `profileDir` 분리 — 우리는 `DEVIN_*` config dir 분리 or 동일 계정 공유로 대응
- ⚠ 개선: 워커 오케스트레이션(태스크 그래프), 리서치 모드, hooks 이벤트 수집,
  스킬 마켓 탭은 시제품에 없음 → 차별점

## 2. Vibe Kanban (BloopAI) — 오케스트레이션 앱의 교과서

github.com/BloopAI/vibe-kanban — Rust Axum + SQLx/SQLite + React/TS, **Tauri v2 래핑**
(PR #2896: Axum 서버를 in-process 백그라운드 Tokio 태스크로, 웹뷰는 127.0.0.1:랜덤포트).

핵심 패턴:
- **executor 추상화**: 에이전트별 executor 모듈 → 우리는 executor=devin acp 단일 계열이지만
  "프로필" 개념으로 확장 (모델/모드/에이전트타입 조합 = 프로필)
- **git worktree 격리**: 태스크마다 worktree+브랜치 → 병렬 워커 충돌 방지. diff 인라인 리뷰, 머지/PR
- **setup script 병렬 실행**: 워크스페이스 생성과 동시에 setup 스크립트 돌림
- **preview proxy**: dev server 프록시 + 클릭→컴포넌트 소스 점프
- **MCP 서버 내장**: 프로젝트/이슈를 MCP로 외부 노출 → Devil of Devin도
  "devil MCP"를 `devin mcp add`로 워커에 주입해 워커↔앱 통신 채널 가능
- SQLite 로컬 저장소, WS로 실시간 갱신

## 3. OmO — oh-my-openagent (code-yeongyu) — 오케스트레이션 루프

github.com/code-yeongyu/oh-my-openagent (Sustainable Use License). `omo` = senpi(pi fork) 위
멀티모델 오케스트레이션 하네스. **사용자가 언급한 "omo"가 이것.**

차용할 패턴:
- **IntentGate**: 프롬프트 키워드(`ultrawork`/`ulw`, `mass ulw`) 감지 → 모드 프롬프트 주입.
  → 우리 앱: 슬래시/버튼으로 모드 프리셋을 프롬프트에 주입하는 동일 아이디어
- **`task` 위임 + 라우팅**: 메인 에이전트가 `task(category)` 또는 `task(subagent_type)`으로 위임.
  큐레이션된 읽기전용 전문가 레인: `explore`(코드 grep), `librarian`(문서/OSS 검색),
  `plan-consultant`, `plan-reviewer`
  → 우리: `devin acp` 워커 풀을 역할 레인으로 분할 (탐색/구현/검토), 각 레인에 모델+시스템프리셋 매핑
- **`mass ulw` 의존 그래프**: 작업을 DAG로 나눠 phase별 실행
  → 우리: 태스크 그래프(depends_on)로 워커 순서 제어
- **Team mode**: 리드 + 백그라운드 멤버 children
- **검증 게이트**: "증거로 증명 후 진행" — 각 워커 완료 시 검증 워커(예: `devin acp --agent-type review`)
- **체크포인트된 목표**: 긴 작업을 증거와 함께 체크포인트 → 재시작 시 이어서
- **Code-mode tool calls**: 여러 도구호출을 하나의 프로그램으로 묶어 왕복 감소(비용절감 아이디어)
- **Monitors not polling**: 빌드/배포/파일 대기를 구독으로 — 우리: 감시 규칙(watcher) 기능

## 4. 딥리서치 OSS 비교 (리서치 모드용)

| 프로젝트 | 구조 | 특징 | 우리에게 |
|---------|------|------|---------|
| assafelovic/gpt-researcher (30k★, Apache-2) | 고정 파이프라인: planner→병렬 executor(서브질문별)→publisher | 21개 retriever, 재귀 deep 모드(breadth/depth), 출처 추적, MCP | 파이프라인 구조 차용 |
| langchain-ai/open_deep_research (12k★, MIT, archived) | LangGraph supervisor + 병렬 researcher 서브그래프 | scope 단계(사용자 명확화), 압축 모듈 | supervisor 패턴 |
| HF smolagents open deep research | CodeAgent가 Python을 작성해 브라우저 구동 | 코드행동 구조로 GAIA 55% | 코드액팅 — Devin 자체가 코드액터라 자연스러움 |
| LearningCircuit/local-deep-research (9k★) | 로컬 RAG + 웹 | 로컬 문서 결합 | 로컬 파일 리서치 옵션 |

**결정**: 외부 딥리서치 엔진을 쓰지 않고, **Devin 워커 자체가 리서처**. 구조는
gpt-researcher의 고정 파이프라인 + OmO 레인 분할을 결합:

```
[기획자 1명] → 주제 분해 → N개 리서치 질문
[리서처 N명] → 각자 web_search/web_fetch → sources + 노트 (병렬, 횟수 지정)
[검토자 R명] → 노트 교차검토 (이중/삼중 검토 횟수 지정) → 모순/공백 지적
[추가 라운드] → 검토 지적 → 보강 리서치 (최대 K라운드)
[종합자 1명] → research/<topic>/*.md 확정 (AI가 참조하기 좋은 포맷)
```

## 5. 스킬 생태계 (스킬 탭용)

- 스킬 = `SKILL.md` + 자원파일 디렉터리. Devin/Claude/Copilot/Windsurf 동일 규약.
- GitHub 소스: `devin plugins install owner/repo#path` (Devin 플러그인 = 스킬+훅+룰+MCP 묶음)
- 단일 스킬은 저장소에서 `SKILL.md` 내려받아 스킬 디렉터리에 복사로 충분
- 로컬 스캔 경로(01 문서 §5 참고) + 플러그인 캐시 읽기전용 표시
- GitHub 검색: `code search "filename:SKILL.md"` API (인증 필요) 또는 curated registry
