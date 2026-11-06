// End-to-end Loop run against a scratch project, driven through the real webview (CDP :9222).
// Usage: node tools/loop-e2e.mjs <projectPath> [outDir] [--install-harness]
import { writeFileSync, mkdirSync } from "node:fs";

const project = process.argv[2] ?? "D:\\dod-looptest";
const outDir = process.argv[3] && !process.argv[3].startsWith("--") ? process.argv[3] : "D:/Devil_of_Devin/report/shots-v2";
const installHarness = process.argv.includes("--install-harness");
mkdirSync(outDir, { recursive: true });

async function connect() {
  for (let i = 0; i < 120; i++) {
    try {
      const list = await (await fetch("http://localhost:9222/json")).json();
      const page = list.find((t) => t.type === "page" && t.url.includes("localhost:1420"));
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
        return ws;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 2000));
  }
  throw new Error("no webview");
}
const ws = await connect();
let id = 0;
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id;
  const on = (ev) => { const m = JSON.parse(ev.data); if (m.id === mid) { ws.removeEventListener("message", on); m.error ? reject(m.error) : resolve(m.result); } };
  ws.addEventListener("message", on);
  ws.send(JSON.stringify({ id: mid, method, params }));
});
const js = async (expr, awaitP = true, timeout = 120000) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: awaitP, returnByValue: true, timeout });
  if (r.exceptionDetails) return { err: r.exceptionDetails.exception?.description ?? JSON.stringify(r.exceptionDetails).slice(0, 400) };
  return r.result?.value;
};
const shot = async (name) => {
  await new Promise((r) => setTimeout(r, 500));
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${outDir}/${name}.png`, Buffer.from(s.data, "base64"));
  console.log("shot", name);
};
await send("Runtime.enable"); await send("Page.enable");
await new Promise((r) => setTimeout(r, 2000));
const P = JSON.stringify(project);
console.log("ipc:", await js(`import('/src/lib/ipc.ts').then(m => { window.__ipc = m; return 'ok'; })`));
console.log("store:", await js(`import('/src/store.ts').then(m => { window.__store = m.useStore; return 'ok'; })`));
if (installHarness) {
  console.log("harness install:", JSON.stringify(await js(`__ipc.api.harnessInstall().then(h => [h.installed.length, h.version, h.commit]).catch(e => 'ERR ' + e)`, true, 300000)));
}
console.log("projectAdd:", JSON.stringify(await js(`__ipc.api.projectAdd(${P}).then(p => p.name).catch(e => 'ERR ' + e)`)));
await js(`__store.setState({ view: 'loop' }); 'ok'`, false);
console.log("board:", JSON.stringify(await js(`__ipc.api.loopBoard(${P}).then(b => ({ tasks: b.tasks.map(t => t.id + ':' + t.status), gates: b.gates.map(g => g.name + (g.command ? '=' + g.command : '')) }))`)));
await shot("loop-00-board");
const run = await js(`__ipc.api.loopStart({ project: ${P}, budgetMinutes: 12, concurrency: 1 }).then(r => ({ id: r.id, status: r.status, resolved: r.resolved })).catch(e => 'ERR ' + e)`);
console.log("loopStart:", JSON.stringify(run));
if (!run?.id) process.exit(1);
let seen = 0, shots = 0;
for (let i = 0; i < 180; i++) {
  await new Promise((r) => setTimeout(r, 8000));
  const st = await js(`__ipc.api.loopStatus(${JSON.stringify(run.id)}).then(r => r ? ({ status: r.status, log: r.log.map(l => l.at.slice(11) + ' ' + l.msg), tasks: Object.values(r.tasks).map(t => t.id + ':' + t.status + ' a' + t.attempts), tokens: r.totalTokens }) : null)`);
  if (!st) { console.log("status null"); continue; }
  for (const l of st.log.slice(seen)) console.log("  ", l);
  seen = st.log.length;
  if (shots === 0 && st.tasks.some((t) => t.includes("in_progress"))) { shots++; await shot("loop-01-running"); }
  if (!["running", "preflight", "stopping"].includes(st.status)) {
    console.log("FINAL:", st.status, JSON.stringify(st.tasks), "tokens", st.tokens);
    break;
  }
}
await new Promise((r) => setTimeout(r, 1500));
await shot("loop-02-final");
// open the task detail for the screenshot
await js(`(function(){ const el = document.querySelector('.task'); el && el.click(); return !!el; })()`, false);
await shot("loop-03-detail");
await js(`__store.getState().openDoc('plan_setup/STATE.md'); 'ok'`, false);
await new Promise((r) => setTimeout(r, 1200));
await shot("loop-04-state-md");
ws.close();
