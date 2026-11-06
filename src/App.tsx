import { useEffect } from "react";
import { useStore } from "./store";
import { installEventBridge, refreshAll } from "./lib/events";
import { api, ev } from "./lib/ipc";
import Sidebar from "./components/Sidebar";
import PermissionModal from "./components/PermissionModal";
import ActivityPanel from "./components/ActivityPanel";
import PipelineView from "./views/PipelineView";
import LoopView from "./views/LoopView";
import DocsView from "./views/DocsView";
import WorkersView from "./views/WorkersView";
import SkillsView from "./views/SkillsView";
import TerminalView from "./views/TerminalView";
import SettingsView from "./views/SettingsView";
import UpdateBanner from "./components/UpdateBanner";
import { checkForUpdate } from "./lib/updater";

const TITLES: Record<string, string> = {
  pipeline: "파이프라인", loop: "루프", docs: "문서", workers: "워커",
  skills: "스킬", terminal: "터미널", settings: "설정",
};

export default function App() {
  const view = useStore((s) => s.view);
  const info = useStore((s) => s.info);
  const project = useStore((s) => s.activeProject());
  const activityOpen = useStore((s) => s.activityOpen);
  const perms = useStore((s) => s.perms);
  const toast = useStore((s) => s.toast);
  const loginState = useStore((s) => s.loginState);
  const set = useStore((s) => s.set);
  const pendingPerms = perms.length;

  useEffect(() => {
    installEventBridge().then(refreshAll);
    // 시작 5초 뒤, 이후 6시간마다 GitHub Release 확인 (개발 모드는 생략)
    const first = setTimeout(() => checkForUpdate(false), 5000);
    const every = setInterval(() => checkForUpdate(false), 6 * 60 * 60 * 1000);
    return () => { clearTimeout(first); clearInterval(every); };
  }, []);

  // login PTY: surface the auth URL as a button
  useEffect(() => {
    if (!loginState.busy) return;
    const un = ev.ptyOut("login", (p) => {
      try {
        const text = atob(p.data);
        const m = text.match(/https?:\/\/[^\s"'<>()]+/);
        if (m) set({ loginState: { busy: true, url: m[0] } });
      } catch {}
    });
    const t = setTimeout(() => set({ loginState: { busy: false, url: null } }), 120_000);
    return () => { un.then((f) => f()); clearTimeout(t); };
  }, [loginState.busy]);

  const startLogin = () => {
    set({ loginState: { busy: true, url: null } });
    api.devinLogin().catch((e) => { set({ loginState: { busy: false, url: null } }); useStore.getState().notify(`로그인 실패: ${e}`, "err"); });
  };

  const authed = info?.authenticated || info?.session_token;

  return (
    <div className="shell">
      <Sidebar />
      <div className="content">
        <div className="topbar">
          <div className="crumb">
            <span className="crumb-project">{project?.name ?? "프로젝트 없음"}</span>
            <span className="crumb-sep">/</span>
            <span className="crumb-view">{TITLES[view]}</span>
          </div>
          <div className="topbar-right">
            <span className={`devin-status ${authed ? "ok" : "warn"}`} title={info?.auth_detail ?? ""}>
              {info?.path
                ? `${String(info.version ?? "").replace(/^devin\s+/, "devin ").split(" (")[0]} · ${info.authenticated ? "CLI 인증" : info.session_token ? "ACP 토큰" : "미인증"}`
                : "devin CLI 없음"}
            </span>
            {info?.path && !authed && (
              <button className="small primary" disabled={loginState.busy} onClick={startLogin}>
                {loginState.busy ? "로그인 진행중…" : "로그인"}
              </button>
            )}
            {loginState.url && (
              <button className="small" onClick={() => api.urlOpen(loginState.url!)}>브라우저로 열기 ↗</button>
            )}
            <button className={`small icon-btn ${activityOpen ? "active" : ""}`} title="활동 패널"
              onClick={() => set({ activityOpen: !activityOpen })}>
              ◫ 활동{pendingPerms ? <span className="pill warn">{pendingPerms}</span> : null}
            </button>
          </div>
        </div>
        <UpdateBanner />
        <div className="body">
          <div className="main">
            {view === "pipeline" && <PipelineView />}
            {view === "loop" && <LoopView />}
            {view === "docs" && <DocsView />}
            {view === "workers" && <WorkersView />}
            {view === "skills" && <SkillsView />}
            {view === "settings" && <SettingsView />}
            {/* terminal stays mounted so xterm keeps its buffer across views */}
            <div style={{ display: view === "terminal" ? "flex" : "none", flex: 1, minHeight: 0 }}>
              <TerminalView active={view === "terminal"} />
            </div>
          </div>
          {activityOpen && <ActivityPanel />}
        </div>
      </div>
      {perms.length > 0 && <PermissionModal req={perms[0]} />}
      {toast && <div className={`toast ${toast.kind}`}>{toast.text}</div>}
    </div>
  );
}
