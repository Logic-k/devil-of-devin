// Drive the REAL Tauri webview via WebView2 CDP on :9222
// Verifies: __TAURI__ bridge, event listen permission, worker create, prompt streaming.
const list = await (await fetch("http://localhost:9222/json")).json();
const page = list.find((t) => t.type === "page" && t.url.includes("localhost:1420"));
if (!page) { console.log("no page target"); process.exit(1); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));

let id = 0;
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id;
  const on = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id === mid) { ws.removeEventListener("message", on); m.error ? reject(m.error) : resolve(m.result); }
  };
  ws.addEventListener("message", on);
  ws.send(JSON.stringify({ id: mid, method, params }));
});
const evalJs = async (expr, awaitP = false, timeout = 30000) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: awaitP, returnByValue: true, timeout });
  if (r.exceptionDetails) return { err: r.exceptionDetails.exception?.description ?? JSON.stringify(r.exceptionDetails).slice(0, 300) };
  return r.result?.value;
};

await send("Runtime.enable");
await send("Page.enable");

console.log("== load app ipc module ==");
console.log(await evalJs(`import('/src/lib/ipc.ts').then(m => { window.__ipc = m; return 'ipc loaded: ' + Object.keys(m).join(','); }).catch(e => 'IMPORT FAIL: ' + e)`, true));

console.log("== listen permission check ==");
console.log(await evalJs(`__ipc.onEvent('acp://update/__probe__', () => {}).then(u => { u(); return 'LISTEN OK'; }).catch(e => 'LISTEN FAIL: ' + e)`, true));

console.log("== worker_create ==");
const worker = await evalJs(`__ipc.api.workerCreate({ cwd: 'D:\\\\Devil_of_Devin' }).then(w => w).catch(e => 'ERR: ' + e)`, true, 60000);
console.log(JSON.stringify(worker).slice(0, 400));
if (typeof worker !== "object" || !worker?.id) { console.log("create failed, abort"); process.exit(1); }

console.log("== prompt + collect events ==");
const result = await evalJs(`(async () => {
  const evs = [];
  const un = await __ipc.onEvent('acp://update/${worker.agentId}', (e) => {
    const u = e?.update ?? {};
    evs.push(u.sessionUpdate + '|' + (u.content?.text ?? '').slice(0, 60));
  });
  try {
    const r = await __ipc.api.workerPrompt('${worker.id}', 'Reply with just the word: PONG');
    un();
    return { events: evs, stopReason: r?.stopReason };
  } catch (e) { un(); return { events: evs, error: String(e) }; }
})()`, true, 120000);
console.log(JSON.stringify(result, null, 1).slice(0, 2000));
ws.close();
