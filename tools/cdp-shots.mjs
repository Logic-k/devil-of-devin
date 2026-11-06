// Screenshot every tab of the vite-served UI via CDP.
// Usage: node cdp-shots.mjs [debugPort] [url] [outDir]
import { writeFileSync, mkdirSync } from "node:fs";

const port = process.argv[2] || "19222";
const url = process.argv[3] || "http://localhost:1420";
const outDir = process.argv[4] || "D:/Devil_of_Devin/report/shots";
mkdirSync(outDir, { recursive: true });

const list = await (await fetch(`http://localhost:${port}/json`)).json();
let page = list.find((t) => t.type === "page" && t.url.startsWith(url));
if (!page) {
  const res = await fetch(`http://localhost:${port}/json/new?${encodeURIComponent(url)}`, { method: "PUT" });
  page = await res.json();
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));

let id = 0;
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const mid = ++id;
    const on = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id === mid) {
        ws.removeEventListener("message", on);
        m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
      }
    };
    ws.addEventListener("message", on);
    ws.send(JSON.stringify({ id: mid, method, params }));
  });

await send("Page.enable");
await send("Runtime.enable");
await new Promise((r) => setTimeout(r, 2500)); // let React mount

const tabs = ["workers", "research", "develop", "tasks", "skills", "terminal", "settings"];
const names = ["01-workers", "02-research", "03-develop", "04-tasks", "05-skills", "06-terminal", "07-settings"];

for (let i = 0; i < tabs.length; i++) {
  const click = await send("Runtime.evaluate", {
    expression: `document.querySelectorAll('.tabs button')[${i}]?.click(); 'ok'`,
    returnByValue: true,
  });
  await new Promise((r) => setTimeout(r, 700));
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const file = `${outDir}/${names[i]}.png`;
  writeFileSync(file, Buffer.from(shot.data, "base64"));
  console.log("saved", file);
}

// collect console errors as a sanity signal
const errs = await send("Runtime.evaluate", {
  expression: `document.title + '|' + document.querySelectorAll('.tabs button').length`,
  returnByValue: true,
});
console.log("title/tabs:", JSON.stringify(errs.result?.value));
ws.close();
