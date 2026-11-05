# 04 — 리서치 모드 & 오케스트레이션 설계

## 1. 리서치 모드 (원클릭 파이프라인)

사용자 요구: "리서치 횟수 지정 + 이중/삼중 검토 + 최고 아이디어 추출 후 작업 착수.
결과는 AI가 참고하기 쉬운 md 파일".

### 파이프라인 정의 (`research.spec.json` / UI 설정)

```json
{
  "topic": "Tauri v2 auto-updater best practices",
  "targetDir": "D:/Devil_of_Devin",
  "researchers": 4,            // 병렬 리서치 워커 수 (=리서치 횟수)
  "questionsPerResearcher": 3, // 워커당 탐구 질문 수
  "reviewRounds": 2,           // 검토 라운드 (이중/삼중 검토)
  "refineRounds": 1,           // 검토 지적 → 보강 리서치 반복
  "synthesize": true,          // 최종 종합 md 생성
  "model": "swe-2-max"
}
```

### 단계

```
1. PLAN      플래너 워커(1): 주제 → N개 리서치 질문 (research/INDEX.md에 기록)
2. SWEEP     리서처 N: 각자 질문 배정, web_search/web_fetch/문서 수집
             → research/raw/<q>-<i>.md (주장+출처 URL+날짜 표 형식)
3. REVIEW    검토자 R: raw 노트 교차검토 → 모순/미검증/공백 리스트
             → research/review/round-<k>.md
4. REFINE    검토 지적사항 → 추가 리서치 (K회 반복)
5. SYNTH     종합자(1): 검증된 내용만 종합 → research/<topic-slug>.md
             "AI 참조용" 포맷: 결정사항/근거/출처표/미결사항
```

- 각 워커 = 독립 `devin acp` 세션, cwd=targetDir
- 산출물은 파일로 커밋 → 다음 단계 워커가 파일로 읽음 (컨텍스트 전달 = 파일 기반, 세션 간 격리 유지)
- 실패 워커 재시도 + 검토 라운드에서 출처 없는 주장 제거

### "AI가 이해하기 쉬운 md" 규약

- 파일당 단일 주제, 상단에 frontmatter (status, updated, confidence)
- `## 결정` `## 근거` `## 출처` `## 미결` 고정 섹션
- INDEX.md가 목차 역할 → 개발 모드 프롬프트가 INDEX만 읽고 필요 문서를 지목

## 2. 개발 모드 (원클릭)

```
[리서치 md 읽기] → PLAN 워커가 구현 계획 (작업 DAG)
  → IMPLEMENT 워커들 (git worktree 격리, 병렬/순차)
  → 각 완료 시 REVIEW 워커 (--agent-type review 가능) + 빌드/테스트 검증
  → 통과 시 메인 브랜치 머지 제안 → 사용자 승인
```

## 3. 워커 오케스트레이션 엔진 (OmO/vibe-kanban 차용)

### 워커 모델

```
WorkerProfile = {
  name, model(default swe-2-max), mode(normal/plan),
  role: planner|researcher|implementer|reviewer|custom,
  systemPreset?: 프롬프트 프리셋 (IntentGate와 동일 개념)
}
Worker = { id, profile, status, acpSessionId?, worktree?, taskId }
Task  = { id, title, prompt, dependsOn[], assigneeLane, status, artifacts[] }
```

### 실행 규칙

- 태스크 DAG: `dependsOn` 해소되면 큐잉 → 빈 워커 슬롯에 배정 (레인별 concurrency)
- 작업 디렉터리 충돌 방지: `git worktree add .devil/worktrees/<task>` (git repo일 때)
- 완료 정의: 워커 stopReason=end_turn + 검증 조건 (지정 명령 성공 / 리뷰어 approve)
- 체크포인트: 각 태스크 결과를 `.devil/runs/<run>/tasks.json`에 기록 → 재개 가능

### OmO식 모드 프리셋 (프롬프트 주입)

- `ulw` 류: "코드 읽기→계획→증거 검증→실제 표면에서 확인" 지시문 프리셋
- 레인 프리셋: explore=읽기전용 빠른탐색, reviewer=비판적 검증만

## 4. 자동화 모드 (Vercel/Supabase)

목표: 배포/DB 관련 워커 작업을 최소 입력으로.

- **연결 경로**: MCP 서버로 주입 (`devin mcp add`와 동일 형식을 `session/new`의
  `mcpServers[]`에 직접 전달 가능 — ACP 표준 필드!)
  - Vercel MCP / `vercel` CLI 토큰 env, Supabase MCP (`@supabase/mcp-server-supabase`)
  - 토큰은 앱 시크릿 저장소(Windows Credential Manager / 암호화 파일)에 보관,
    워커 spawn 시 env/mcp로 주입 → 프롬프트/로그에 키 안 남김
- **자동화 룰**: "deploy" 카테고리 태스크는 검증 후 자동 `vercel deploy` 같은
  허용리스트 명령 실행. `--permission-mode smart`(fast 모델 안전판정) + `permissions.deny`로 위험군 잠금.

## 5. 스킬 탭 설계 (사용자 추가 요구)

- **로컬 스킬 탭**: 스캔 경로 —
  `%APPDATA%\devin\skills`, `%APPDATA%\cognition\skills`, `~\.agents\skills`,
  `%APPDATA%\devin\cli\plugins\cache\**\skills`, 열린 프로젝트의 `.devin/.cognition/.agents/skills`
  → 이름/설명/트리거 표시, 활성화 토글(디렉터리 이동), SKILL.md 뷰어
- **GitHub 스킬 탭**: 검색(키워드→GitHub code/repo search) + 큐레이션 소스
  + URL 붙여넣기 → 설치 대상(글로벌/프로젝트) 선택 → 다운로드+배치
  + `devin plugins install <src>` 옵션 (묶음일 때)
- **저장소**: 즐겨찾는 스킬 repo 목록을 앱 DB에 저장 → 재확인/업데이트

## 6. 비용 가시성

- ACP `usage_update` 알림 + `/session-stats` 등가 정보 → 워커별 크레딧 표시
- `devin models list --format json` 캐시 → 모델 드롭다운
