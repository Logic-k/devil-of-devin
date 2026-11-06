import { useEffect, useState } from "react";
import { api, ev } from "./lib/ipc";
import WorkersTab from "./tabs/WorkersTab";
import ResearchTab from "./tabs/ResearchTab";
import DevelopTab from "./tabs/DevelopTab";
import TasksTab from "./tabs/TasksTab";
import SkillsTab from "./tabs/SkillsTab";
import TerminalTab from "./tabs/TerminalTab";
import SettingsTab from "./tabs/SettingsTab";

type Tab = "workers" | "research" | "develop" | "tasks" | "skills" | "terminal" | "settings";

const TABS: [Tab, string][] = [
  ["workers", "워커"],
  ["research", "리서치"],
  ["develop", "개발"],
  ["tasks", "태스크"],
  ["skills", "스킬"],
  ["terminal", "터미널"],
  ["settings", "설정"],
];

export default function App() {
  const [tab, setTab] = useState<Tab>("workers");
  const [info, setInfo] = useState<any>(null);
  const [settings, setSettings] = useState<any>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const [loginUrl, setLoginUrl] = useState<string | null>(null);

  const refresh = async () => {
    try { setInfo(await api.devinInfo()); } catch { setInfo(null); }
    try { setSettings(await api.settingsGet()); } catch {}
  };
  useEffect(() => { refresh(); }, []);
  useEffect(() => {
    const un = ev.loginDone(() => { setLoggingIn(false); setLoginUrl(null); refresh(); });
    return () => { un.then((f) => f()); };
  }, []);

  // watch the login PTY output for a URL to surface as a clickable button
  useEffect(() => {
    if (!loggingIn) return;
    const un = ev.ptyOut("login", (p) => {
      try {
        const text = atob(p.data);
        const m = text.match(/https?:\/\/[^\s"'<>()]+/);
        if (m) setLoginUrl(m[0]);
      } catch {}
    });
    const t = setTimeout(() => setLoggingIn(false), 120_000);
    return () => { un.then((f) => f()); clearTimeout(t); };
  }, [loggingIn]);

  const startLogin = () => {
    setLoggingIn(true);
    setLoginUrl(null);
    api.devinLogin().catch((e) => { setLoggingIn(false); alert(`로그인 실패: ${e}`); });
  };

  return (
    <>
      <div className="topbar">
        <span className="logo"><span className="logo-mark">😈</span> Devil of Devin</span>
        <span className={`devin-status ${(info?.authenticated || info?.session_token) ? "ok" : ""}`}>
          {info?.path
            ? `devin ${info.version ?? ""} · ${info.authenticated ? "CLI 인증됨" : info.session_token ? "ACP 토큰 사용 가능" : "미인증"}`
            : "devin CLI를 찾을 수 없음"}
        </span>
        {info?.path && !info.authenticated && !info.session_token && (
          <button className="small primary" disabled={loggingIn} onClick={startLogin}>
            {loggingIn ? "로그인 진행중…" : "로그인"}
          </button>
        )}
        {loginUrl && (
          <button className="small" onClick={() => api.urlOpen(loginUrl)}>
            브라우저로 열기 ↗
          </button>
        )}
        <div className="tabs">
          {TABS.map(([k, label]) => (
            <button key={k} className={tab === k ? "active" : ""} onClick={() => setTab(k)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="main">
        {tab === "workers" && <WorkersTab devinInfo={info} />}
        {tab === "research" && <ResearchTab defaultModel={settings?.default_model} />}
        {tab === "develop" && <DevelopTab defaultModel={settings?.default_model} />}
        {tab === "tasks" && <TasksTab />}
        {tab === "skills" && <SkillsTab />}
        {tab === "terminal" && <TerminalTab devinPath={info?.path} />}
        {tab === "settings" && <SettingsTab settings={settings} onSaved={refresh} />}
      </div>
    </>
  );
}
