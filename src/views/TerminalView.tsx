import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { api, ev } from "../lib/ipc";
import { useStore } from "../store";
import "@xterm/xterm/css/xterm.css";

function b64decode(b: string): Uint8Array {
  const bin = atob(b);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export default function TerminalView({ active }: { active: boolean }) {
  const info = useStore((s) => s.info);
  const project = useStore((s) => s.activeProject());
  const ptyId = useStore((s) => s.ptyId);
  const set = useStore((s) => s.set);
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [cmdline, setCmdline] = useState("");

  useEffect(() => {
    if (!hostRef.current || termRef.current) return;
    const term = new Terminal({
      fontFamily: "Cascadia Code, Consolas, monospace", fontSize: 13,
      theme: { background: "#0b0b12", foreground: "#e6e9f0", cursor: "#8b6cff" }, convertEol: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(hostRef.current);
    fit.fit();
    termRef.current = term; fitRef.current = fit;
    const onResize = () => {
      fit.fit();
      const id = useStore.getState().ptyId;
      if (id) api.ptyResize(id, term.cols, term.rows).catch(() => {});
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    if (active) setTimeout(() => {
      fitRef.current?.fit();
      const id = useStore.getState().ptyId;
      if (id && termRef.current) api.ptyResize(id, termRef.current.cols, termRef.current.rows).catch(() => {});
    }, 30);
  }, [active]);

  useEffect(() => {
    if (!ptyId) return;
    const un1 = ev.ptyOut(ptyId, (p) => termRef.current?.write(b64decode(p.data)));
    const un2 = ev.ptyExit(ptyId, () => { termRef.current?.writeln("\r\n[process exited]"); set({ ptyId: null }); });
    const d = termRef.current?.onData((data) => api.ptyWrite(ptyId, data).catch(() => {}));
    return () => { un1.then((f) => f()); un2.then((f) => f()); d?.dispose(); };
  }, [ptyId]);

  const spawn = async (args: string[]) => {
    if (!info?.path) return;
    if (ptyId) await api.ptyKill(ptyId).catch(() => {});
    termRef.current?.clear();
    const id = await api.ptySpawn({
      command: info.path, args, cwd: project?.path ?? undefined,
      cols: termRef.current?.cols ?? 120, rows: termRef.current?.rows ?? 30,
    });
    set({ ptyId: id });
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
      <div className="row" style={{ padding: "8px 12px", borderBottom: "1px solid var(--border)" }}>
        <button className="primary small" onClick={() => spawn([])} disabled={!info?.path}>devin TUI 시작</button>
        <input className="grow" placeholder="추가 인자 (예: --model swe-2-max -r 세션ID)" value={cmdline} onChange={(e) => setCmdline(e.target.value)} />
        <button className="small" onClick={() => spawn(cmdline.trim().split(/\s+/).filter(Boolean))} disabled={!info?.path}>인자로 시작</button>
        {ptyId && <button className="danger small" onClick={() => api.ptyKill(ptyId).then(() => set({ ptyId: null }))}>종료</button>}
        {!ptyId && <button className="small" onClick={() => set({ ptyId: "login" })}>로그인 세션 연결</button>}
        <span className="dim small">cwd: {project?.path ?? "-"}</span>
      </div>
      <div ref={hostRef} className="term-container" />
    </div>
  );
}
