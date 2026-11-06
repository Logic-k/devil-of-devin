---
status: synthesized
updated: 2026-11-06
confidence: high
topic: 업무자동화 오픈소스 (워크플로우/RPA/AI에이전트/문서처리)
---

# 업무자동화 오픈소스 리서치

## 결정

- **범용 업무자동화 1순위: n8n** — 생태계 최대(연동 1,100+), AI Agent 노드 내장, 셀프호스팅 무료. 단, 라이선스는 OSI 오픈소스가 **아닌** Sustainable Use License(fair-code). 내부 업무자동화엔 문제없으나 "고객에게 워크플로우 빌더를 제공하는 상품"엔 사용 불가.
- **진짜 오픈소스(MIT)가 필요하면: Activepieces** — 700+ pieces, 노코드 빌더가 n8n보다 쉬움. 모든 piece가 MCP 서버로 노출 → AI 에이전트/본 앱 워커에 통합 그대로 재사용 가능.
- **개발자 중심: Windmill(스크립트→워크플로우, Rust), Kestra(YAML 선언형), Temporal(내구성 코드 워크플로우)**.
- **Windows 데스크톱 RPA: OpenRPA(+OpenFlow)** — UiPath식 비주얼 디자이너+오케스트레이터를 오픈소스(MPL-2.0)로. 코드형이면 **Robot Framework + RPA Framework**(Apache-2.0).
- **UI 라이브러리 직접 제어: Python=pywinauto(BSD), .NET=FlaUI(MIT)**, 둘 다 MS UI Automation 기반. 매크로/핫키는 AutoHotkey(GPL).
- **AI 브라우저 자동화: browser-use(MIT, ~85k stars, DOM-first) vs Skyvern(AGPL-3.0, vision-first+노코드 빌더)**. 둘 다 브라우저 창 밖(OS 파일다이얼로그, 네이티브 앱)은 못 다룸 → 데스크톱 RPA와 조합 필요.
- **문서 자동화: Docling(IBM, MIT)** — PDF/Office/HTML/음성 → 구조화, OCR 내장, **MCP 서버 제공**, docling-serve로 API화. 스키마 추출은 docling-graph.
- **AI 에이전트 프레임워크: LangGraph(상태형/내구 실행)가 표준 선택지**, 멀티에이전트는 CrewAI, .NET/Azure는 Microsoft Agent Framework(AutoGen 후속).
- **도입 비추천(2026-11 기준)**: Flowise(repo 아카이브 2026-08, critical CVE 미패치), TagUI(1차 유지보수 중단), AutoGen(유지보수 모드→MAF로 이관).

## 근거

### 1. 워크플로우 자동화 / iPaaS (Zapier 대안)

| 도구 | 라이선스 | 언어 | Stars(약) | 적합 |
|---|---|---|---|---|
| n8n | Sustainable Use (fair-code) | TS | 205k | 기술팀 범용, AI Agent 노드 |
| Activepieces | MIT (ee 디렉터리 제외) | TS | 24.7k | 노코드+라이선스 자유, piece=MCP 서버 |
| Node-RED | Apache-2.0 | JS | 23.7k | IoT/이벤트 실시간 |
| Windmill | AGPLv3 | Rust 백엔드 | 18k | 코드퍼스트, 내부도구 |
| Kestra | Apache-2.0 | Java | 28.4k | 데이터/인프라 오케스트레이션 |
| Temporal | MIT | Go | 23.3k | 장기 트랜잭션, 내구성 실행 |
| Apache Airflow | Apache-2.0 | Python | 47k | 대용량 데이터 파이프라인 |
| Prefect | Apache-2.0 | Python | 23.9k | Python 네이티브 데이터 워크플로우 |
| Automatisch | AGPLv3 | TS | 14k | 단순 Zapier형 동기화 |
| Huginn | MIT | Ruby | 50k | 개인 웹모니터링 에이전트 |
| StackStorm | Apache-2.0 | Python | 6.5k | DevOps 사고대응 자동화 |
| Dify | Dify OSL(Apache+조건) | Python/TS | 152k | LLM 앱/RAG 빌더 |

- n8n: v2.42.4 stable(2026-10-07). SSO·RBAC 등 거버넌스는 Enterprise. 큐 모드 워커는 무료판에 포함.
- Activepieces: v0.92.2(2026-10-07). PostgreSQL/PGLite, 워커당 1GB. 280+ piece가 MCP 서버로 이중 노출.
- 한국어 리소스는 n8n 중심으로 가장 풍부(부분 한글화). 한국 커뮤니티 권장: 워크플로우 5개+/월 1,000회+ 실행이면 셀프호스팅이 최저비용.
- Camunda는 BPMN 엔터프라이즈 옵션(데스크톱 모델러 무료).

### 2. RPA (데스크톱/레거시 자동화)

| 도구 | 라이선스 | 특징 |
|---|---|---|
| OpenRPA + OpenFlow | MPL-2.0 | Windows 비주얼 디자이너, Node-RED 연동, UiPath 유사 아키텍처 |
| Robot Framework + RPA Framework | Apache-2.0 | 키워드 구동, Browser Lib(Playwright 기반), PDF/Excel/SAP/Windows 앱 키워드 수백 개. 오케스트레이터는 별도 필요 |
| Robocorp 라이브러리군 | Apache-2.0 | Sema4.ai 인수 후에도 유지보수 중(호스팅 오케스트레이션은 상용) |
| taskt | (C#/.NET) | Windows 전용 드래그앤드롭+레코더 |
| UI.Vision | 코어 AGPLv3, XModules 독점 EULA | 브라우저 확장+컴퓨터비전/OCR, 데스크톱 자동화는 독점 컴포넌트 필요 |
| TagUI | 오픈소스 | AI Singapore 유지보수 중단 → 커뮤니티 의존, 신규 도입 비추 |
| SikuliX / PyAutoGUI | 오픈소스 | 이미지 매칭 기반 — Citrix/원격데스크톱/캔버스 등 접근성 트리 불가 영역 |

- 오케스트레이션(스케줄/큐/자격증명/모니터링)은 대부분 별도: OpenRPA→OpenFlow, Robot Framework→Robocorp 클라우드 또는 cron/Airflow 조합.

### 3. Windows GUI 자동화 라이브러리 (RPA 스튜디오 없이 직접 코딩)

| 도구 | 라이선스 | 언어 | 비고 |
|---|---|---|---|
| pywinauto | BSD-3 | Python | Win32+UIA 두 백엔드, ~6k stars |
| FlaUI | MIT | C#/.NET | UIA2/UIA3 래퍼, ~3.2k stars |
| AutoHotkey | GPL | 자체 DSL | 핫키/매크로 표준, v2 현역 |
| WinAppDriver | 오픈소스 | - | 사실상 개발 정체 |
| AutoIt | freeware | - | **오픈소스 아님** — 주의 |

### 4. AI 브라우저 자동화 에이전트

| 도구 | 라이선스 | Stars | 방식 | 비고 |
|---|---|---|---|---|
| browser-use | MIT | ~85k | DOM-first, vision fallback | WebVoyager 89.1%, 스텝당 ~3s/토큰 |
| Skyvern | AGPL-3.0 | ~21-23k | vision-first(스크린샷) | WebVoyager 85.85%, 노코드 빌더, 2FA/비밀번호 매니저/라이브 뷰포트 내장 |
| Stagehand | MIT | ~21.8k | DOM+캐시 | Playwright 확장, 재실행 시 LLM 0 → 비용 최저 |
| Notte | SSPL | ~1.9k | 하이브리드 | 저지연(<50ms) |

- 공통 한계: CDP로 Chromium 탭만 제어 → 브라우저 창 밖(파일 다이얼로그, SAP GUI, Citrix, 네이티브 앱) 불가. 그 지점부터는 §2/§3 도구로 연결.
- AGPL(Skyvern): 사내 자동화는 무관, 제품 임베드/외부 서비스화는 법무 검토.

### 5. AI 에이전트 프레임워크 (코드 기반)

- **LangGraph** (Python/JS): 상태형 그래프, 체크포인트/인터럽트/내구 실행 — 장기 실행 업무 워크플로우의 기본 선택지.
- **CrewAI** (Python): 역할 기반 멀티에이전트 + Flows.
- **Microsoft Agent Framework** (Python/.NET/Go): AutoGen의 공식 후속, 엔터프라이즈 거버넌스/HITL. AutoGen은 유지보수 모드 → 신규 도입 비추.
- 기타: OpenAI Agents SDK(최단 경로), Pydantic AI(타입 안정), Mastra(TS), Google ADK(다언어/GCP), smolagents(로컬모델), LlamaIndex/Haystack(문서 RAG 중심).
- **계층 구분**: SDK(OpenAI Agents/Pydantic) → 오케스트레이션 런타임(LangGraph/MAF) → 멀티에이전트(CrewAI) → 검색/메모리(LlamaIndex) → 거버넌스/액션 표면. "업무자동화" 전체를 한 프레임워크가 덮지 못함.
- 비주얼 LLM 빌더: Dify, Langflow, Coze Studio. 지식베이스: MaxKB, RAGFlow, AnythingLLM. 내부업무 앱+AI: NocoBase(AI Employees).

### 6. 문서 처리 자동화

- **Docling** (IBM, MIT): PDF/DOCX/PPTX/XLSX/HTML/이미지/오디오 → 구조화 문서 모델. 레이아웃/읽기순서/표/수식 인식, RapidOCR/EasyOCR/Tesseract 등 OCR 백엔드, 로컬 실행(에어갭 가능), **MCP 서버 내장**, docling-serve API.
- **docling-graph**: 인보이스/청구서 등 스키마 기반 필드 추출(로컬 VLM/LLM 백엔드, Ollama 조합으로 완전 로컬 파이프라인 가능).
- 파이프라인 패턴: 폴더 감시 → Docling 파싱(Markdown) → 로컬 LLM 스키마 추출(JSON) → CSV/DB 적재.

### 7. 본 프로젝트(Devin 워커 오케스트레이터)와의 연결점

- MCP로 즉시 연결 가능: Activepieces pieces, Docling MCP 서버, n8n MCP 지원 → `session/new`의 `mcpServers[]`에 주입하면 Devin 워커가 업무자동화 도구를 직접 구동.
- Skyvern/browser-use 같은 에이전트형 도구는 워커의 "손"으로서 서브프로세스 운용에 적합 — 단 OS 샌드박스 없는 Windows에서 permissions.deny로 위험 범위 잠글 것.

## 출처

| 주장 | 출처 | 날짜 |
|---|---|---|
| Activepieces MIT, 24k+ stars, 700+ pieces, MCP 내장 | rerun.build/blog/best-n8n-alternatives | 2026-09 |
| n8n v2.42.4 / Activepieces v0.92.2 / Flowise 아카이브·CVE 미패치 / Dify 1.17.1 | papyrus.z29k.com 오픈소스 워크플로우 비교 | 2026-10-07 |
| n8n Sustainable Use는 OSI 오픈소스 아님, SSO/RBAC Enterprise | stackscape.org/alt/n8n | 2026 조회 |
| 도구별 라이선스·stars 표(Activepieces, Windmill, Temporal, Kestra, Prefect, Airflow, StackStorm, Huginn, Automatisch, Node-RED) | aidiscoverywire.com/open-source-workflow-automation-tools | 2026 |
| OpenRPA/Robot Framework/TagUI/UI.Vision/taskt/Skyvern RPA 비교 | aimultiple.com/open-source-rpa, itechguides.com | 2026 |
| TagUI 1차 유지보수 중단 | aimultiple.com, itechguides.com | 2026 |
| Robot Framework 내장 오케스트레이터 부재, Robocorp 플랫폼 보완 | webbrowserbot.com/rpa/open-source-rpa.php | 2026 |
| browser-use MIT~85k/Skyvern AGPL·vision-first·85.85%/Stagehand 캐시/Notte SSPL | stevengonsalvez.com browser-tools-part2, ondelva.com | 2026-08~11 |
| 브라우저 에이전트는 브라우저 창 밖 제어 불가 | mediar.ai skyvern-vs-browser-use | 2026 |
| pywinauto BSD·Win32/UIA, FlaUI MIT·UIA2/3, AutoHotkey 스크립팅 | testguild.com/automation-tools-desktop, github 저장소 | 2026-10 |
| LangGraph/CrewAI/MAF(AutoGen 후속·유지보수 모드) 등 프레임워크 지도 | delta4.io 오픈소스 AI 에이전트 프레임워크, github.com/microsoft/agent-framework | 2026 |
| 5계층 분류(SDK/런타임/멀티에이전트/검색/거버넌스) | form.io/open-source-ai-agent-framework | 2026 |
| Docling MIT·포맷·OCR·MCP·docling-serve | github.com/docling-project/docling, docling.ai | 2026-11 조회 |
| Docling+Ollama 로컬 인보이스 파이프라인 패턴 | nevizhai.com/automate-document-processing-local-ai | 2026 |
| NocoBase AI Employees / Activepieces·Dify·Langflow·Coze·MaxKB·RAGFlow 분류 | nocobase.com 블로그 | 2026 |
| n8n 한국어 부분지원·셀프호스팅 경제성·한국 커뮤니티 기준 | aihubkorea.kr, 2sync.com/ko | 2026 |

## 미결

- **라이선스 세부 검증**: n8n SUL의 정확한 허용 범위(사내 임베드 vs 고객 제공), Skyvern/ToolJet AGPL의 배포 시 의무 — 실제 도입 전 라이선스 원문 확인 필요.
- **스타 수/버전은 조회 시점 스냅샷** — 각 소스의 측정일이 상이(2026-08~10). 도입 결정 시 GitHub 재확인.
- **Activepieces "280+ vs 700+ pieces"** — 소스마다 수치 다름(공식 집계 vs MCP 노출 가능 piece 기준 추정).
- **OpenRPA 실전 성숙도**: 대규모 로봇 운영 사례·장기 유지보수 로드맵은 미검증. 파일럿 필요.
- **한국 특화 연동**(세금계산서/홈택스/국내 SaaS)은 해외 자료에 거의 없음 — 별도 검증 필요.
- **Notte, Terminator, Karavan 등 신생 도구**: 사례 부족, 실측 데이터 확보 전 채택 보류.
- WinAppDriver/AutoIt/SikuliX의 정확한 라이선스·유지보수 상태 일부 미확인(AutoIt은 freeware로 오픈소스 아님).
