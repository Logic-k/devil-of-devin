# Devil of Devin 😈

**Devin CLI를 감싸는 개인용 데스크톱 오케스트레이터.** Tauri 2 + React + Rust.
[harness_loop_full_korean](https://github.com/GangJaeYu/harness_loop_full_korean)의
"IDEA → PRD → TRD → 설계 → 계획 → 하네스 → 루프" 흐름을 기본 워크플로우로 삼아,
문서 단계는 Devin 세션과 대화로 진행하고 **Loop(구현·게이트·독립 검증·통합)는 앱이 자율로 돌립니다.**

## 요구사항

- Windows 10/11, WebView2
- [Devin CLI](https://devin.ai) (`%LOCALAPPDATA%\devin\cli\bin\devin.exe`, 로그인되어 있을 것)
- git, Node 18+, Rust 1.77+ (개발 시)

## 실행 / 빌드

```bash
npm install
npm run tauri dev      # 개발
npm run tauri build    # 배포용 .exe / .msi → src-tauri/target/release/bundle/
```

## 화면 구성

| 화면 | 기능 |
|---|---|
| **파이프라인** | 7단계 진행 상태(산출물 파일 존재로 판정). 단계 카드에서 `/idea-brainstorm` … `/loop-setup` 스킬을 프로젝트의 coordinator 워커에 호출. 리서치 파이프라인(plan→sweep→review→synth → `research/*.md`)도 여기서 실행 |
| **루프** | `plan_setup/` 태스크 칸반(대기·진행·차단·실패·완료) + 게이트 4칸 상태. Loop 시작/중지/재개, 동시 워커·예산·phase 필터·모델 설정, 태스크별 게이트 출력·검증 보고·시도 기록 |
| **문서** | docs/ · plan_setup/ · harness_setup/ · loop_setup/ · research/ · .devil/loop/ 마크다운 뷰어/편집 |
| **워커** | `devin acp` 세션과 자유 대화(스트리밍·도구 호출·플랜·사용량), 세션 모드 전환, 이전 실행 세션 재연결, 파이프라인 워커 대화 열람 |
| **스킬** | 하네스 스킬 팩 원클릭 설치(버전 고정 가능), 로컬 스킬 스캔/토글, GitHub 검색·설치 |
| **터미널** | ConPTY로 `devin` TUI 직접 사용 |
| **설정** | 기본 모델·세션 모드, Loop 엔진(동시 수·시도·타임아웃·모델), 훅 서버, MCP/ENV 주입, 시크릿 보관소 |

상단 **활동** 패널: 워커 훅 이벤트(PreToolUse/PostToolUse/Stop), CLI 경고 로그, 대기 중인 권한 요청.

## 사용 순서

1. 좌측 **프로젝트 ＋** 로 폴더 추가 (git 저장소 권장 — 워크트리 병렬에 필요).
2. **스킬** 탭에서 하네스 스킬 팩 설치 (`~/.agents/skills/`에 13개).
3. **파이프라인**에서 아이디어 입력 → `/idea-brainstorm` 실행. 워커 탭의 coordinator 대화에서 질문에 답하며
   PRD → TRD → 설계 → 계획 → 하네스 → 루프 순으로 진행. 각 단계 산출물은 **문서** 탭에서 바로 확인·편집.
4. **루프** 탭에서 ▶ Loop 시작. 앱이 의존이 풀린 태스크를 워크트리에 배정하고, 구현 워커 →
   `lint → unit → e2e` → **구현하지 않은 별도 세션의 task_validation** → `--no-ff` 병합을 반복합니다.
   같은 방식 3회 실패 시 전략 전환, 끝내 실패하면 증거를 남기고 미완료로 보고합니다.
   진행 상황은 `plan_setup/STATE.md`(재개 기록) · `LOG.md`(이력) · `.devil/loop/`(증거)에 남습니다.

## 하네스 호환 규약

- 태스크 파일: frontmatter `id / status / depends_on / acceptance_criteria / verification`.
- 게이트 정의 `harness_setup/quality_gates/{lint,unit,e2e,task_validation}.md`:
  frontmatter `command:` 또는 첫 ```bash / ```powershell / ```cmd 블록을 앱이 그대로 실행합니다.
  명령이 없는 게이트는 review 에이전트가 정의를 읽고 판정합니다.
- 검증 워커는 `devin acp --agent-type review`(읽기 전용+셸)로 새 프로세스·새 세션에서 돌며
  응답 마지막 줄 `RESULT: PASS|FAIL` 로 판정합니다.

## 핵심 아키텍처

```
React(zustand) ──invoke──> Rust commands ──stdio JSON-RPC──> devin.exe acp (워커당 1 프로세스)
     <──acp://event / loop://event / workers://changed ... (전역 이벤트 스트림, 한 번만 구독)
```

- `acp.rs` ACP 클라이언트 (세션 토큰 자동 인증, 모드 전환, 사용량, 프로세스 종료 시 대기 호출 정리)
- `looprun.rs` Loop 엔진 (태스크/게이트 파서, 워크트리, 게이트 러너, 독립 검증, STATE/LOG)
- `project.rs` / `harness.rs` 프로젝트·단계·문서, 스킬 팩 설치
- `workers.rs` 워커 레지스트리(앱 재시작 후 세션 재연결) + 일회성 비상속 세션 러너

## 인증 (실측)

`devin auth status`가 "Not logged in"이라도 `credentials.toml`의 세션 토큰으로 ACP 인증이 됩니다
(앱이 자동 수행). 상단 배지가 "ACP 토큰"이면 정상. 로그인 버튼은 토큰 갱신용 폴백입니다.

## MCP 자동화 (Vercel/Supabase)

설정 탭의 **+ supabase MCP / + vercel MCP**로 `mcp_servers`에 추가하면 모든 워커 세션에 주입됩니다.
토큰은 시크릿 보관소에 저장하고 `$secret:NAME`으로 참조하세요 (env·MCP headers 모두 치환).
