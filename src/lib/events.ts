import { api, ev } from "./ipc";
import { useStore } from "../store";

/** Translate one ACP session/update into transcript entries. */
function applyUpdate(agentId: string, p: any) {
  const s = useStore.getState();
  const u = p?.update ?? {};
  switch (u.sessionUpdate) {
    case "user_message_chunk":
      break; // appended optimistically on send
    case "agent_message_chunk":
      s.appendChunk(agentId, "agent", u.content?.text ?? "");
      break;
    case "agent_thought_chunk":
      s.appendChunk(agentId, "thought", u.content?.text ?? "");
      break;
    case "tool_call":
      s.pushMsg(agentId, { kind: "tool", text: u.title ?? u.toolCallId ?? "tool", tool: u });
      break;
    case "tool_call_update":
      // fold status updates into the last tool entry when it is the same call
      useStore.setState((st) => {
        const list = [...(st.transcripts[agentId] ?? [])];
        for (let i = list.length - 1; i >= 0 && i >= list.length - 6; i--) {
          const m = list[i];
          if (m.kind === "tool" && m.tool?.toolCallId && m.tool.toolCallId === u.toolCallId) {
            list[i] = { ...m, tool: { ...m.tool, ...u, title: u.title ?? m.tool.title } };
            return { transcripts: { ...st.transcripts, [agentId]: list } };
          }
        }
        list.push({ kind: "tool", text: `↳ ${u.title ?? u.status ?? ""}`, tool: u, at: Date.now() });
        return { transcripts: { ...st.transcripts, [agentId]: list } };
      });
      break;
    case "plan":
      s.pushMsg(agentId, { kind: "plan", text: "Plan", plan: u.entries ?? [] });
      break;
    case "current_mode_update":
      s.pushMsg(agentId, { kind: "notice", text: `모드 → ${u.currentModeId ?? u.modeId ?? u.currentMode ?? "?"}` });
      break;
    case "usage_update":
    case "available_commands_update":
    case "config_option_update":
    case "session_info_update":
      break;
    default:
      if (u.sessionUpdate && !String(u.sessionUpdate).startsWith("_")) {
        s.pushMsg(agentId, { kind: "notice", text: `[${u.sessionUpdate}]` });
      }
  }
}

let installed = false;

/** Subscribe once (App mount) to every backend stream and feed the store. */
export async function installEventBridge() {
  if (installed) return;
  installed = true;
  const S = () => useStore.getState();

  await ev.acpEvent((e: any) => {
    const { agentId, label, kind, payload } = e;
    useStore.setState((st) =>
      st.agents[agentId] && st.agents[agentId].label === label
        ? {}
        : { agents: { ...st.agents, [agentId]: { ...(st.agents[agentId] ?? {}), label } } });
    switch (kind) {
      case "update":
        applyUpdate(agentId, payload);
        break;
      case "permission":
        S().pushPerm({ agentId, rpcId: payload.rpcId, params: payload.params });
        break;
      case "elicitation":
        // answer elicitations with cancel — the loop must never wait on a human
        api.acpPermissionRespond(agentId, payload.rpcId, { action: "cancel" }).catch(() => {});
        S().pushMsg(agentId, { kind: "notice", text: "elicitation 요청을 자동 취소했습니다" });
        break;
      case "auto-permission":
        S().pushMsg(agentId, {
          kind: "notice",
          text: `자동 승인: ${payload?.params?.toolCall?.title ?? ""} → ${payload?.picked ?? ""}`,
        });
        break;
      case "exit":
        useStore.setState((st) => ({ agents: { ...st.agents, [agentId]: { ...(st.agents[agentId] ?? { label }), exited: true } } }));
        S().pushMsg(agentId, { kind: "system", text: `— 에이전트 종료 (code ${payload?.code ?? "?"}) —` });
        break;
      case "log":
        S().pushLog({ agentId, label, line: payload?.line ?? "", stderr: !!payload?.stderr });
        break;
      case "usage":
        useStore.setState((st) => ({ usage: { ...st.usage, [agentId]: payload } }));
        break;
    }
  });
  await ev.workersChanged((list) => useStore.setState({ workers: list }));
  await ev.projectsChanged((p) =>
    useStore.setState({ projects: p.projects ?? [], activeProjectId: p.active ?? null }));
  await ev.loopEvent((p) => {
    if (p?.run) useStore.setState((st) => ({ loops: { ...st.loops, [p.run.id]: p.run } }));
  });
  await ev.researchEvent((p) => {
    if (p?.run) useStore.setState((st) => ({ research: { ...st.research, [p.run.id]: p.run } }));
  });
  await ev.hookEvent((body) => S().pushHook(body));
  await ev.loginDone(() => {
    useStore.setState({ loginState: { busy: false, url: null } });
    refreshInfo();
  });
}

export async function refreshInfo() {
  try { useStore.setState({ info: await api.devinInfo() }); } catch { useStore.setState({ info: null }); }
  try { useStore.setState({ settings: await api.settingsGet() }); } catch {}
}

export async function refreshAll() {
  await refreshInfo();
  try {
    const p = await api.projectList();
    useStore.setState({ projects: p.projects ?? [], activeProjectId: p.active ?? null });
  } catch {}
  try { useStore.setState({ workers: await api.workerList() }); } catch {}
  try { useStore.setState({ savedWorkers: await api.workerSavedList() }); } catch {}
  try {
    const loops = await api.loopList();
    useStore.setState({ loops: Object.fromEntries(loops.map((r: any) => [r.id, r])) });
  } catch {}
  try {
    const rs = await api.researchList();
    useStore.setState({ research: Object.fromEntries(rs.map((r: any) => [r.id, r])) });
  } catch {}
}
