# Devil of Devin 😈

**Devin CLI를 감싸는 개인용 데스크톱 워커/오케스트레이션 앱.**
Tauri 2 + React + Rust. `devin acp`(Agent Client Protocol over stdio)를
구동해 CLI급 가벼움으로 워커 스폰, 리서치 파이프라인, 스킬 관리까지 처리한다.

## 요구사항

- Windows 10/11
- [Devin CLI](https://devin.ai) — `%LOCALAPPDATA%\devin\cli\bin\devin.exe`
  (자동 탐지되며, 설정에서 경로 지정 가능)
- Rust 1.77+ (`cargo`), Node 18+ (`npm`)
- WebView2 (Windows 11 기본 탑재)

## 실행 / 빌드

```bash
npm install
npm run tauri dev      # 개발 모드 (핫리로드)
npm run tauri build    # 배포용 .exe / .msi 생성 → src-tauri/target/release/bundle/
```

검증만 할 때: `cargo check`(src-tauri) + `npm run build`(프론트).

## 탭 구성

| 탭 | 기능 |
|---|---|
| **워커** | `devin acp` 에이전트 스폰 → `session/new` 자동 생성. 이름/cwd/모델/역할/자동승인 설정, 프롬프트 스트리밍(메시지·생각·툴콜·플랜), 권한 요청 승인/거절, `session/load`로 이전 세션 이어받기(리플레이 포함), 다중 워커 병렬 관리 |
| **리서치** | 브리프 입력 → 플래너→리서처 N회→비평가 M회 파이프라인(oneshot 워커) → `research/*.md` 산출물. 반복 횟수·모델 설정 가능 |
| **개발** | 프로젝트 cwd 선택 → 리서치 md 컨텍스트 주입 후 개발 워커 실행, 진행 상태 스트리밍 |
| **태스크** | 칸반식 보드 — 워커/파이프라인 작업 상태 추적, `tasks.json` 영속화 |
| **스킬** | 로컬 스킬 스캔(`%APPDATA%\devin\skills`, `.agents\skills`, 플러그인 캐시 등), GitHub 검색·설치(`owner/repo#경로`), 활성/비활성 토글 — 설치 즉시 모든 워커가 사용 가능 |
| **터미널** | ConPTY로 `devin` TUI 직접 사용, `devin -p` 원샷 프롬프트 |
| **설정** | devin 경로, 기본 모델(`swe-2-max`), 권한 모드, 훅 서버, MCP 프리셋(Supabase/Vercel), 워커 env, 스킬 디렉터리 |

## 핵심 아키텍처

```
React UI ──invoke──> Rust commands ──stdio JSON-RPC──> devin.exe acp
        <──emit──── ACP updates (session/update 등) <──┘
```

- `src-tauri/src/acp.rs` — ACP 클라이언트: 프로세스 스폰, `initialize` 핸드셰이크,
  요청/알림 라우팅, 권한 요청 자동응답(`auto_approve`) 또는 UI 전달.
- `src-tauri/src/workers.rs` — 워커 = ACP 에이전트 1 + 세션 1.
  `run_oneshot_worker`는 리서치/개발 파이프라인의 일회성 워커.
- `src-tauri/src/hooks.rs` — `127.0.0.1` 로컬 수신 서버. 앱 시작 시 자동 기동,
  워커 생성 시 해당 cwd의 `.devin/hooks.v1.json`에 curl POST 훅을 자동 주입
  (기존 훅 보존) → 워커의 PreToolUse/Stop 등 라이프사이클 이벤트가 UI로 유입.
- `src-tauri/src/skills.rs` — 스킬 스캔/설치/토글. GitHub code search(토큰
  없으면 repo search로 폴백) + `git clone --depth 1` 설치.
- 설정: `%APPDATA%\devil-of-devin\settings.json` — 모델, 권한 모드, 훅 포트,
  `mcp_servers`(모든 워커 `session/new`에 주입), `worker_env`, GitHub 토큰 등.

## 인증

ACP는 호스트가 자격증명을 제공해야 한다(로컬 `credentials.toml` 미사용).
앱은 `initialize` 응답의 `authMethods`를 보고 브라우저 로그인(`devin-browser`)
플로우를 트리거한다 — 상단 바의 **로그인** 버튼 사용. 미인증 상태에서는
워커 생성이 실패할 수 있으니 먼저 로그인.

## MCP 자동화 (Vercel/Supabase)

설정 탭의 **+ Supabase MCP / + Vercel MCP** 버튼으로 `mcp_servers`에
엔드포인트를 추가하면 모든 워커 세션에 주입된다. 토큰이 필요한 서버는
`worker_env`(KEY=VALUE)에 넣고 서버 설정에서 참조. 토큰 값에는
`$secret:NAME`을 써서 Windows 자격증명 관리자(시크릿 보관소 카드)에서 읽게 할 수 있다.

유용한 환경변수 (worker_env에 그대로 사용 가능):
- `DEVIN_MODEL` — 세션 기본 모델
- `DEVIN_REFUSAL_FALLBACK=swe-2-max,swe-2-flash` — 제공자 거부 시 폴백 모델
- `DEVIN_PERMISSION_MODE` — 권한 모드

## 리서치 산출물

`D:\Devil_of_Devin\research\` — 프로젝트 브리프, Devin CLI 조사, ACP 프로토콜,
레퍼런스(CommandCode/omo/Vibe Kanban/GPT Researcher 등), 오케스트레이션 기법,
아키텍처 문서. 개발 모드는 이 디렉터리의 md를 컨텍스트로 읽는다.

## 참고

- 시제품: `D:\Command Code Desktop\cmdc-gui.exe` (Tauri/ACP 데스크톱 셸 —
  바이너리만 존재, 구조만 참고)
- 리서치/개발 파이프라인은 omo(Oh My OpenAgent)의 planner/executor 분리와
  GPT Researcher의 다중 검토 패턴을 차용
