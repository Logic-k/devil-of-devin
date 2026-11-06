import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/ipc";
import { useStore } from "../store";
import { ModelInput } from "../components/fields";
import Markdown from "../components/Markdown";

const COLS: [string, string][] = [
  ["pending", "대기"], ["in_progress", "진행 중"], ["blocked", "차단"], ["failed", "실패"], ["completed", "완료"],
];
const GATES = ["lint", "unit", "e2e", "task_validation"];
const MODES = ["smart", "accept-edits", "bypass", "plan", "ask"];

export default function LoopView() {
  const project = useStore((s) => s.activeProject());
  const loops = useStore((s) => s.loops);
  const settings = useStore((s) => s.settings);
  const notify = useStore((s) => s.notify);
  const selectWorker = useStore((s) => s.selectWorker);
  const [board, setBoard] = useState<any>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [cfgOpen, setCfgOpen] = useState(false);
  const [showLog, setShowLog] = useState(true);
  const [cfg, setCfg] = useState<any>({
    concurrency: "", useWorktrees: true, maxAttempts: "", budgetMinutes: "", phase: "",
    implementerModel: "", validatorModel: "", mode: "", skipValidation: false, resume: false,
  });

  const run = useMemo(() => {
    const mine = Object.values(loops).filter((r: any) => r.project === project?.path);
    mine.sort((a: any, b: any) => (a.id < b.id ? 1 : -1));
    return (mine[0] as any) ?? board?.latestRun ?? null;
  }, [loops, project?.path, board]);
  const active = run && ["running", "preflight", "stopping"].includes(run.status);

  const refresh = async () => {
    if (!project) return;
    try { setBoard(await api.loopBoard(project.path)); } catch {}
  };
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, active ? 3000 : 8000);
    return () => clearInterval(t);
  }, [project?.path, active]);

  const tasks: any[] = board?.tasks ?? [];
  const phases = useMemo(() => [...new Set(tasks.map((t) => t.phase).filter(Boolean))], [tasks]);
  const merged = tasks.map((t) => {
    const rs = run?.tasks?.[t.id];
    const status = active && rs?.status ? rs.status : t.status;
    return { ...t, rs, status };
  });
  const selected = merged.find((t) => t.id === sel);

  const start = async (resume = false) => {
    if (!project) return;
    try {
      const config: any = {
        project: project.path,
        concurrency: cfg.concurrency ? +cfg.concurrency : null,
        useWorktrees: cfg.useWorktrees,
        maxAttempts: cfg.maxAttempts ? +cfg.maxAttempts : null,
        budgetMinutes: cfg.budgetMinutes ? +cfg.budgetMinutes : null,
        phase: cfg.phase || null,
        implementerModel: cfg.implementerModel || null,
        validatorModel: cfg.validatorModel || null,
        mode: cfg.mode || null,
        skipValidation: cfg.skipValidation,
        resume,
      };
      await api.loopStart(config);
      setCfgOpen(false);
      notify(resume ? "Loop 재개" : "Loop 시작", "ok");
    } catch (e) { notify(`${e}`, "err"); }
  };

  const gateDot = (t: any, g: string) => {
    const rs = t.rs;
    if (!rs) return <span key={g} className="gdot" title={g} />;
    if (g === "task_validation") {
      const v = rs.validation;
      return <span key={g} className={`gdot ${v ? (v.pass ? "pass" : "fail") : ""}`} title={`task_validation ${v ? (v.pass ? "PASS" : "FAIL") : ""}`} />;
    }
    const last = Math.max(0, ...rs.gates.map((x: any) => x.attempt));
    const r = rs.gates.filter((x: any) => x.gate === g && x.attempt === last).pop();
    return <span key={g} className={`gdot ${r ? (r.pass ? "pass" : "fail") : ""}`} title={`${g} ${r ? (r.pass ? "PASS" : "FAIL") : "-"}`} />;
  };

  if (!project) return <div className="panel center"><div className="dim">프로젝트를 선택하세요</div></div>;

  const gateDefs: any[] = board?.gates ?? [];
  const counts = Object.fromEntries(COLS.map(([k]) => [k, merged.filter((t) => t.status === k).length]));

  return (
    <div className="panel loop-panel">
      <div className="loop-head card compact">
        <div className="row">
          <span className={`badge ${active ? "run" : run?.status === "done" ? "ok" : run ? "warn" : ""}`}>
            {run ? `run ${run.id} · ${run.status}` : "실행 기록 없음"}
          </span>
          <span className="small dim">태스크 {tasks.length}개 · 완료 {counts.completed} · 실패 {counts.failed}{run ? ` · 토큰 ${run.totalTokens?.toLocaleString?.() ?? 0}` : ""}</span>
          <span className="small dim">게이트: {GATES.map((g) => {
            const d = gateDefs.find((x) => x.name === g);
            return <span key={g} className={`badge ${d ? (d.agentJudged ? "warn" : "ok") : ""}`} title={d?.command ?? (d ? "에이전트 판정" : "정의 없음")}>{g}</span>;
          })}</span>
          <div className="row" style={{ marginLeft: "auto" }}>
            {!active && <button onClick={() => setCfgOpen((o) => !o)}>{cfgOpen ? "설정 닫기" : "설정"}</button>}
            {!active && <button className="primary" disabled={!tasks.length} onClick={() => start(false)}>▶ Loop 시작</button>}
            {!active && run && run.status !== "done" && <button onClick={() => start(true)}>↻ 재개</button>}
            {active && <button onClick={() => api.loopStop(run.id, "stop")}>■ 중지 (진행 중 작업은 마무리)</button>}
            {active && <button className="danger" onClick={() => { if (confirm("워커를 즉시 종료할까요?")) api.loopStop(run.id, "abort"); }}>강제 중단</button>}
          </div>
        </div>
        {tasks.length === 0 && (
          <div className="banner" style={{ marginTop: 8 }}>
            plan_setup/ 에 태스크 파일이 없습니다. 파이프라인의 <b>계획</b> 단계(/plan-generator)를 먼저 진행하세요.
          </div>
        )}
        {cfgOpen && (
          <div className="cfg-grid">
            <label className="field">동시 워커 수<input type="number" min={1} max={8} placeholder={settings?.loop_concurrency ?? 2} value={cfg.concurrency} onChange={(e) => setCfg({ ...cfg, concurrency: e.target.value })} /></label>
            <label className="field">태스크당 최대 시도<input type="number" min={1} max={10} placeholder={settings?.loop_max_attempts ?? 3} value={cfg.maxAttempts} onChange={(e) => setCfg({ ...cfg, maxAttempts: e.target.value })} /></label>
            <label className="field">시간 예산(분)<input type="number" min={0} placeholder="제한 없음" value={cfg.budgetMinutes} onChange={(e) => setCfg({ ...cfg, budgetMinutes: e.target.value })} /></label>
            <label className="field">phase 필터
              <select value={cfg.phase} onChange={(e) => setCfg({ ...cfg, phase: e.target.value })}>
                <option value="">전체</option>
                {phases.map((p) => <option key={p} value={p}>phase {p}</option>)}
              </select>
            </label>
            <label className="field">구현 모델<ModelInput value={cfg.implementerModel} placeholder={settings?.loop_implementer_model || settings?.default_model} onChange={(v) => setCfg({ ...cfg, implementerModel: v })} /></label>
            <label className="field">검증 모델<ModelInput value={cfg.validatorModel} placeholder={settings?.loop_validator_model || settings?.default_model} onChange={(v) => setCfg({ ...cfg, validatorModel: v })} /></label>
            <label className="field">권한 모드
              <select value={cfg.mode} onChange={(e) => setCfg({ ...cfg, mode: e.target.value })}>
                <option value="">기본 ({settings?.loop_mode ?? "smart"})</option>
                {MODES.map((m) => <option key={m} value={m}>{m}</option>)}
              </select>
            </label>
            <label className="row small dim" style={{ alignSelf: "end" }}><input type="checkbox" checked={cfg.useWorktrees} onChange={(e) => setCfg({ ...cfg, useWorktrees: e.target.checked })} /> git 워크트리 격리</label>
            <label className="row small dim" style={{ alignSelf: "end" }}><input type="checkbox" checked={cfg.skipValidation} onChange={(e) => setCfg({ ...cfg, skipValidation: e.target.checked })} /> 독립 검증 생략 (비권장)</label>
          </div>
        )}
      </div>

      <div className="loop-body">
        <div className="board">
          {COLS.map(([k, label]) => (
            <div key={k} className={`col col-${k}`}>
              <div className="col-head">{label} <span className="dim">{counts[k]}</span></div>
              {merged.filter((t) => t.status === k).map((t) => (
                <div key={t.id} className={`task ${sel === t.id ? "selected" : ""}`} onClick={() => setSel(t.id)}>
                  <div className="task-id mono">{t.id}</div>
                  <div className="task-title">{t.title}</div>
                  <div className="task-meta">
                    <span className="gdots">{GATES.map((g) => gateDot(t, g))}</span>
                    {t.rs?.attempts ? <span className="dim">시도 {t.rs.attempts}{t.rs.strategySwitch ? " ⚡" : ""}</span> : null}
                    {t.dependsOn?.length ? <span className="dim" title={t.dependsOn.join(", ")}>↳ {t.dependsOn.length}</span> : null}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>

        {selected && (
          <TaskDetail task={selected} project={project.path} onClose={() => setSel(null)} onChanged={refresh}
            openAgent={(id: string) => selectWorker(null, id)} />
        )}
      </div>

      {run && (
        <div className="card compact loop-log">
          <div className="row small"><b>Loop 로그</b> <button className="tiny ghost" onClick={() => setShowLog((v) => !v)}>{showLog ? "접기" : "펼치기"}</button></div>
          {showLog && (
            <div className="log-view">
              {run.log.slice(-200).map((l: any, i: number) => <div key={i} className="mono small"><span className="dim">{l.at.slice(11)}</span> {l.msg}</div>)}
              {run.summary && run.status !== "running" && <details><summary className="small">최종 보고서</summary><Markdown text={run.summary} /></details>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function TaskDetail({ task, project, onClose, onChanged, openAgent }: any) {
  const notify = useStore((s) => s.notify);
  const agents = useStore((s) => s.agents);
  const rs = task.rs;
  const setStatus = async (st: string) => {
    try { await api.loopTaskSetStatus(project, task.relPath, st); onChanged(); notify(`${task.id} → ${st}`, "ok"); }
    catch (e) { notify(`${e}`, "err"); }
  };
  const openDoc = useStore((s) => s.openDoc);
  const liveAgent = Object.keys(agents).find((id) => !agents[id].exited && agents[id].label?.startsWith(`loop:${task.id}:`));
  return (
    <div className="task-detail">
      <div className="row">
        <span className="badge">{task.status}</span>
        <b className="mono">{task.id}</b>
        <button className="tiny ghost" style={{ marginLeft: "auto" }} onClick={onClose}>×</button>
      </div>
      <h3 style={{ margin: "6px 0" }}>{task.title}</h3>
      <div className="small dim mono">{task.relPath} <button className="tiny" onClick={() => openDoc(task.relPath)}>열기</button></div>
      {liveAgent && <button className="small primary" style={{ marginTop: 6 }} onClick={() => openAgent(liveAgent)}>실행 중인 워커 대화 보기 →</button>}

      <h4>수용 기준</h4>
      <ol className="small">{task.acceptanceCriteria.map((c: string, i: number) => <li key={i}>{c}</li>)}</ol>
      {task.dependsOn?.length > 0 && <div className="small dim">의존: {task.dependsOn.join(", ")}</div>}
      {task.verification?.length > 0 && <div className="small dim">verification: {task.verification.join(" → ")}</div>}

      {rs && (
        <>
          <h4>시도 {rs.attempts}{rs.strategySwitch ? " · 전략 전환됨" : ""}</h4>
          {rs.branch && <div className="small mono dim">브랜치 {rs.branch}{rs.worktree ? ` · ${rs.worktree}` : ""}</div>}
          {rs.note && <div className="small" style={{ marginTop: 4 }}>{rs.note}</div>}
          {rs.gates.length > 0 && (
            <>
              <h4>게이트</h4>
              {rs.gates.slice().reverse().map((g: any, i: number) => (
                <details key={i} className="gate-res">
                  <summary className="small">
                    <span className={`badge ${g.pass ? "ok" : "err"}`}>{g.pass ? "PASS" : "FAIL"}</span> {g.gate} · 시도 {g.attempt} · {g.durationMs}ms{g.agentJudged ? " · 에이전트 판정" : ""}
                  </summary>
                  <pre className="small">{g.command}</pre>
                  <pre className="small dim">{g.outputTail}</pre>
                </details>
              ))}
            </>
          )}
          {rs.validation && (
            <>
              <h4>독립 검증 <span className={`badge ${rs.validation.pass ? "ok" : "err"}`}>{rs.validation.pass ? "PASS" : "FAIL"}</span></h4>
              <button className="tiny" onClick={() => openDoc(rs.validation.reportPath)}>{rs.validation.reportPath}</button>
              <pre className="small dim">{rs.validation.summary}</pre>
            </>
          )}
          {rs.attemptNotes?.length > 0 && <div className="small dim">{rs.attemptNotes.join(" · ")}</div>}
        </>
      )}

      <h4>상태 변경</h4>
      <div className="row">
        {["pending", "blocked", "completed"].filter((s) => s !== task.status).map((s) => (
          <button key={s} className="small" onClick={() => setStatus(s)}>{s}</button>
        ))}
      </div>
    </div>
  );
}
