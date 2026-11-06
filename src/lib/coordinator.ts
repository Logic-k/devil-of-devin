import { api } from "./ipc";
import { useStore } from "../store";

/**
 * Each project has one long-lived "coordinator" worker: the conversation in
 * which the harness stages (IDEA → … → LOOP) are driven by slash-skills.
 * Returns the worker, creating it on first use.
 */
export async function getCoordinator(projectPath: string): Promise<any> {
  const st = useStore.getState();
  const existing = st.workers.find(
    (w) => w.role === "coordinator" && (w.project === projectPath || w.cwd === projectPath) && w.status !== "stopped",
  );
  if (existing) return existing;
  // a detached coordinator from a previous app run → re-attach its session
  const saved = st.savedWorkers.find(
    (w) => w.role === "coordinator" && (w.project === projectPath || w.cwd === projectPath) && w.sessionId,
  );
  const settings = st.settings ?? {};
  const w = await api.workerCreate({
    name: "coordinator",
    cwd: projectPath,
    role: "coordinator",
    project: projectPath,
    mode: settings.default_permission_mode || "smart",
    resumeSession: saved?.sessionId,
  });
  if (saved) await api.workerForgetSaved(saved.id).catch(() => {});
  useStore.setState({ workers: await api.workerList(), savedWorkers: await api.workerSavedList() });
  return w;
}

/** Send a prompt to the project's coordinator and jump to its transcript. */
export async function promptCoordinator(projectPath: string, text: string) {
  const w = await getCoordinator(projectPath);
  const st = useStore.getState();
  st.selectWorker(w.id);
  st.pushMsg(w.agentId, { kind: "user", text });
  try {
    await api.workerPrompt(w.id, text);
  } catch (e) {
    st.pushMsg(w.agentId, { kind: "system", text: `오류: ${e}` });
    throw e;
  }
}
