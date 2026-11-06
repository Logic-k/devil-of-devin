import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const api = {
  devinInfo: () => invoke<any>("devin_info"),
  devinAuthStatus: () => invoke<string>("devin_auth_status"),
  devinModels: () => invoke<any>("devin_models"),
  devinPrintOnce: (cwd: string, prompt: string, model?: string, mode?: string) =>
    invoke<string>("devin_print_once", { cwd, prompt, model, permissionMode: mode }),

  acpSpawn: (opts: {
    cwd: string; model?: string; agentType?: string; cloud?: boolean;
    autoApprove?: boolean; env?: Record<string, string>;
  }) => invoke<any>("acp_spawn", { opts }),
  acpCall: (agent: string, method: string, params: any) =>
    invoke<any>("acp_call", { agent, method, params }),
  acpSessionNew: (agent: string, cwd: string) =>
    invoke<any>("acp_session_new", { agent, cwd, triggerAuth: true }),
  acpSessionLoad: (agent: string, session: string, cwd: string) =>
    invoke<any>("acp_session_load", { agent, session, cwd }),
  acpPrompt: (agent: string, session: string, text: string) =>
    invoke<any>("acp_prompt", { agent, session, text }),
  acpCancel: (agent: string, session: string) =>
    invoke<void>("acp_cancel", { agent, session }),
  acpPermissionRespond: (agent: string, rpcId: any, result: any) =>
    invoke<void>("acp_permission_respond", { agent, rpcId, result }),
  acpKill: (agent: string) => invoke<void>("acp_kill", { agent }),
  acpList: () => invoke<any>("acp_list"),

  workerCreate: (opts: any) => invoke<any>("worker_create", { opts }),
  workerPrompt: (workerId: string, text: string) =>
    invoke<any>("worker_prompt", { workerId, text }),
  workerList: () => invoke<any[]>("worker_list"),
  workerStop: (workerId: string) => invoke<void>("worker_stop", { workerId }),
  workerDelete: (workerId: string) => invoke<void>("worker_delete", { workerId }),
  pickFolder: (defaultDir?: string) =>
    invoke<string | null>("pick_folder", { default: defaultDir ?? null }),

  researchStart: (spec: any) => invoke<string>("research_start", { spec }),
  researchStatus: (id: string) => invoke<any>("research_status", { id }),

  developStart: (spec: any) => invoke<string>("develop_start", { spec }),
  developStatus: (id: string) => invoke<any>("develop_status", { id }),

  devinLogin: () => invoke<void>("devin_login"),
  urlOpen: (url: string) => invoke<void>("url_open", { url }),

  tasksLoad: (dir: string) => invoke<any[]>("tasks_load", { dir }),
  tasksSave: (dir: string, tasks: any[]) => invoke<void>("tasks_save", { dir, tasks }),

  ptySpawn: (opts: any) => invoke<string>("pty_spawn", { opts }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => invoke<void>("pty_kill", { id }),

  hookServerStart: (port?: number) => invoke<number>("hook_server_start", { port }),
  hookServerStop: () => invoke<void>("hook_server_stop"),
  hooksInject: (cwd: string, port: number) =>
    invoke<string>("hooks_inject", { cwd, port }),

  secretSet: (name: string, value: string) =>
    invoke<void>("secret_set", { name, value }),
  secretExists: (name: string) => invoke<boolean>("secret_exists", { name }),
  secretDelete: (name: string) => invoke<void>("secret_delete", { name }),

  skillsScanLocal: () => invoke<any[]>("skills_scan_local"),
  skillsGithubSearch: (query: string) => invoke<any>("skills_github_search", { query }),
  skillsInstall: (src: string, destRoot: string) =>
    invoke<string>("skills_install", { src, destRoot }),
  skillsSetEnabled: (dir: string, enabled: boolean) =>
    invoke<string>("skills_set_enabled", { dir, enabled }),

  settingsGet: () => invoke<any>("settings_get"),
  settingsSet: (s: any) => invoke<void>("settings_set", { settings: s }),
};

export function onEvent<T = any>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => cb(e.payload));
}

export const ev = {
  acpUpdate: (agent: string, cb: (p: any) => void) =>
    onEvent(`acp://update/${agent}`, cb),
  acpPermission: (agent: string, cb: (p: any) => void) =>
    onEvent(`acp://permission/${agent}`, cb),
  acpAutoPerm: (agent: string, cb: (p: any) => void) =>
    onEvent(`acp://auto-permission/${agent}`, cb),
  acpExit: (agent: string, cb: (p: any) => void) =>
    onEvent(`acp://exit/${agent}`, cb),
  acpNotify: (agent: string, cb: (p: any) => void) =>
    onEvent(`acp://notify/${agent}`, cb),
  ptyOut: (id: string, cb: (p: { data: string }) => void) =>
    onEvent(`pty://out/${id}`, cb),
  ptyExit: (id: string, cb: () => void) =>
    onEvent(`pty://exit/${id}`, cb),
  hookEvent: (cb: (p: any) => void) => onEvent("hook://event", cb),
  researchProgress: (id: string, cb: (p: any) => void) =>
    onEvent(`research://progress/${id}`, cb),
  developProgress: (id: string, cb: (p: any) => void) =>
    onEvent(`develop://progress/${id}`, cb),
  loginDone: (cb: (p: any) => void) => onEvent("devin://login-done", cb),
};
