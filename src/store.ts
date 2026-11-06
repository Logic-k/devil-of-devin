import { create } from "zustand";

export type View = "pipeline" | "loop" | "docs" | "workers" | "skills" | "terminal" | "settings";

export interface Msg {
  kind: "user" | "agent" | "thought" | "tool" | "plan" | "notice" | "system";
  text: string;
  tool?: any;
  plan?: any[];
  at: number;
}

export interface PermReq { id: string; agentId: string; rpcId: any; params: any; at: number }
export interface HookEvent { at: number; event: string; body: any }
export interface AgentLog { at: number; agentId: string; label: string; line: string; stderr?: boolean }
export interface Usage { totalTokens: number; inputTokens: number; outputTokens: number; prompts: number }

export interface Project { id: string; name: string; path: string; createdAt: string }

interface Store {
  info: any;
  settings: any;
  projects: Project[];
  activeProjectId: string | null;
  workers: any[];
  savedWorkers: any[];
  agents: Record<string, { label: string; exited?: boolean; cwd?: string }>;
  transcripts: Record<string, Msg[]>;
  perms: PermReq[];
  hookEvents: HookEvent[];
  agentLogs: AgentLog[];
  usage: Record<string, Usage>;
  loops: Record<string, any>;
  research: Record<string, any>;
  view: View;
  selectedWorkerId: string | null;
  selectedAgentId: string | null;
  ptyId: string | null;
  /** doc (rel path) the Docs view should open next time it renders */
  pendingDoc: string | null;
  activityOpen: boolean;
  loginState: { busy: boolean; url: string | null };
  toast: { id: number; text: string; kind: "ok" | "err" | "info" } | null;

  set: (p: Partial<Store>) => void;
  setView: (v: View) => void;
  activeProject: () => Project | null;
  pushMsg: (agentId: string, m: Omit<Msg, "at">) => void;
  appendChunk: (agentId: string, kind: Msg["kind"], text: string) => void;
  pushPerm: (p: Omit<PermReq, "id" | "at">) => void;
  popPerm: (id: string) => void;
  pushHook: (body: any) => void;
  pushLog: (l: Omit<AgentLog, "at">) => void;
  notify: (text: string, kind?: "ok" | "err" | "info") => void;
  selectWorker: (workerId: string | null, agentId?: string | null) => void;
  openDoc: (rel: string) => void;
}

let permSeq = 0;
let toastSeq = 0;

export const useStore = create<Store>((set, get) => ({
  info: null,
  settings: null,
  projects: [],
  activeProjectId: null,
  workers: [],
  savedWorkers: [],
  agents: {},
  transcripts: {},
  perms: [],
  hookEvents: [],
  agentLogs: [],
  usage: {},
  loops: {},
  research: {},
  view: "pipeline",
  selectedWorkerId: null,
  selectedAgentId: null,
  ptyId: null,
  pendingDoc: null,
  activityOpen: false,
  loginState: { busy: false, url: null },
  toast: null,

  set: (p) => set(p),
  setView: (view) => set({ view }),
  activeProject: () => {
    const { projects, activeProjectId } = get();
    return projects.find((p) => p.id === activeProjectId) ?? projects[0] ?? null;
  },
  pushMsg: (agentId, m) =>
    set((s) => ({
      transcripts: { ...s.transcripts, [agentId]: [...(s.transcripts[agentId] ?? []), { ...m, at: Date.now() }] },
    })),
  appendChunk: (agentId, kind, text) =>
    set((s) => {
      const list = [...(s.transcripts[agentId] ?? [])];
      const last = list[list.length - 1];
      if (last?.kind === kind) {
        list[list.length - 1] = { ...last, text: last.text + text };
      } else {
        list.push({ kind, text, at: Date.now() });
      }
      return { transcripts: { ...s.transcripts, [agentId]: list } };
    }),
  pushPerm: (p) => set((s) => ({ perms: [...s.perms, { ...p, id: `p${++permSeq}`, at: Date.now() }] })),
  popPerm: (id) => set((s) => ({ perms: s.perms.filter((p) => p.id !== id) })),
  pushHook: (body) =>
    set((s) => ({
      hookEvents: [...s.hookEvents.slice(-499), { at: Date.now(), event: body?.hook_event_name ?? body?.event ?? body?.type ?? "hook", body }],
    })),
  pushLog: (l) => set((s) => ({ agentLogs: [...s.agentLogs.slice(-999), { ...l, at: Date.now() }] })),
  notify: (text, kind = "info") => {
    const id = ++toastSeq;
    set({ toast: { id, text, kind } });
    setTimeout(() => { if (get().toast?.id === id) set({ toast: null }); }, 4500);
  },
  selectWorker: (workerId, agentId = null) =>
    set({ selectedWorkerId: workerId, selectedAgentId: agentId, view: "workers" }),
  openDoc: (rel) => set({ pendingDoc: rel, view: "docs" }),
}));

/** Agent id the Workers view is currently showing. */
export function activeAgentId(s: Pick<Store, "workers" | "selectedWorkerId" | "selectedAgentId">): string | null {
  if (s.selectedWorkerId) return s.workers.find((w) => w.id === s.selectedWorkerId)?.agentId ?? null;
  return s.selectedAgentId;
}
