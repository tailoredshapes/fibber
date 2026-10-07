// examples/webgpu/js/browser.mjs: drives a Chromium that browser.sh started with --remote-debugging-port, over the DevTools protocol (node's
// WebSocket): opens browser.html, relays the page's console lines, and exits with the module's status when the page prints `exit N`.
//   node browser.mjs DEVTOOLS_PORT URL [TIMEOUT_S]
import { setTimeout as sleep } from "node:timers/promises";

const [port, url, timeoutS = "120"] = process.argv.slice(2);
const deadline = Date.now() + Number(timeoutS) * 1000;
let targets = [];
while (Date.now() < deadline) {
  try { targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); if (targets.length) break; } catch {}
  await sleep(250);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.log("browser.mjs: no page target on the devtools port"); console.log("exit 2"); process.exit(2); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0;
const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++id, method, params }));
let status = null;
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.method === "Runtime.consoleAPICalled") {
    const line = m.params.args.map((a) => a.value ?? a.description ?? "").join(" ");
    console.log(line);
    const e = /^exit (\d+)$/.exec(line);
    if (e) status = Number(e[1]);
  }
  if (m.method === "Runtime.exceptionThrown") console.log("page exception: " + (m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text));
};
send("Runtime.enable");
send("Page.enable");
send("Page.navigate", { url });
while (status === null && Date.now() < deadline) await sleep(100);
ws.close();
if (status === null) { console.log("browser.mjs: the page did not report an exit within " + timeoutS + " s"); process.exit(2); }
process.exit(status);
