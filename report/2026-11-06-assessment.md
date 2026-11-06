# Devil of Devin — 현황 점검 및 리뉴얼 제안 (2026-11-06)

## 1. 현재 상태 요약

| 항목 | 상태 |
|---|---|
| 빌드 | `cargo check` 통과, `tsc -b` 통과 (오늘 재확인) |
| 코드 규모 | Rust 약 2,000줄 / React+TS 약 1,900줄 (총 3,921줄) |
| Devin CLI | 3000.11.234, ACP 세션 토큰 인증 동작 확인 (report/report.html) |
| 동작 확인된 것 | ACP spawn → initialize → authenticate → session/new → prompt 스트리밍, 훅 서버 수신, 리서치 파이프라인 1회 완주(`research/oss-business-automation.md`) |
| 미구현(설계 문서 대비) | git worktree 격리, 태스크 DAG 병렬 스케줄러, `.devil/runs` 체크포인트/재개, 비용 표시(usage), 워커 영속화 |
| 테스트 | 없음 (단위/통합/E2E 모두) |

7개 탭(워커·리서치·개발·태스크·스킬·터미널·설정)이 모두 렌더링되고 핵심 경로(워커 1개 대화)는 동작한다.
그러나 "오케스트레이터"로서의 핵심인 **여러 워커를 계획에 따라 돌리고, 검증하고, 끊겨도 이어가는** 부분은 아직 스켈레톤 수준이다.

## 2. 발견된 문제점

### 심각 (사용 중 바로 체감되는 버그)

1. **탭을 바꾸면 대화·진행 상태가 사라진다.** `App.tsx:88-94`가 활성 탭만 렌더링하므로 WorkersTab이 언마운트되며 `msgs`(`WorkersTab.tsx:19`)와 이벤트 구독이 모두 소멸한다. 다른 탭을 보는 동안 도착한 에이전트 출력은 영구 유실. ResearchTab/DevelopTab도 `runId`를 잃어 진행 로그를 다시 볼 수 없고, TerminalTab은 xterm과 `ptyId`를 잃어 **PTY 프로세스가 고아가 된다**.
2. **에이전트 프로세스가 죽으면 호출이 영원히 멈춘다.** `acp.rs:98-112` stdout 리더가 끝나도 `pending` 맵의 대기자를 정리하지 않아 `call()`(`acp.rs:275-284`)이 끝나지 않는다. `initialize`에도 타임아웃이 없어 CLI가 응답하지 않으면 워커 생성이 무한 대기.
3. **태스크 보드가 워커를 누적 생성하고 종료하지 않는다.** `TasksTab.tsx:36-58`는 태스크마다 `worker_create`로 새 `devin acp` 프로세스를 띄우고 끝나도 kill하지 않는다. "DAG 순서"라고 되어 있지만 실제로는 직렬 실행이며 의존은 제목 문자열로 매칭한다.
4. **훅 이벤트를 수집만 하고 아무도 보지 않는다.** `hooks.rs:55-58`가 `hook://event`를 emit하지만 프론트에서 구독하는 곳이 없다(`ev.hookEvent` 미사용). 타깃 프로젝트의 `.devin/hooks.v1.json`에 랜덤 포트가 박힌 항목을 매번 써넣는 부작용만 남는다.

### 중간 (설계 의도와 어긋나거나 위험한 부분)

5. **인증 오류 판정이 너무 넓다.** `acp.rs:308-311` `is_auth_error`가 메시지에 `auth`/`login`/`credential`이 포함되면 전부 인증 오류로 보고 브라우저 PKCE 로그인까지 시도한다.
6. **권한 모드 설정이 워커에 전달되지 않는다.** `Settings.default_permission_mode`는 저장만 되고 Rust 어디에서도 쓰이지 않는다. `auto_approve`는 항상 `allow_always`를 고르므로 Windows(샌드박스 없음)에서는 사실상 dangerous 모드.
7. **`$secret:` 치환이 `worker_env`에만 적용된다.** README는 MCP 서버 설정에서도 참조 가능하다고 적었지만 `secrets.rs:11`은 env만 치환하고 `mcp_servers`는 평문 그대로 `session/new`에 들어간다. `github_token`도 settings.json에 평문 저장.
8. **개발 파이프라인이 프로젝트 디렉터리를 직접 수정한다.** `develop.rs:146` 구현 워커가 cwd에서 바로 작업하며 브랜치/워크트리/커밋 보호가 없다. 리뷰 종료 판정(`develop.rs:173-181`)은 `"none"` 부분 문자열 검색이라 "nonetheless" 같은 단어에도 조기 종료한다.
9. **파이프라인 워커마다 새 프로세스를 띄운다.** `workers.rs:181` `run_oneshot_worker`는 단계마다 spawn → initialize → authenticate → session/new를 반복한다. 리서치 4×3 질문이면 프로세스 15개 이상. 동시 상한도 `researchers` 값뿐이다.
10. **워커·세션이 앱 재시작 시 사라진다.** `Worker` 레지스트리는 메모리 전용. Devin 쪽 세션은 남아 있으나 앱은 다시 연결하지 않는다.
11. **stderr를 버린다.** `acp.rs:68` `Stdio::null()` 때문에 CLI 오류 원인을 알 수 없다.

### 경미

12. 프롬프트가 전부 영어로 하드코딩되어 있어 산출물 언어·형식을 바꿀 수 없다(`research.rs`, `develop.rs`).
13. 작업 디렉터리 `D:\Devil_of_Devin`이 프론트 5곳 이상에 하드코딩(`SettingsTab.tsx:54` 훅 주입 버튼 등). "프로젝트" 개념이 없다.
14. 워커 목록에 파이프라인 내부용 oneshot 에이전트가 "acp" 항목으로 섞여 보이고 사용자가 중간에 종료할 수 있다.
15. `url_open`(`devin_cli.rs:178-187`)이 `cmd /c start`를 써서 `&`가 포함된 URL이 깨진다.
16. `usage_update`를 JSON 덤프로만 표시(`WorkersTab.tsx:83`) — 비용 가시성 목표 미달.
17. 오류를 `alert()`로 표시, `thiserror` 의존성 미사용, 설정 탭 훅 포트 표시가 자동 기동 상태를 반영하지 않음.

## 3. 디자인 현황

현재 UI는 "개발자용 대시보드" 스타일의 다크 테마로 깔끔하지만, 탭 7개가 서로 독립된 화면이라
**프로젝트 → 단계 → 작업 → 워커**라는 흐름이 보이지 않는다. 사용자는 매 탭에서 디렉터리를 다시 고르고,
리서치 결과가 개발 탭에 어떻게 이어지는지, 태스크와 워커가 어떤 관계인지 화면에서 알 수 없다.

## 4. harness_loop_full_korean 분석과 적합성

### 무엇인가

Claude Code/Codex용 **한국어 스킬 13개**(SKILL.md 묶음, v2.2.4, MIT). 코드가 아니라 지시문이다.

```
IDEA → PRD → TRD → ARCHITECTURE → PLAN → HARNESS → LOOP
docs/IDEA.md, docs/PRD.md(+자율운영 합의), docs/TRD.md, docs/ARCHITECTURE/,
plan_setup/{PLAN.md, phase-NN/…task-MM.md(YAML frontmatter), STATE.md, LOG.md},
harness_setup/{HARNESS.md, quality_gates/, scripts/}, loop_setup/LOOP.md
```

핵심 규칙:
- Loop 전에는 단계마다 사람과 합의·확인, Loop 후에는 질문 0회.
- 완료 판정은 `lint → unit → e2e → task_validation` 순서, 마지막은 **구현하지 않은 별도 세션**이 수행.
- 같은 방식 수정 3회 실패 시 전략 전환, 끝내 안 되면 정직한 미완료 보고.
- STATE.md/LOG.md로 세션이 끊겨도 재개.
- 독립 태스크는 병렬(Orca 워크트리 워커 권장, 없으면 서브에이전트/별도 CLI).

### Devin CLI와의 궁합

- Devin도 SKILL.md 기반 스킬을 슬래시 커맨드로 로드한다(`devin skills list`에서 `~\.agents\skills`의 스킬이 `/이름 [user,model]`로 확인됨). 13개 스킬은 호스트 비의존 마크다운이라 **그대로 설치해 쓸 수 있다.** 리포에 `.agents/plugins/marketplace.json`도 있어 `devin plugins install`로도 받을 가능성이 높다(확인 필요).
- "구현하지 않은 별도 세션" = 이 앱의 `run_oneshot_worker`(새 `devin acp` 프로세스 + 새 세션)와 정확히 일치한다. `--agent-type review`(읽기 전용)가 task_validation 워커로 안성맞춤이다.
- "Orca 워크트리 워커"의 역할을 이 앱이 맡으면 된다. 즉 **이 앱 = 조정자(coordinator) + 워커 풀 + 상태판**.
- 현재 리서치/개발 파이프라인(영어 하드코딩 프롬프트)은 harness의 IDEA/TRD 단계 보조와 Loop로 흡수된다.

### 주의할 점

- Devin ACP에서 슬래시 커맨드(`/prd-generator …`)를 `session/prompt` 텍스트로 보내면 실행되는지 실측 필요. 안 되면 SKILL.md 내용을 시스템 프롬프트처럼 앞에 붙여 보내는 폴백을 둔다.
- Windows에는 Devin 샌드박스가 없으므로 Loop 중 자동 승인 범위를 PRD의 `## 자율운영 합의`에 적고 앱이 그 범위를 `permissions.deny`로 강제해야 한다.
- harness의 e2e/브라우저 게이트는 프로젝트마다 다르다. 앱은 `harness_setup/quality_gates/*.md`에 적힌 명령을 **그대로 실행하는 게이트 러너**만 제공하면 된다.

**결론: 기본 워크플로우로 채택 가능하며, 오히려 현재 설계 문서(04, 05)가 바라던 것(계획 DAG, 검증, 재개)을 이미 문서 규약으로 정해 둔 것이라 구현 범위가 명확해진다.**

## 5. 리뉴얼·리디자인 제안

### 목표 구조

```
┌ 사이드바 ──────────┬ 메인 ─────────────────────────────────────────┐
│ 프로젝트 목록      │ [파이프라인]  IDEA ─ PRD ─ TRD ─ ARCH ─ PLAN ─ HARNESS ─ LOOP │
│  ├ Devil_of_Devin │   각 단계: 산출물 존재/승인 상태, "이 단계 진행" 버튼  │
│  └ …              │ [루프 보드]  plan_setup 태스크 칸반 + 게이트 4칸 상태  │
│ 워커 풀 (실행 중) │   태스크 클릭 → 담당 워커 대화 / 검증 증거 / LOG      │
│ 비용·사용량       │ [문서]  docs/*.md, STATE.md, LOG.md 마크다운 뷰어     │
│                   │ [워커]  자유 대화 (현재 워커 탭)                       │
│ 설정 · 스킬 · 터미널 (보조 메뉴)                                         │
└───────────────────┴───────────────────────────────────────────────┘
```

### 단계별 계획

| Phase | 내용 | 산출 |
|---|---|---|
| 0. 안정화 | 2장 심각 1~4 수정: 전역 상태 저장소(zustand) 도입으로 탭 전환 시 상태 유지, `pending` 정리+타임아웃, 태스크 워커 종료, 훅 이벤트 패널, 권한 모드 전달 | 현재 기능이 믿고 쓸 수 있는 상태 |
| 1. 프로젝트 + 하네스 설치 | `project.rs`(프로젝트 레지스트리 영속화), 스킬 탭에서 harness 13개 원클릭 설치(버전 고정), 파이프라인 단계 뷰(산출물 파일 존재로 상태 판정), 단계 실행 = 워커 세션에 해당 스킬 호출 | IDEA~HARNESS를 앱 안에서 대화로 진행 |
| 2. Loop 엔진 | `loop.rs`: `plan_setup` 태스크 frontmatter 파서(serde_yaml), `depends_on` 스케줄러, git worktree 격리, 게이트 러너(quality_gates 명령 실행·증거 저장), 별도 세션 task_validation(`--agent-type review`), STATE.md/LOG.md 기록, 3회 실패 전략 전환 프롬프트, 재개 | 질문 없는 자율 Loop |
| 3. 리디자인 | 위 구조로 셸 재구성, 마크다운 뷰어, 비용 패널(usage_update 집계), 알림, 한국어 프롬프트 템플릿 편집 | 완성형 데스크톱 |

### 규모 추정

Phase 0은 현 코드 수정(약 400줄 변경), Phase 1~2는 새 Rust 모듈 2~3개(약 1,500줄)와 프론트 재구성(약 2,000줄). 리서치 탭은 IDEA/TRD 단계의 "리서치 실행" 버튼으로 흡수한다.

## 6. 결정이 필요한 것

1. 리뉴얼 범위: Phase 0만 먼저(안정화) vs Phase 0~3 전체 진행.
2. 기존 리서치 탭 유지 여부(별도 탭으로 남길지, 단계 안으로 흡수할지).
3. Loop 중 자동 승인 기본 범위(smart + deny 목록으로 시작 권장).
4. 테스트 기준: 앱 자체도 harness 규약(lint/unit/e2e)을 따르게 할지.
