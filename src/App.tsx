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

  const refresh = async () => {
    try { setInfo(await api.devinInfo()); } catch { setInfo(null); }
    try { setSettings(await api.settingsGet()); } catch {}
  };
  useEffect(() => { refresh(); }, []);
  useEffect(() => {
    const un = ev.loginDone(() => { setLoggingIn(false); refresh(); });
    return () => { un.then((f) => f()); };
  }, []);

  return (
    <>
      <div className="topbar">
        <span className="logo"><span className="logo-mark">😈</span> Devil of Devin</span>
        <span className={`devin-status ${info?.authenticated ? "ok" : ""}`}>
          {info?.path
            ? `devin ${info.version ?? ""} · ${info.authenticated ? "인증됨" : "미인증"}`
            : "devin CLI를 찾을 수 없음"}
        </span>
        {info?.path && !info.authenticated && (
          <button className="small primary" disabled={loggingIn}
            onClick={() => { setLoggingIn(true); api.devinLogin().catch(() => setLoggingIn(false)); }}>
            {loggingIn ? "브라우저 로그인 대기중…" : "로그인"}
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
