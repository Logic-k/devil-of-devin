import { useEffect, useRef, useState } from "react";
import { api, ev } from "../lib/ipc";
import { DirInput, ModelInput } from "../components/fields";

interface Msg {
  kind: "user" | "agent" | "thought" | "tool" | "plan" | "notice" | "system";
  text: string;
  tool?: any;
  plan?: any[];
}

interface PermReq { rpcId: any; params: any; agentId: string }

export default function WorkersTab({ devinInfo }: { devinInfo: any }) {
  const [workers, setWorkers] = useState<any[]>([]);
  const [rawAgents, setRawAgents] = useState<any[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [selAgent, setSelAgent] = useState<string | null>(null); // for raw agents
  const [msgs, setMsgs] = useState<Record<string, Msg[]>>({});
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [perm, setPerm] = useState<PermReq | null>(null);
  const [form, setForm] = useState({
    name: "", cwd: "D:\\Devil_of_Devin", model: "",
    role: "general", autoApprove: false,
  });
  const [loadSid, setLoadSid] = useState("");
  const [loadCwd, setLoadCwd] = useState("D:\\Devil_of_Devin");
  const [loadAgent, setLoadAgent] = useState<string | null>(null);
  const [loadList, setLoadList] = useState<any[]>([]);
  const transcriptEnd = useRef<HTMLDivElement>(null);

  const selected = workers.find((w) => w.id === sel);
  const selectedRaw = rawAgents.find((a) => a.id === selAgent);

  const refresh = () => {
    api.workerList().then(setWorkers).catch(() => {});
    api.acpList().then(setRawAgents).catch(() => {});
  };
  useEffect(() => { refresh(); }, []);

  // subscribe to events for the selected worker or raw agent
  const activeAgent = selected?.agentId ?? selectedRaw?.id ?? null;
  const activeSession = selected?.sessionId ?? selectedRaw?.sessions?.[0] ?? null;
  const pushMsg = (agent: string, m: Msg) =>
    setMsgs((old) => ({ ...old, [agent]: [...(old[agent] ?? []), m] }));

  const appendChunk = (agent: string, kind: Msg["kind"], text: string) =>
    setMsgs((old) => {
      const list = [...(old[agent] ?? [])];
      const last = list[list.length - 1];
      if (last?.kind === kind) {
        list[list.length - 1] = { ...last, text: last.text + text };
        return { ...old, [agent]: list };
      }
      return { ...old, [agent]: [...list, { kind, text }] };
    });

  const handleUpdate = (agent: string) => (p: any) => {
    const push = (m: Msg) => pushMsg(agent, m);
    const u = p?.update ?? {};
    switch (u.sessionUpdate) {
      case "user_message_chunk":
        break; // user text is already appended optimistically in send()
      case "agent_message_chunk":
        appendChunk(agent, "agent", u.content?.text ?? "");
        break;
      case "agent_thought_chunk":
        appendChunk(agent, "thought", u.content?.text ?? "");
        break;
      case "tool_call":
        push({ kind: "tool", text: u.title ?? u.toolCallId ?? "tool", tool: u });
        break;
      case "tool_call_update":
        push({ kind: "tool", text: `↳ ${u.title ?? u.status ?? ""}`, tool: u });
        break;
      case "plan":
        push({ kind: "plan", text: "Plan", plan: u.entries ?? [] });
        break;
      case "current_mode_update":
        push({ kind: "notice", text: `mode → ${u.modeId ?? u.currentMode ?? "?"}` });
        break;
      case "usage_update":
        push({ kind: "notice", text: `usage: ${JSON.stringify(u)}` });
        break;
      default:
        if (u.sessionUpdate) push({ kind: "notice", text: `[${u.sessionUpdate}]` });
    }
  };

  useEffect(() => {
    if (!activeAgent) return;
    const agent = activeAgent;
    const unU = ev.acpUpdate(agent, handleUpdate(agent));
    const unP = ev.acpPermission(agent, (p: any) =>
      setPerm({ rpcId: p.rpcId, params: p.params, agentId: agent }));
    const unE = ev.acpExit(agent, () => pushMsg(agent, { kind: "system", text: "— agent exited —" }));
    return () => { unU.then((f) => f()); unP.then((f) => f()); unE.then((f) => f()); };
  }, [activeAgent]);

  useEffect(() => { transcriptEnd.current?.scrollIntoView({ behavior: "smooth" }); },
    [msgs, activeAgent]);

  const createWorker = async () => {
    try {
      const w = await api.workerCreate({
        name: form.name || undefined,
        cwd: form.cwd,
        model: form.model || undefined,
        role: form.role,
        agentType: form.role === "reviewer" ? "review" : undefined,
        autoApprove: form.autoApprove,
      });
      await refresh();
      setSel(w.id);
    } catch (e) { alert(`워커 생성 실패: ${e}`); }
  };

  const fetchSessions = async () => {
    try {
      let agentId = loadAgent;
      if (!agentId) {
        const spawn = await api.acpSpawn({
          cwd: loadCwd, model: form.model || undefined, autoApprove: form.autoApprove,
        });
        agentId = spawn.agentId as string;
        setLoadAgent(agentId);
      }
      const r = await api.acpCall(agentId, "session/list", { cwd: loadCwd });
      const list = Array.isArray(r) ? r : r?.sessions ?? [];
      setLoadList(list);
      if (!list.length) alert("세션이 없거나 인증이 필요합니다.");
      await refresh();
    } catch (e) { alert(`세션 목록 실패: ${e}`); }
  };

  const loadSession = async (sidOverride?: string) => {
    const sid = (sidOverride ?? loadSid).trim();
    if (!sid) return;
    try {
      let agentId = loadAgent;
      if (!agentId) {
        const spawn = await api.acpSpawn({
          cwd: loadCwd, model: form.model || undefined, autoApprove: form.autoApprove,
        });
        agentId = spawn.agentId as string;
        setLoadAgent(agentId);
      }
      // subscribe BEFORE loading so the replayed history is captured
      const unU = ev.acpUpdate(agentId, handleUpdate(agentId));
      const unP = ev.acpPermission(agentId, (p: any) =>
        setPerm({ rpcId: p.rpcId, params: p.params, agentId }));
      const unE = ev.acpExit(agentId, () =>
        pushMsg(agentId, { kind: "system", text: "— agent exited —" }));
      try {
        await api.acpSessionLoad(agentId, sid, loadCwd);
      } finally {
        unU.then((f) => f()); unP.then((f) => f()); unE.then((f) => f());
      }
      pushMsg(agentId, { kind: "system", text: `loaded session ${sid}` });
      await refresh();
      setSel(null);
      setSelAgent(agentId);
    } catch (e) { alert(`세션 로드 실패: ${e}`); }
  };

  const send = async () => {
    const text = draft.trim();
    if (!text || busy) return;
    if (!activeAgent || !activeSession) {
      pushMsg(activeAgent ?? "none", {
        kind: "system",
        text: "세션이 준비되지 않았습니다. 좌측에서 워커를 선택하세요.",
      });
      return;
    }
    const agent = activeAgent;
    setDraft("");
    setBusy(true);
    // devin acp does not echo user_message_chunk — append the user msg ourselves
    pushMsg(agent, { kind: "user", text });
    try {
      if (selected) {
        await api.workerPrompt(selected.id, text);
      } else {
        await api.acpPrompt(agent, activeSession, text);
      }
    } catch (e) {
      pushMsg(agent, { kind: "system", text: `error: ${e}` });
    } finally { setBusy(false); }
  };

  const respondPerm = async (outcome: string, optionId?: string) => {
    if (!perm) return;
    const result = outcome === "cancelled"
      ? { outcome: { outcome: "cancelled" } }
      : { outcome: { outcome: "selected", optionId } };
    try { await api.acpPermissionRespond(perm.agentId, perm.rpcId, result); } catch {}
    setPerm(null);
  };

  const curMsgs = activeAgent ? msgs[activeAgent] ?? [] : [];

  return (
    <div className="workers-layout">
      <div className="worker-list">
        <div className="card">
          <h3>새 워커</h3>
          <div className="col">
            <input placeholder="이름 (선택)" value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })} />
            <DirInput placeholder="작업 디렉터리" value={form.cwd}
              onChange={(v) => setForm({ ...form, cwd: v })} />
            <ModelInput placeholder="모델 (비우면 기본값: swe-2-max)" value={form.model}
              onChange={(v) => setForm({ ...form, model: v })} />
            <select value={form.role} onChange={(e) => setForm({ ...form, role: e.target.value })}>
              <option value="general">general (기본 에이전트)</option>
              <option value="planner">planner (계획 레인)</option>
              <option value="researcher">researcher (리서치 레인)</option>
              <option value="implementer">implementer (구현 레인)</option>
              <option value="reviewer">reviewer (--agent-type review)</option>
            </select>
            <label className="row small dim">
              <input type="checkbox" checked={form.autoApprove}
                onChange={(e) => setForm({ ...form, autoApprove: e.target.checked })} />
              권한 자동 승인
            </label>
            <button className="primary" onClick={createWorker} disabled={!devinInfo?.path}>
              워커 생성 (devin acp)
            </button>
          </div>
        </div>
        {workers.map((w) => (
          <div key={w.id} className={`worker-item ${sel === w.id ? "selected" : ""}`}
            onClick={() => { setSel(w.id); setSelAgent(null); }}>
            <div className="row">
              <span className="name">{w.name}</span>
              <span className={`badge ${w.status === "running" ? "run" : w.status === "error" ? "err" : "ok"}`}>
                {w.status}
              </span>
              <button className="danger small" style={{ marginLeft: "auto" }}
                onClick={(e) => { e.stopPropagation(); api.workerStop(w.id).then(refresh); }}>
                종료
              </button>
              <button className="danger small" title="워커 및 세션 삭제"
                onClick={(e) => {
                  e.stopPropagation();
                  if (confirm(`워커 ${w.name}을(를) 삭제할까요? (세션도 삭제됩니다)`))
                    api.workerDelete(w.id).then(refresh);
                }}>
                삭제
              </button>
            </div>
            <div className="meta">{w.model} · {w.role}</div>
            <div className="meta">{w.cwd}</div>
          </div>
        ))}
        {rawAgents
          .filter((a) => !workers.some((w) => w.agentId === a.id))
          .map((a) => (
            <div key={a.id} className={`worker-item ${selAgent === a.id ? "selected" : ""}`}
              onClick={() => { setSelAgent(a.id); setSel(null); }}>
              <div className="row">
                <span className="name">{a.id.slice(0, 14)}…</span>
                <span className="badge">acp</span>
                <button className="danger small" style={{ marginLeft: "auto" }}
                  onClick={(e) => { e.stopPropagation(); api.acpKill(a.id).then(refresh); }}>
                  종료
                </button>
              </div>
              <div className="meta">{a.model ?? "default"} · sessions: {a.sessions?.length ?? 0}</div>
              <div className="meta">{a.cwd}</div>
            </div>
          ))}
        {!workers.length && !rawAgents.length &&
          <div className="dim small" style={{ padding: 8 }}>워커 없음</div>}

        <div className="card" style={{ marginTop: 12 }}>
          <h3>기존 세션 불러오기</h3>
          <div className="col">
            <DirInput placeholder="작업 디렉터리" value={loadCwd}
              onChange={setLoadCwd} />
            <div className="row">
              <input className="grow" placeholder="세션 ID" value={loadSid}
                onChange={(e) => setLoadSid(e.target.value)} />
              <button onClick={() => loadSession()} disabled={!loadSid.trim() || !devinInfo?.path}>
                session/load
              </button>
            </div>
            <button className="small" onClick={fetchSessions} disabled={!devinInfo?.path}>
              세션 목록 불러오기
            </button>
            {loadList.map((s: any) => (
              <div key={s.sessionId ?? s.id} className="skill-row">
                <div className="grow">
                  <strong className="small">{s.title ?? s.sessionId ?? s.id}</strong>
                  <div className="dim small">
                    {s.sessionId ?? s.id}{s.updatedAt ? ` · ${s.updatedAt}` : ""}
                  </div>
                </div>
                <button className="small" onClick={() => {
                  const sid = s.sessionId ?? s.id;
                  setLoadSid(sid);
                  loadSession(sid);
                }}>로드</button>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="chat-area">
        {activeAgent ? (
          <>
            <div className="transcript">
              {curMsgs.map((m, i) => (
                <div key={i} className={`msg ${m.kind}`}>
                  <div className="who">{m.kind}</div>
                  {m.kind === "tool" ? (
                    <div className="tool-call">
                      <span className="tool-name">{m.tool?.kind ?? "tool"}</span> {m.text}
                      {m.tool?.status && <span className="dim"> ({m.tool.status})</span>}
                      {m.tool?.rawInput && (
                        <pre className="small dim">{JSON.stringify(m.tool.rawInput).slice(0, 400)}</pre>
                      )}
                    </div>
                  ) : m.kind === "plan" ? (
                    <ul className="plan-items">
                      {(m.plan ?? []).map((e: any, j: number) => (
                        <li key={j} className={e.status}>{e.status === "completed" ? "☑" : "☐"} {e.content}</li>
                      ))}
                    </ul>
                  ) : (
                    <div className="bubble">{m.text}</div>
                  )}
                </div>
              ))}
              {busy && (
                <div className="msg agent">
                  <div className="who">agent</div>
                  <div className="bubble dim">실행 중…</div>
                </div>
              )}
              {!curMsgs.length && !busy && (
                <div className="dim small" style={{ padding: 12 }}>
                  {activeSession ? `세션 ${activeSession} — 프롬프트를 입력하세요.` : "세션 없음"}
                </div>
              )}
              <div ref={transcriptEnd} />
            </div>
            <div className="composer">
              <textarea value={draft} placeholder="프롬프트 입력… (Ctrl+Enter 전송)"
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && e.ctrlKey) send(); }} />
              <div className="send-col">
                <button className="primary" onClick={send} disabled={busy || !draft.trim()}>
                  {busy ? "실행중…" : "전송"}
                </button>
                <button onClick={() => activeSession && api.acpCancel(activeAgent, activeSession)}>취소</button>
              </div>
            </div>
          </>
        ) : (
          <div className="panel"><div className="dim">좌측에서 워커를 생성/선택하세요.</div></div>
        )}
      </div>

      {perm && (
        <div className="perm-overlay" onClick={() => respondPerm("cancelled")}>
          <div className="perm-modal" onClick={(e) => e.stopPropagation()}>
            <h3>권한 요청</h3>
            <div className="small dim">{perm.params?.toolCall?.title ?? ""}</div>
            <pre className="small" style={{ maxHeight: 200, overflow: "auto" }}>
              {JSON.stringify(perm.params?.toolCall ?? perm.params, null, 2).slice(0, 2000)}
            </pre>
            <div className="perm-opts">
              {(perm.params?.options ?? []).map((o: any) => (
                <button key={o.optionId ?? o.id}
                  className={o.kind?.startsWith("allow") ? "primary" : ""}
                  onClick={() => respondPerm("selected", o.optionId ?? o.id)}>
                  {o.name ?? o.kind ?? o.optionId}
                </button>
              ))}
              <button className="danger" onClick={() => respondPerm("cancelled")}>거부</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
