import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/ipc";
import { useStore } from "../store";
import { promptCoordinator } from "../lib/coordinator";
import { ModelInput } from "../components/fields";

const STAGE_HELP: Record<string, { desc: string; prompt: string; needsInput?: boolean }> = {
  idea: {
    desc: "관련 기능·운영 조건·산출물 후보를 넓게 펼친 뒤 문제·사용자·범위를 좁혀 docs/IDEA.md를 만듭니다.",
    prompt: "/idea-brainstorm {input}",
    needsInput: true,
  },
  prd: {
    desc: "요구마다 ID와 관찰 가능한 수용 기준, 자율운영 합의(권한·예산·금지·모델)를 docs/PRD.md에 적습니다. 화면 제품이면 디자인 미리보기를 고릅니다.",
    prompt: "/prd-generator docs/IDEA.md 를 바탕으로 PRD를 작성해줘. 질문이 필요하면 선택지·이유·추천안을 붙여 물어봐. {input}",
  },
  trd: {
    desc: "기존 환경을 조사해 기술 선택과 근거·검증 수단을 docs/TRD.md에 정합니다.",
    prompt: "/trd-generator docs/PRD.md 기준으로 기술 결정을 정리해줘. {input}",
  },
  architecture: {
    desc: "구성·경계·핵심 흐름·데이터/API 계약을 docs/ARCHITECTURE/에 설계합니다.",
    prompt: "/architecture-generator PRD·TRD 기준으로 구조와 흐름을 설계해줘. {input}",
  },
  plan: {
    desc: "기능 단위 태스크와 작은 phase로 나누고 plan_setup/에 PLAN·STATE·LOG를 만듭니다. 루프 보드는 이 태스크 파일을 읽습니다.",
    prompt: "/plan-generator 문서를 기능 단위 태스크와 phase로 나눠줘. 각 태스크 frontmatter에 id·status·depends_on·acceptance_criteria·verification을 반드시 넣어줘. {input}",
  },
  harness: {
    desc: "수용 기준을 실제로 검사할 수단(lint/unit/e2e/task_validation)과 로컬 실행 환경을 harness_setup/에 준비합니다.",
    prompt:
      "/harness-setup 수용 기준을 검사할 하네스와 로컬 환경을 준비해줘. harness_setup/quality_gates/ 에 lint.md · unit.md · e2e.md · task_validation.md 를 만들고, " +
      "각 파일에는 실제로 실행할 명령을 ```bash 코드 블록(또는 frontmatter의 command: 키)으로 넣어줘 — Devil of Devin 앱이 그 명령을 그대로 실행해 PASS/FAIL을 판정한다. {input}",
  },
  loop: {
    desc: "실행 방식·병렬·검증·복구·재개를 loop_setup/LOOP.md에 정합니다. 실제 Loop 실행(배정·게이트·독립 검증·STATE/LOG)은 이 앱의 루프 보드가 맡습니다.",
    prompt:
      "/loop-setup Loop 실행 방식을 loop_setup/LOOP.md 에 정리해줘. 실행 환경은 'Devil of Devin'(Others)이다: 태스크 배정·git 워크트리 격리·게이트 실행·별도 세션의 task_validation·STATE/LOG 갱신을 앱이 맡는다. " +
      "LOOP.md 작성까지만 하고 Loop 자체는 시작하지 마 — Loop 시작은 앱의 루프 보드에서 한다. {input}",
  },
};

export default function PipelineView() {
  const project = useStore((s) => s.activeProject());
  const settings = useStore((s) => s.settings);
  const notify = useStore((s) => s.notify);
  const setView = useStore((s) => s.setView);
  const researchRuns = useStore((s) => s.research);
  const [stages, setStages] = useState<any[]>([]);
  const [sel, setSel] = useState<string>("idea");
  const [input, setInput] = useState("");
  const [harness, setHarness] = useState<any>(null);
  const [git, setGit] = useState<any>(null);
  const [busy, setBusy] = useState(false);
  const [researchOpen, setResearchOpen] = useState(false);

  const refresh = async () => {
    if (!project) return;
    try { setStages(await api.projectStages(project.path)); } catch {}
    try { setGit(await api.projectGitInfo(project.path)); } catch {}
  };
  useEffect(() => {
    refresh();
    api.harnessStatus().then(setHarness).catch(() => {});
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [project?.path]);

  // auto-select the first incomplete stage
  useEffect(() => {
    const first = stages.find((s) => s.status !== "done");
    if (first && !stages.some((s) => s.id === sel && s.status !== "done")) setSel(first.id);
  }, [stages.map((s) => s.status).join()]);

  const stage = stages.find((s) => s.id === sel);
  const help = STAGE_HELP[sel];
  const doneCount = stages.filter((s) => s.status === "done").length;

  const run = async () => {
    if (!project || !stage) return;
    if (help.needsInput && !input.trim()) { notify("아이디어 설명을 입력하세요", "err"); return; }
    setBusy(true);
    try {
      const text = help.prompt.replace("{input}", input.trim()).trim();
      await promptCoordinator(project.path, text);
      setInput("");
    } catch (e) {
      notify(`${e}`, "err");
    } finally { setBusy(false); }
  };

  const openDoc = useStore((s) => s.openDoc);

  const projectResearch = useMemo(
    () => Object.values(researchRuns).filter((r: any) => r.spec?.targetDir === project?.path).sort((a: any, b: any) => (a.startedAt < b.startedAt ? 1 : -1)),
    [researchRuns, project?.path],
  );

  if (!project) {
    return (
      <div className="panel center">
        <div className="empty-hero">
          <div className="empty-mark">😈</div>
          <h2>프로젝트를 추가하세요</h2>
          <p className="dim">좌측 "프로젝트 ＋"로 폴더를 고르면 IDEA → PRD → TRD → 설계 → 계획 → 하네스 → 루프 순서로 진행할 수 있습니다.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="panel">
      {harness && !harness.complete && (
        <div className="banner warn">
          <span>하네스 스킬 팩이 설치되지 않았습니다 ({harness.installed.length}/13). 단계 진행에는 13개 스킬이 필요합니다.</span>
          <button className="small primary" onClick={() => setView("skills")}>스킬 탭에서 설치</button>
        </div>
      )}

      <div className="stepper">
        {stages.map((s, i) => (
          <button key={s.id} className={`step ${s.status} ${sel === s.id ? "selected" : ""}`} onClick={() => setSel(s.id)}>
            <span className="step-num">{s.status === "done" ? "✓" : i + 1}</span>
            <span className="step-name">{s.name}</span>
            <span className="step-skill mono">/{s.skill}</span>
          </button>
        ))}
      </div>
      <div className="row small dim" style={{ padding: "0 4px" }}>
        <span>{doneCount}/{stages.length} 단계 완료</span>
        <span>·</span>
        <span className="mono">{project.path}</span>
        {git && <span className={`badge ${git.isGit ? "ok" : "warn"}`}>{git.isGit ? `git ${git.branch ?? ""}${git.dirty ? " (변경 있음)" : ""}` : "git 아님 — 워크트리 병렬 불가"}</span>}
        <button className="tiny" style={{ marginLeft: "auto" }} onClick={() => setResearchOpen((o) => !o)}>
          {researchOpen ? "리서치 닫기" : "리서치 실행 ▸"}
        </button>
      </div>

      {researchOpen && <ResearchCard projectPath={project.path} defaultModel={settings?.default_model} />}
      {projectResearch.length > 0 && !researchOpen && (
        <div className="card compact">
          {projectResearch.slice(0, 2).map((r: any) => (
            <div key={r.id} className="row small">
              <span className={`badge ${r.done ? (r.error ? "err" : "ok") : "run"}`}>{r.phase}</span>
              <span>리서치: {r.spec.topic}</span>
              {r.artifacts?.map((a: string) => <button key={a} className="tiny" onClick={() => openDoc(a)}>{a}</button>)}
            </div>
          ))}
        </div>
      )}

      {stage && (
        <div className="grid2">
          <div className="card">
            <h3>{stage.name} 단계 <span className={`badge ${stage.status === "done" ? "ok" : stage.status === "partial" ? "warn" : ""}`}>{stage.status === "done" ? "완료" : stage.status === "partial" ? "일부" : "미진행"}</span></h3>
            <p className="dim small">{help?.desc}</p>
            <div className="artifacts">
              {stage.artifacts.map((a: any) => (
                <button key={a.path} className={`artifact ${a.exists ? "ok" : ""}`} disabled={!a.exists}
                  onClick={() => openDoc(a.path)}>
                  <span className="mono">{a.path}</span>
                  <span className="dim small">{a.exists ? a.modified : "없음"}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="card">
            <h3>이 단계 진행</h3>
            <p className="small dim">
              프로젝트의 <b>coordinator</b> 워커에 <code>/{stage.skill}</code> 스킬을 호출합니다. 대화는 워커 탭에서 이어지며,
              스킬이 질문하면 거기서 답하세요. 단계가 끝나면 산출물이 위에 표시됩니다.
            </p>
            <textarea rows={help?.needsInput ? 5 : 3} value={input} onChange={(e) => setInput(e.target.value)}
              placeholder={help?.needsInput ? "만들고 싶은 것을 설명하세요. 예: 개인용 독서 기록 웹앱 — 책 추가·메모·별점, 새로고침해도 유지" : "추가 지시 (선택)"} />
            <div className="row" style={{ marginTop: 8 }}>
              <button className="primary" disabled={busy || (harness && !harness.complete)} onClick={run}>
                {busy ? "전송 중…" : `/${stage.skill} 실행`}
              </button>
              <span className="small dim mono">{help?.prompt.replace("{input}", "").trim().slice(0, 70)}…</span>
            </div>
            {stage.id === "loop" && stage.status === "done" && (
              <div className="row" style={{ marginTop: 10 }}>
                <button onClick={() => setView("loop")}>루프 보드로 이동 →</button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ResearchCard({ projectPath, defaultModel }: { projectPath: string; defaultModel?: string }) {
  const notify = useStore((s) => s.notify);
  const research = useStore((s) => s.research);
  const [spec, setSpec] = useState({ topic: "", researchers: 3, questionsPerResearcher: 2, reviewRounds: 1, refineRounds: 1, synthesize: true, model: "" });
  const [runId, setRunId] = useState<string | null>(null);
  const run = runId ? research[runId] : null;
  const start = async () => {
    try {
      const id = await api.researchStart({ ...spec, targetDir: projectPath, model: spec.model || undefined, autoApprove: true });
      setRunId(id);
    } catch (e) { notify(`${e}`, "err"); }
  };
  const num = (k: keyof typeof spec, label: string) => (
    <label className="field">{label}
      <input type="number" min={0} max={10} value={spec[k] as number} onChange={(e) => setSpec({ ...spec, [k]: +e.target.value })} />
    </label>
  );
  return (
    <div className="card">
      <h3>리서치 파이프라인 <span className="dim small">plan → sweep → review → refine → synth → research/*.md</span></h3>
      <div className="row">
        <input className="grow" placeholder="주제 (예: Tauri v2 자동 업데이트 모범 사례)" value={spec.topic} onChange={(e) => setSpec({ ...spec, topic: e.target.value })} />
        {num("researchers", "리서처")}
        {num("questionsPerResearcher", "질문/리서처")}
        {num("reviewRounds", "검토")}
        {num("refineRounds", "보강")}
        <label className="field">모델<ModelInput value={spec.model} placeholder={defaultModel ?? "swe-2-max"} onChange={(v) => setSpec({ ...spec, model: v })} style={{ width: 160 }} /></label>
        <button className="primary" disabled={!spec.topic.trim() || (run && !run.done)} onClick={start}>시작</button>
      </div>
      {run && (
        <div className="log-view" style={{ marginTop: 8 }}>
          {run.progress.map((l: string, i: number) => <div key={i} className="mono small dim">{l}</div>)}
          {run.error && <div className="small" style={{ color: "var(--err)" }}>{run.error}</div>}
        </div>
      )}
    </div>
  );
}
