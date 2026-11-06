import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const api = {
  // devin cli
  devinInfo: () => invoke<any>("devin_info"),
  devinAuthStatus: () => invoke<string>("devin_auth_status"),
  devinModels: () => invoke<any>("devin_models"),
  devinPrintOnce: (cwd: string, prompt: string, model?: string, mode?: string) =>
    invoke<string>("devin_print_once", { cwd, prompt, model, permissionMode: mode }),
  devinLogin: () => invoke<void>("devin_login"),
  urlOpen: (url: string) => invoke<void>("url_open", { url }),
  pickFolder: (defaultDir?: string) =>
    invoke<string | null>("pick_folder", { default: defaultDir ?? null }),

  // raw acp
  acpSpawn: (opts: {
    cwd: string; model?: string; agentType?: string; cloud?: boolean;
    autoApprove?: boolean; env?: Record<string, string>; label?: string;
  }) => invoke<any>("acp_spawn", { opts }),
  acpCall: (agent: string, method: string, params: any) =>
    invoke<any>("acp_call", { agent, method, params }),
  acpSessionNew: (agent: string, cwd: string) =>
    invoke<any>("acp_session_new", { agent, cwd, triggerAuth: true }),
  acpSessionLoad: (agent: string, session: string, cwd: string) =>
    invoke<any>("acp_session_load", { agent, session, cwd }),
  acpPrompt: (agent: string, session: string, text: string) =>
    invoke<any>("acp_prompt", { agent, session, text }),
  acpSetMode: (agent: string, session: string, mode: string) =>
    invoke<any>("acp_set_mode", { agent, session, mode }),
  acpCancel: (agent: string, session: string) =>
    invoke<void>("acp_cancel", { agent, session }),
  acpPermissionRespond: (agent: string, rpcId: any, result: any) =>
    invoke<void>("acp_permission_respond", { agent, rpcId, result }),
  acpKill: (agent: string) => invoke<void>("acp_kill", { agent }),
  acpList: () => invoke<any[]>("acp_list"),

  // workers
  workerCreate: (opts: {
    name?: string; cwd: string; model?: string; role?: string; agentType?: string;
    autoApprove?: boolean; mode?: string; env?: Record<string, string>;
    project?: string; resumeSession?: string;
  }) => invoke<any>("worker_create", { opts }),
  workerPrompt: (workerId: string, text: string) =>
    invoke<any>("worker_prompt", { workerId, text }),
  workerSetMode: (workerId: string, mode: string) =>
    invoke<void>("worker_set_mode", { workerId, mode }),
  workerList: () => invoke<any[]>("worker_list"),
  workerStop: (workerId: string) => invoke<void>("worker_stop", { workerId }),
  workerDelete: (workerId: string, deleteSession = true) =>
    invoke<void>("worker_delete", { workerId, deleteSession }),
  workerSavedList: () => invoke<any[]>("worker_saved_list"),
  workerForgetSaved: (workerId: string) => invoke<void>("worker_forget_saved", { workerId }),

  // projects / docs
  projectList: () => invoke<{ projects: any[]; active: string | null }>("project_list"),
  projectAdd: (path: string, name?: string) => invoke<any>("project_add", { path, name }),
  projectRemove: (id: string) => invoke<void>("project_remove", { id }),
  projectSetActive: (id: string) => invoke<void>("project_set_active", { id }),
  projectStages: (path: string) => invoke<any[]>("project_stages", { path }),
  projectDocs: (path: string) => invoke<any[]>("project_docs", { path }),
  projectFileRead: (path: string, rel: string) => invoke<string>("project_file_read", { path, rel }),
  projectFileWrite: (path: string, rel: string, content: string) =>
    invoke<void>("project_file_write", { path, rel, content }),
  projectGitInfo: (path: string) => invoke<any>("project_git_info", { path }),

  // harness pack
  harnessStatus: () => invoke<any>("harness_status"),
  harnessInstall: (gitRef?: string) => invoke<any>("harness_install", { gitRef: gitRef ?? null }),

  // loop engine
  loopStart: (config: any) => invoke<any>("loop_start", { config }),
  loopStatus: (id: string) => invoke<any>("loop_status", { id }),
  loopList: () => invoke<any[]>("loop_list"),
  loopStop: (id: string, kind: "stop" | "abort" = "stop") => invoke<void>("loop_stop", { id, kind }),
  loopBoard: (path: string) => invoke<any>("loop_board", { path }),
  loopTaskSetStatus: (path: string, rel: string, status: string) =>
    invoke<void>("loop_task_set_status", { path, rel, status }),

  // research
  researchStart: (spec: any) => invoke<string>("research_start", { spec }),
  researchStatus: (id: string) => invoke<any>("research_status", { id }),
  researchList: () => invoke<any[]>("research_list"),

  // pty
  ptySpawn: (opts: any) => invoke<string>("pty_spawn", { opts }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => invoke<void>("pty_kill", { id }),

  // hooks
  hookServerStart: (port?: number) => invoke<number>("hook_server_start", { port }),
  hookServerStop: () => invoke<void>("hook_server_stop"),
  hookServerStatus: () => invoke<number | null>("hook_server_status"),
  hooksInject: (cwd: string, port: number) => invoke<string>("hooks_inject", { cwd, port }),

  // secrets
  secretSet: (name: string, value: string) => invoke<void>("secret_set", { name, value }),
  secretExists: (name: string) => invoke<boolean>("secret_exists", { name }),
  secretDelete: (name: string) => invoke<void>("secret_delete", { name }),

  // skills
  skillsScanLocal: () => invoke<any[]>("skills_scan_local"),
  skillsGithubSearch: (query: string) => invoke<any>("skills_github_search", { query }),
  skillsInstall: (src: string, destRoot: string) =>
    invoke<string>("skills_install", { src, destRoot }),
  skillsSetEnabled: (dir: string, enabled: boolean) =>
    invoke<string>("skills_set_enabled", { dir, enabled }),

  // settings
  settingsGet: () => invoke<any>("settings_get"),
  settingsSet: (s: any) => invoke<void>("settings_set", { settings: s }),
};

export function onEvent<T = any>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => cb(e.payload));
}

export const ev = {
  /** global agent stream: { agentId, label, kind, payload } */
  acpEvent: (cb: (p: any) => void) => onEvent("acp://event", cb),
  workersChanged: (cb: (p: any[]) => void) => onEvent("workers://changed", cb),
  projectsChanged: (cb: (p: any) => void) => onEvent("projects://changed", cb),
  loopEvent: (cb: (p: any) => void) => onEvent("loop://event", cb),
  researchEvent: (cb: (p: any) => void) => onEvent("research://event", cb),
  hookEvent: (cb: (p: any) => void) => onEvent("hook://event", cb),
  loginDone: (cb: (p: any) => void) => onEvent("devin://login-done", cb),
  ptyOut: (id: string, cb: (p: { data: string }) => void) => onEvent(`pty://out/${id}`, cb),
  ptyExit: (id: string, cb: () => void) => onEvent(`pty://exit/${id}`, cb),
};
