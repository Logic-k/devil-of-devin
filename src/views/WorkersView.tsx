import { useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { useStore, activeAgentId } from "../store";
import { DirInput, ModelInput } from "../components/fields";
import Markdown from "../components/Markdown";

const MODES = ["smart", "accept-edits", "bypass", "plan", "ask"];

export default function WorkersView() {
  const project = useStore((s) => s.activeProject());
  const settings = useStore((s) => s.settings);
  const info = useStore((s) => s.info);
  const workers = useStore((s) => s.workers);
  const saved = useStore((s) => s.savedWorkers);
  const agents = useStore((s) => s.agents);
  const transcripts = useStore((s) => s.transcripts);
  const usage = useStore((s) => s.usage);
  const selectedWorkerId = useStore((s) => s.selectedWorkerId);
  const selectedAgentId = useStore((s) => s.selectedAgentId);
  const selectWorker = useStore((s) => s.selectWorker);
  const pushMsg = useStore((s) => s.pushMsg);
  const notify = useStore((s) => s.notify);
  const [draft, setDraft] = useState("");
  const [showNew, setShowNew] = useState(false);
  const [form, setForm] = useState({ name: "", cwd: "", model: "", role: "general", mode: "", autoApprove: false });
  const [showThoughts, setShowThoughts] = useState(true);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => { if (project && !form.cwd) setForm((f) => ({ ...f, cwd: project.path })); }, [project?.path]);

  const selected = workers.find((w) => w.id === selectedWorkerId);
  const agentId = activeAgentId({ workers, selectedWorkerId, selectedAgentId });
  const msgs = agentId ? transcripts[agentId] ?? [] : [];
  const busy = selected?.status === "running";
  const u = agentId ? usage[agentId] : null;

  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [msgs.length, msgs[msgs.length - 1]?.text?.length, agentId]);

  const createWorker = async () => {
    try {
      const w = await api.workerCreate({
        name: form.name || undefined, cwd: form.cwd, model: form.model || undefined, role: form.role,
        agentType: form.role === "reviewer" ? "review" : undefined, autoApprove: form.autoApprove,
        mode: form.mode || undefined, project: project?.path,
      });
      selectWorker(w.id);
      setShowNew(false);
    } catch (e) { notify(`워커 생성 실패: ${e}`, "err"); }
  };

  const reattach = async (sw: any) => {
    try {
      const w = await api.workerCreate({ name: sw.name, cwd: sw.cwd, model: sw.model, role: sw.role, mode: sw.mode, project: sw.project, resumeSession: sw.sessionId });
      await api.workerForgetSaved(sw.id);
      useStore.setState({ savedWorkers: await api.workerSavedList() });
      selectWorker(w.id);
    } catch (e) { notify(`세션 재연결 실패: ${e}`, "err"); }
  };

  const send = async () => {
    const text = draft.trim();
    if (!text || !agentId) return;
    setDraft("");
    pushMsg(agentId, { kind: "user", text });
    try {
      if (selected) await api.workerPrompt(selected.id, text);
      else {
        const raw = (await api.acpList()).find((a: any) => a.id === agentId);
        const sid = raw?.sessions?.[0];
        if (!sid) throw new Error("세션 없음");
        await api.acpPrompt(agentId, sid, text);
      }
    } catch (e) { pushMsg(agentId, { kind: "system", text: `오류: ${e}` }); }
  };

  const cancel = async () => {
    if (!selected) return;
    const a = (await api.acpList()).find((x: any) => x.id === selected.agentId);
    if (a?.sessions?.[0]) api.acpCancel(selected.agentId, a.sessions[0]).catch(() => {});
  };

  const projWorkers = workers.filter((w) => !project || w.project === project.path || w.cwd === project.path);
  const otherWorkers = workers.filter((w) => !projWorkers.includes(w));
  const projSaved = saved.filter((w) => !workers.some((x) => x.sessionId === w.sessionId) && (!project || w.project === project.path || w.cwd === project.path));
  const oneshots = Object.entries(agents).filter(([id]) => id.startsWith("oneshot-"));

  return (
    <div className="workers">
      <div className="worker-list">
        <button className="primary" style={{ width: "100%" }} onClick={() => setShowNew((v) => !v)}>{showNew ? "닫기" : "＋ 새 워커"}</button>
        {showNew && (
          <div className="card compact col" style={{ marginTop: 8 }}>
            <input placeholder="이름 (선택)" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
            <DirInput placeholder="작업 디렉터리" value={form.cwd} onChange={(v) => setForm({ ...form, cwd: v })} />
            <ModelInput placeholder={`모델 (기본 ${settings?.default_model ?? "swe-2-max"})`} value={form.model} onChange={(v) => setForm({ ...form, model: v })} />
            <select value={form.role} onChange={(e) => setForm({ ...form, role: e.target.value })}>
              <option value="general">general</option>
              <option value="coordinator">coordinator (파이프라인 조정자)</option>
              <option value="implementer">implementer</option>
              <option value="researcher">researcher</option>
              <option value="reviewer">reviewer (--agent-type review, 읽기 전용)</option>
            </select>
            <select value={form.mode} onChange={(e) => setForm({ ...form, mode: e.target.value })}>
              <option value="">모드: 기본 ({settings?.default_permission_mode ?? "smart"})</option>
              {MODES.map((m) => <option key={m} value={m}>모드: {m}</option>)}
            </select>
            <label className="row small dim"><input type="checkbox" checked={form.autoApprove} onChange={(e) => setForm({ ...form, autoApprove: e.target.checked })} /> 권한 요청 자동 승인</label>
            <button className="primary" onClick={createWorker} disabled={!info?.path || !form.cwd}>생성 (devin acp)</button>
          </div>
        )}

        <div className="list-title">이 프로젝트</div>
        {projWorkers.map((w) => <WorkerRow key={w.id} w={w} sel={selectedWorkerId === w.id} onClick={() => selectWorker(w.id)} />)}
        {projWorkers.length === 0 && <div className="side-empty">워커 없음</div>}
        {projSaved.length > 0 && <div className="list-title">이전 실행의 세션</div>}
        {projSaved.map((w) => (
          <div key={w.id} className="worker-item dim">
            <div className="row"><span className="name">{w.name}</span><span className="badge">detached</span>
              <button className="tiny" style={{ marginLeft: "auto" }} onClick={() => reattach(w)}>재연결</button>
              <button className="tiny ghost" onClick={() => api.workerForgetSaved(w.id).then(async () => useStore.setState({ savedWorkers: await api.workerSavedList() }))}>×</button>
            </div>
            <div className="meta">{w.sessionId}</div>
          </div>
        ))}
        {oneshots.length > 0 && <div className="list-title">파이프라인 워커</div>}
        {oneshots.slice(-30).reverse().map(([id, a]) => (
          <div key={id} className={`worker-item ${selectedAgentId === id ? "selected" : ""} ${a.exited ? "dim" : ""}`} onClick={() => selectWorker(null, id)}>
            <div className="row"><span className={`dot ${a.exited ? "off" : "run"}`} /><span className="name mono small">{a.label}</span></div>
          </div>
        ))}
        {otherWorkers.length > 0 && <div className="list-title">다른 프로젝트</div>}
        {otherWorkers.map((w) => <WorkerRow key={w.id} w={w} sel={selectedWorkerId === w.id} onClick={() => selectWorker(w.id)} />)}
      </div>

      <div className="chat-area">
        {agentId ? (
          <>
            <div className="chat-head row small">
              <b>{selected?.name ?? agents[agentId]?.label ?? agentId}</b>
              {selected && <span className="dim mono">{selected.model}</span>}
              {selected && (
                <select className="tiny-select" value={selected.mode} onChange={(e) => api.workerSetMode(selected.id, e.target.value).catch((err) => notify(`${err}`, "err"))}>
                  {MODES.map((m) => <option key={m} value={m}>{m}</option>)}
                </select>
              )}
              {u && <span className="dim">· {u.totalTokens.toLocaleString()} tokens / {u.prompts} prompts</span>}
              <label className="row dim" style={{ marginLeft: "auto" }}><input type="checkbox" checked={showThoughts} onChange={(e) => setShowThoughts(e.target.checked)} /> 생각 표시</label>
              {selected && <button className="tiny" onClick={() => api.workerStop(selected.id)}>종료</button>}
              {selected && <button className="tiny danger" onClick={() => { if (confirm(`${selected.name} 워커와 세션을 삭제할까요?`)) api.workerDelete(selected.id).then(() => selectWorker(null)); }}>삭제</button>}
            </div>
            <div className="transcript">
              {msgs.filter((m) => showThoughts || m.kind !== "thought").map((m, i) => <MsgView key={i} m={m} />)}
              {busy && <div className="msg agent"><div className="who">agent</div><div className="bubble dim pulse">실행 중…</div></div>}
              {!msgs.length && !busy && <div className="dim small" style={{ padding: 12 }}>프롬프트를 입력하세요. 스킬은 <code>/스킬명</code> 으로 호출합니다.</div>}
              <div ref={endRef} />
            </div>
            <div className="composer">
              <textarea value={draft} placeholder="프롬프트 입력… (Ctrl+Enter 전송)"
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && e.ctrlKey) send(); }} />
              <div className="send-col">
                <button className="primary" onClick={send} disabled={busy || !draft.trim()}>{busy ? "실행중…" : "전송"}</button>
                <button onClick={cancel} disabled={!busy}>취소</button>
              </div>
            </div>
          </>
        ) : (
          <div className="center dim">좌측에서 워커를 선택하거나 새로 만드세요</div>
        )}
      </div>
    </div>
  );
}

function WorkerRow({ w, sel, onClick }: any) {
  return (
    <div className={`worker-item ${sel ? "selected" : ""}`} onClick={onClick}>
      <div className="row">
        <span className={`dot ${w.status === "running" ? "run" : w.status === "error" ? "err" : w.status === "stopped" ? "off" : "ok"}`} />
        <span className="name">{w.name}</span>
        <span className="badge" style={{ marginLeft: "auto" }}>{w.role}</span>
      </div>
      <div className="meta">{w.model} · {w.mode} · {w.status}</div>
    </div>
  );
}

function MsgView({ m }: { m: any }) {
  if (m.kind === "tool") {
    const t = m.tool ?? {};
    return (
      <div className="msg tool">
        <details className="tool-call">
          <summary><span className="tool-name">{t.kind ?? "tool"}</span> {t.title ?? m.text}{t.status && <span className="dim"> · {t.status}</span>}</summary>
          {t.rawInput && <pre className="small dim">{JSON.stringify(t.rawInput, null, 1).slice(0, 1500)}</pre>}
          {Array.isArray(t.content) && t.content.map((c: any, i: number) => c?.content?.text ? <pre key={i} className="small">{String(c.content.text).slice(0, 2000)}</pre> : null)}
        </details>
      </div>
    );
  }
  if (m.kind === "plan") {
    return (
      <div className="msg plan"><div className="who">plan</div>
        <ul className="plan-items">{(m.plan ?? []).map((e: any, j: number) => <li key={j} className={e.status}>{e.status === "completed" ? "☑" : e.status === "in_progress" ? "◐" : "☐"} {e.content}</li>)}</ul>
      </div>
    );
  }
  if (m.kind === "agent") {
    return <div className="msg agent"><div className="who">agent</div><div className="bubble"><Markdown text={m.text} /></div></div>;
  }
  return <div className={`msg ${m.kind}`}><div className="who">{m.kind}</div><div className="bubble">{m.text}</div></div>;
}
