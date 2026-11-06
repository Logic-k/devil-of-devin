// Drive the real Tauri webview via WebView2 CDP (:9222) — v0.2 shell.
// Usage: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev
//        node tools/smoke-v2.mjs [outDir] [--prompt]
import { writeFileSync, mkdirSync } from "node:fs";

const outDir = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "D:/Devil_of_Devin/report/shots-v2";
const doPrompt = process.argv.includes("--prompt");
mkdirSync(outDir, { recursive: true });

let page = null;
for (let i = 0; i < 90 && !page; i++) {
  try {
    const list = await (await fetch("http://localhost:9222/json")).json();
    page = list.find((t) => t.type === "page" && t.url.includes("localhost:1420"));
  } catch {}
  if (!page) await new Promise((r) => setTimeout(r, 2000));
}
if (!page) { console.log("no page target on :9222"); process.exit(1); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id;
  const on = (ev) => { const m = JSON.parse(ev.data); if (m.id === mid) { ws.removeEventListener("message", on); m.error ? reject(m.error) : resolve(m.result); } };
  ws.addEventListener("message", on);
  ws.send(JSON.stringify({ id: mid, method, params }));
});
const js = async (expr, awaitP = false, timeout = 60000) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: awaitP, returnByValue: true, timeout });
  if (r.exceptionDetails) return { err: r.exceptionDetails.exception?.description ?? JSON.stringify(r.exceptionDetails).slice(0, 400) };
  return r.result?.value;
};
const shot = async (name) => {
  await new Promise((r) => setTimeout(r, 600));
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${outDir}/${name}.png`, Buffer.from(s.data, "base64"));
  console.log("shot", name);
};
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.method === "Runtime.exceptionThrown") errors.push(m.params.exceptionDetails?.exception?.description?.slice(0, 300));
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") errors.push(m.params.args?.map((a) => a.value ?? a.description).join(" ").slice(0, 300));
});
await send("Runtime.enable"); await send("Page.enable");
await new Promise((r) => setTimeout(r, 2500));

console.log("ipc:", await js(`import('/src/lib/ipc.ts').then(m => { window.__ipc = m; return 'ok'; }).catch(e => 'FAIL ' + e)`, true));
console.log("store:", await js(`import('/src/store.ts').then(m => { window.__store = m.useStore; return 'ok'; }).catch(e => 'FAIL ' + e)`, true));
console.log("info:", JSON.stringify(await js(`__ipc.api.devinInfo()`, true)).slice(0, 200));
console.log("projectAdd:", JSON.stringify(await js(`__ipc.api.projectAdd('D:\\\\Devil_of_Devin').then(p => p.name).catch(e => 'ERR ' + e)`, true)));
console.log("harness:", JSON.stringify(await js(`__ipc.api.harnessStatus().then(h => [h.installed.length, h.complete, h.version]).catch(e => 'ERR ' + e)`, true)));
console.log("board:", JSON.stringify(await js(`__ipc.api.loopBoard('D:\\\\Devil_of_Devin').then(b => [b.tasks.length, b.gates.length]).catch(e => 'ERR ' + e)`, true)));
console.log("stages:", JSON.stringify(await js(`__ipc.api.projectStages('D:\\\\Devil_of_Devin').then(s => s.map(x => x.id + ':' + x.status).join(',')).catch(e => 'ERR ' + e)`, true)));

const views = ["pipeline", "loop", "docs", "workers", "skills", "terminal", "settings"];
for (const v of views) {
  await js(`__store.setState({ view: '${v}' }); 'ok'`);
  await shot(`v2-${v}`);
}

if (doPrompt) {
  await js(`__store.setState({ view: 'workers' })`);
  console.log("worker_create:");
  const w = await js(`__ipc.api.workerCreate({ cwd: 'D:\\\\Devil_of_Devin', name: 'smoke', project: 'D:\\\\Devil_of_Devin' }).then(w => w).catch(e => 'ERR ' + e)`, true, 90000);
  console.log(JSON.stringify(w).slice(0, 300));
  if (w?.id) {
    await js(`__store.getState().selectWorker('${w.id}')`);
    await js(`__store.getState().pushMsg('${w.agentId}', { kind: 'user', text: 'Reply with just the word: PONG' })`);
    const r = await js(`__ipc.api.workerPrompt('${w.id}', 'Reply with just the word: PONG').then(r => r).catch(e => 'ERR ' + e)`, true, 180000);
    console.log("prompt:", JSON.stringify(r).slice(0, 300));
    await shot("v2-workers-chat");
    // state survives a view switch?
    await js(`__store.setState({ view: 'docs' })`);
    await js(`__store.setState({ view: 'workers' })`);
    const kept = await js(`(__store.getState().transcripts['${w.agentId}'] || []).map(m => m.kind + ':' + m.text.slice(0, 20)).join(' | ')`);
    console.log("transcript after view switch:", kept);
    const usage = await js(`JSON.stringify(__store.getState().usage['${w.agentId}'])`);
    console.log("usage:", usage);
    await shot("v2-workers-chat-after-switch");
    await js(`__ipc.api.workerDelete('${w.id}', true)`, true, 30000);
  }
}
console.log("console errors:", JSON.stringify(errors.slice(0, 10), null, 1));
ws.close();
