import { useState } from "react";
import { useStore } from "../store";

const fmt = (t: number) => new Date(t).toLocaleTimeString("ko-KR", { hour12: false });

export default function ActivityPanel() {
  const [tab, setTab] = useState<"hooks" | "logs" | "perms">("hooks");
  const hooks = useStore((s) => s.hookEvents);
  const logs = useStore((s) => s.agentLogs);
  const perms = useStore((s) => s.perms);
  const set = useStore((s) => s.set);

  return (
    <div className="activity">
      <div className="activity-head">
        <button className={tab === "hooks" ? "active" : ""} onClick={() => setTab("hooks")}>훅 {hooks.length}</button>
        <button className={tab === "logs" ? "active" : ""} onClick={() => setTab("logs")}>로그 {logs.length}</button>
        <button className={tab === "perms" ? "active" : ""} onClick={() => setTab("perms")}>권한 {perms.length}</button>
        <button className="tiny ghost" style={{ marginLeft: "auto" }} onClick={() => set({ activityOpen: false })}>×</button>
      </div>
      <div className="activity-body">
        {tab === "hooks" && (hooks.length ? [...hooks].reverse().map((h, i) => (
          <div key={i} className="act-row">
            <span className="act-time">{fmt(h.at)}</span>
            <span className="badge">{h.event}</span>
            <span className="act-text mono">
              {h.body?.tool_name ?? h.body?.toolName ?? ""} {typeof h.body?.prompt === "string" ? h.body.prompt.slice(0, 80) : ""}
              {h.body?.session_id ? ` · ${h.body.session_id}` : ""}
            </span>
          </div>
        )) : <div className="side-empty">훅 이벤트 없음 — 워커가 도구를 쓰면 여기에 표시됩니다</div>)}
        {tab === "logs" && (logs.length ? [...logs].reverse().map((l, i) => (
          <div key={i} className={`act-row ${l.stderr ? "warn" : ""}`}>
            <span className="act-time">{fmt(l.at)}</span>
            <span className="badge">{l.label}</span>
            <span className="act-text mono">{l.line.slice(0, 300)}</span>
          </div>
        )) : <div className="side-empty">로그 없음</div>)}
        {tab === "perms" && (perms.length ? perms.map((p) => (
          <div key={p.id} className="act-row">
            <span className="act-time">{fmt(p.at)}</span>
            <span className="act-text">{p.params?.toolCall?.title ?? "권한 요청"} <span className="dim">({p.agentId.slice(0, 12)})</span></span>
          </div>
        )) : <div className="side-empty">대기 중인 권한 요청 없음</div>)}
      </div>
    </div>
  );
}
