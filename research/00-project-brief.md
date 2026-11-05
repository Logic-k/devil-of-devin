# Devil of Devin (데빈만 쓰는 악마) — 프로젝트 브리프

> 작성: 2026-11-06 · 상태: 리서치 완료, 구현 착수

## 목적

저렴한 Devin SWE-2 계열 모델(특히 **SWE-2 Max / SWE-2 Flash Max**)을 워커로 활용하는
**워커 오케스트레이션 데스크톱 앱**. Devin Desktop이 이미 있지만, **Devin CLI를 개조한
나만의 Devin Desktop**을 만드는 것이 목표.

- CLI 수준으로 가볍게 구동되지만, 되는 건 다 되는 프로그램
- Tauri 등으로 포장 (확정: **Tauri v2 + Rust + WebView2**)
- 사용자 시제품 `CommandCode-Desktop-app`의 아키텍처를 적극 참고

## 핵심 기능 요구사항

| # | 기능 | 설명 |
|---|------|------|
| 1 | 워커 실행 | `devin acp` 서브프로세스를 JSON-RPC(ACP)로 구동. 다수 워커 병렬 |
| 2 | 리서치 모드 (원클릭) | 웹검색/딥리서치 → AI가 이해하기 쉬운 md 파일로 프로젝트에 저장 → 이후 개발 모드가 참조 |
| 3 | 개발 모드 (원클릭) | 리서치 md → 계획 → 워커들이 구현 |
| 4 | 리서치 횟수 지정 + 이중/삼중 검토 | 반복 횟수·검토 라운드 설정 가능. 최고 아이디어 추출 후 작업 |
| 5 | 인증 최소화 + 자동화 모드 | Vercel/Supabase 위주 배포 자동화. 보안 챙기되 사용자 입력 최소 |
| 6 | 스킬 탭 | GitHub의 스킬(SKILL.md) 탐색/저장 + 로컬 컴퓨터의 기존 스킬 활용/관리 |
| 7 | 오케스트레이션 | omo, vibe-kanban 등 OSS 패턴 적극 차용 |
| 8 | 모델 | 모든 워커는 Devin SWE-2 Max (또는 SWE-2 Flash Max) |

## 작업 디렉터리

`D:\Devil_of_Devin` (ASCII-safe 경로 — 이 머신은 비ASCII 경로에서 AAPT2/Gradle 문제가 있었으므로
의도적으로 ASCII 경로 사용)

## 리서치 문서 목록

| 파일 | 내용 |
|------|------|
| `01-devin-cli.md` | Devin CLI 완전 분석 (acp/-p/hooks/config/models, 실측 결과) |
| `02-acp-protocol.md` | ACP v1 프로토콜 + Cognition 확장, SDK 현황 |
| `03-reference-implementations.md` | CommandCode-Desktop-app 역분석, vibe-kanban, omo |
| `04-research-and-orchestration.md` | 딥리서치 OSS 비교 + 리서치/개발 모드 설계 |
| `05-architecture.md` | 확정 아키텍처, 모듈 구성, 데이터 흐름 |
