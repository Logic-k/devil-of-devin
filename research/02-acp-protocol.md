# 02 — ACP (Agent Client Protocol) 정리

> 출처: agentclientprotocol.com 스펙 + `devin acp` 실측 + SDK 문서
> 목적: 앱이 `devin acp` 서브프로세스의 **클라이언트**가 되는 데 필요한 최소+확장 구현 범위

## 1. 통신 모델

JSON-RPC 2.0 over stdio (newline-delimited JSON). 양방향: 에이전트→클라이언트 요청도 있다
(권한 요청, fs 접근, 터미널). camelCase 프로퍼티, snake_case discriminator.
커스텀 확장: 메서드는 `_` 접두, 데이터는 `_meta` 필드.

## 2. 메서드 맵

### 클라이언트 → 에이전트 (우리가 호출)

| 메서드 | 필수 | 용도 |
|--------|------|------|
| `initialize` | ✓ | 버전/기능 협상. params: `protocolVersion:1`, `clientCapabilities`, `clientInfo` |
| `authenticate` | 조건부 | params: `methodId` (Devin: `"devin-browser"`). 브라우저 OAuth 트리거 |
| `session/new` | ✓ | params: `cwd`, `mcpServers[]` → `{sessionId, modes?, configOptions?...}` |
| `session/load` | 선택 | `loadSession` cap 시. 과거 대화를 `session/update`로 리플레이 |
| `session/resume` | 선택 | `sessionCapabilities.resume` cap 시. 리플레이 없이 재접속 |
| `session/prompt` | ✓ | params: `sessionId`, `prompt:[{type:"text",text}]`. 응답 `stopReason` |
| `session/cancel` | 알림 | 진행 중 턴 취소 |
| `session/set_mode` | 선택 | 모드 전환 (plan/normal/...) |
| `session/set_config_option` | 선택 | 모델/사고량/속도 등 세션 설정 변경 |
| `session/close` | 선택 | 세션 자원 해제 (idempotent) |
| `session/request_permission` 응답 | — | 에이전트 요청에 대한 응답 (아래) |

Cognition 확장(실측 `_meta`): `session/list`(+contentSearch, orderBy), `session/delete`,
`session/rename`, 추가 워크스페이스 루트, `close`에 `force` 옵션 — 상세 메서드명은
구현 시 `_meta` 키 → 실제 요청명 매핑을 런타임 탐색으로 확정.

### 에이전트 → 클라이언트 (우리가 처리/응답)

| 메서드 | 처리 |
|--------|------|
| `session/request_permission` | 툴콜 승인 UI → `{outcome:{outcome:"selected",optionId}}` |
| `fs/read_text_file` / `fs/write_text_file` | `fs` cap 광고 시 파일 대행 IO |
| `terminal/create` `output` `release` `wait_for_exit` `kill` | `terminal` cap 광고 시 가상 터미널 |
| `elicitation/create` / `elicitation/complete` | 구조화 입력 요청 (URL 플로우 포함) |

### 에이전트 → 클라이언트 알림 (`session/update`)

`update.sessionUpdate` 종류:
`user_message_chunk` / `agent_message_chunk` / `agent_thought_chunk` (content block),
`tool_call` / `tool_call_update` (kind/status/locations/content/terminal),
`plan` (TODO 리스트), `available_commands_update` (슬래시 명령 광고),
`current_mode_update`, `config_option_update`, `usage_update`(비용),
`session_info_update`/`notice`, `compaction` 등.

## 3. 인증

- `initialize` 응답 `authMethods` 나열. Devin은 `devin-browser`만 광고(실측).
- `authenticate` → 즉시 응답 + 에이전트 측이 브라우저 플로우 진행 (비동기).
- 미인증 상태에서 `session/new` → `auth_required` 에러 → 그때 authenticate 호출하면 됨
  (lazy auth: 앱 시작 시 즉시 브라우저 안 띄우고 첫 세션 생성 시점에 처리 → "인증 최소화" 요구와 부합)
- `logout` 메서드는 `agentCapabilities.auth.logout` cap 존재 시.

## 4. SDK 현황

| 라이브러리 | 버전 | 비고 |
|-----------|------|------|
| `agent-client-protocol` (Rust) | 5.x | client/agent/proxy/conductor. `process` feature로 자식프로세스 spawn. v1 stable + `unstable_protocol_v2` |
| `@agentclientprotocol/sdk` (TS) | 2.4.x | `client()`/`agent()` fluent API, /unstable /experimental/v2 엔트리포인트 |
| acp-kotlin | - | JVM |

**결정**: Rust SDK는 버전 빌더/마커 타입이 복잡하고 Cognition `_meta` 확장이 비표준이라
**얇은 자체 JSON-RPC 클라이언트**로 구현 (tokio + serde_json, ~수백 라인).
→ 메서드 커버리지를 YAML/JSON 메서드 테이블로 분리해 확장성 확보.

## 5. 구현 체크리스트 (Devil of Devin ACP 클라이언트)

- [ ] `devin acp --model swe-2-max` spawn (stdin/stdout 파이프, stderr=로그 캡처)
- [ ] `initialize` → capability 저장 (loadSession, sessionCaps, authMethods)
- [ ] `session/new`(cwd=워커 worktree) → sessionId
- [ ] `session/prompt` 텍스트+`resource_link`/`embedded` 컨텐츠
- [ ] `session/update` → 이벤트 버스 → 프론트 렌더 (메시지/툴콜/플랜/usage/모드)
- [ ] `session/request_permission` → 앱 승인 모달 (+자동승인 정책: 모드별 룰)
- [ ] `fs/*` 미광고로 시작 (에이전트 자체 fs 사용) → 필요 시 나중에 광고
- [ ] `terminal` cap: Devin은 자체 터미널 라이프사이클 확장도 광고 → 우선 미광고
- [ ] `session/list`/`resume`/`rename`/`close` Cognition 확장 메서드 탐색
- [ ] `session/set_config_option` 모델/사고량 변경 UI
- [ ] 재연결: 프로세스 죽으면 재spawn + `session/load`(또는 resume) 복구
