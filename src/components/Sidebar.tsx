import { useMemo } from "react";
import { useStore, type View } from "../store";
import { api } from "../lib/ipc";

const NAV: [View, string, string][] = [
  ["pipeline", "파이프라인", "⟶"],
  ["loop", "루프", "↻"],
  ["docs", "문서", "▤"],
  ["workers", "워커", "◉"],
  ["skills", "스킬", "✦"],
  ["terminal", "터미널", ">_"],
  ["settings", "설정", "⚙"],
];

export default function Sidebar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const projects = useStore((s) => s.projects);
  const activeId = useStore((s) => s.activeProjectId);
  const workers = useStore((s) => s.workers);
  const agents = useStore((s) => s.agents);
  const usage = useStore((s) => s.usage);
  const loops = useStore((s) => s.loops);
  const selectWorker = useStore((s) => s.selectWorker);
  const selectedWorkerId = useStore((s) => s.selectedWorkerId);
  const notify = useStore((s) => s.notify);
  const project = useStore((s) => s.activeProject());

  const addProject = async () => {
    const p = await api.pickFolder(project?.path);
    if (!p) return;
    try { await api.projectAdd(p); } catch (e) { notify(`${e}`, "err"); }
  };

  const totals = useMemo(() => {
    let t = 0, p = 0;
    for (const u of Object.values(usage)) { t += u?.totalTokens ?? 0; p += u?.prompts ?? 0; }
    return { tokens: t, prompts: p };
  }, [usage]);

  const runningLoop = Object.values(loops).find((r: any) => r.project === project?.path && ["running", "preflight", "stopping"].includes(r.status));
  const liveOneshots = Object.entries(agents).filter(([id, a]) => !a.exited && id.startsWith("oneshot-"));
  const projWorkers = workers.filter((w) => !project || w.project === project.path || w.cwd === project.path);

  return (
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">😈</span><span>Devil of Devin</span></div>

      <div className="side-section">
        <div className="side-title">프로젝트 <button className="tiny" onClick={addProject} title="폴더 추가">＋</button></div>
        {projects.length === 0 && <div className="side-empty">폴더를 추가하세요</div>}
        {projects.map((p) => (
          <div key={p.id} className={`side-item ${p.id === activeId ? "active" : ""}`}
            title={p.path} onClick={() => api.projectSetActive(p.id)}>
            <span className="side-item-name">{p.name}</span>
            {runningLoop && p.path === project?.path && <span className="dot run" title="Loop 실행 중" />}
            <button className="tiny ghost" title="목록에서 제거"
              onClick={(e) => { e.stopPropagation(); if (confirm(`${p.name} 을(를) 목록에서 제거할까요? (파일은 그대로)`)) api.projectRemove(p.id); }}>×</button>
          </div>
        ))}
      </div>

      <nav className="side-nav">
        {NAV.map(([k, label, icon]) => (
          <button key={k} className={`nav-item ${view === k ? "active" : ""}`} onClick={() => setView(k)}>
            <span className="nav-icon">{icon}</span>{label}
            {k === "loop" && runningLoop && <span className="dot run" />}
          </button>
        ))}
      </nav>

      <div className="side-section grow">
        <div className="side-title">워커 풀 <span className="dim">{projWorkers.length + liveOneshots.length}</span></div>
        {projWorkers.map((w) => (
          <div key={w.id} className={`side-item ${selectedWorkerId === w.id && view === "workers" ? "active" : ""}`}
            onClick={() => selectWorker(w.id)} title={`${w.model} · ${w.mode}`}>
            <span className={`dot ${w.status === "running" ? "run" : w.status === "error" ? "err" : w.status === "stopped" ? "off" : "ok"}`} />
            <span className="side-item-name">{w.name}</span>
            <span className="side-item-meta">{w.role}</span>
          </div>
        ))}
        {liveOneshots.map(([id, a]) => (
          <div key={id} className={`side-item ${useStore.getState().selectedAgentId === id && view === "workers" ? "active" : ""}`}
            onClick={() => selectWorker(null, id)} title={id}>
            <span className="dot run" />
            <span className="side-item-name mono">{a.label}</span>
          </div>
        ))}
        {projWorkers.length + liveOneshots.length === 0 && <div className="side-empty">실행 중인 워커 없음</div>}
      </div>

      <div className="side-footer">
        <div className="side-kv"><span>프롬프트</span><b>{totals.prompts}</b></div>
        <div className="side-kv"><span>토큰</span><b>{totals.tokens.toLocaleString()}</b></div>
      </div>
    </aside>
  );
}
