import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { api, ev } from "../lib/ipc";
import "@xterm/xterm/css/xterm.css";

function b64decode(b: string): Uint8Array {
  const bin = atob(b);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export default function TerminalTab({ devinPath }: { devinPath?: string }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [ptyId, setPtyId] = useState<string | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [cmdline, setCmdline] = useState("");

  useEffect(() => {
    if (!hostRef.current || termRef.current) return;
    const term = new Terminal({
      fontFamily: "Cascadia Code, Consolas, monospace",
      fontSize: 13,
      theme: { background: "#0a0c10", foreground: "#e6e9f0", cursor: "#7c5cff" },
      convertEol: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(hostRef.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;
    const onResize = () => {
      fit.fit();
      if (ptyId) api.ptyResize(ptyId, term.cols, term.rows).catch(() => {});
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [ptyId]);

  useEffect(() => {
    if (!ptyId) return;
    const un1 = ev.ptyOut(ptyId, (p) => termRef.current?.write(b64decode(p.data)));
    const un2 = ev.ptyExit(ptyId, () => termRef.current?.writeln("\r\n[process exited]"));
    const d = termRef.current?.onData((data) => api.ptyWrite(ptyId, data).catch(() => {}));
    return () => { un1.then((f) => f()); un2.then((f) => f()); d?.dispose(); };
  }, [ptyId]);

  const spawn = async (args: string[]) => {
    if (!devinPath) return;
    if (ptyId) { await api.ptyKill(ptyId).catch(() => {}); }
    termRef.current?.clear();
    const id = await api.ptySpawn({
      command: devinPath, args,
      cwd: "D:\\Devil_of_Devin",
      cols: termRef.current?.cols ?? 120, rows: termRef.current?.rows ?? 30,
    });
    setPtyId(id);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
      <div className="row" style={{ padding: "10px 14px", borderBottom: "1px solid var(--border)" }}>
        <button className="primary" onClick={() => spawn([])} disabled={!devinPath}>
          devin TUI 시작
        </button>
        <input className="grow" placeholder="추가 인자 (예: --model swe-2-max -r 세션ID)"
          value={cmdline} onChange={(e) => setCmdline(e.target.value)} />
        <button onClick={() => spawn(cmdline.trim().split(/\s+/).filter(Boolean))}
          disabled={!devinPath}>인자로 시작</button>
        {ptyId && <button className="danger" onClick={() => api.ptyKill(ptyId).then(() => setPtyId(null))}>종료</button>}
        <span className="dim small">완전한 Devin CLI REPL (ConPTY)</span>
      </div>
      <div ref={hostRef} className="term-container" />
    </div>
  );
}
