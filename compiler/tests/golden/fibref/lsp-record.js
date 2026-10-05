'use strict';
// Records transcripts of a language server (the seed-1 `fibref lsp`) over a fixed list of scenarios, for the port of the server
// (docs/design/fibref-port.md section 6). usage: node lsp-record.js SERVER-BINARY [-I DIR].. > lsp-transcripts.jsonl
// Each output line is {"scenario":NAME,"dir":"->"|"<-","msg":{...}}: every message the client sends and every one the server
// answers, in order. A scenario sends one step at a time and waits for what the step must produce (the answer to a request, the
// diagnostics of a buffer), so a transcript is deterministic. The same file replays against the port: lsp-replay.js (not yet written).
const { spawn } = require('node:child_process');

const URI = 'file:///nonexistent/fibber-test/a.fib';
const GOOD = '(defun inc (k: i64) -> i64 (+ k 1))\n(defun g () -> i64 (inc 1))\n';
const BROKEN = '(defun f (x: i64) -> i64\n  (let ((y 1)) (+ x ';
const STRUCT = '(defstruct P (x: i64 y: i64))\n(defun gx (p: P) -> i64 (. p x))\n(defun main () -> i64 (gx (P 1 2)))\n';
const NS = '(ns a (:require [fib.string :as s]))\n(defun f (t: str) -> str (s/trim t))\n';
const BAD_TYPE = '(defun f (x: i64) -> str x)\n';
const UNICODE = '(defun f () -> str "héllo 😀") (defun g () -> i64 (+ 1 "x"))\n';

const open = (text, uri = URI) => ({ method: 'textDocument/didOpen', params: { textDocument: { uri, languageId: 'fibber', version: 1, text } }, await: 'diag' });
const change = (text, v = 2) => ({ method: 'textDocument/didChange', params: { textDocument: { uri: URI, version: v }, contentChanges: [{ text }] }, await: 'diag' });
const at = (line, character) => ({ textDocument: { uri: URI }, position: { line, character } });
const req = (method, params) => ({ method, params, id: true });

const scenarios = {
  'vscode-session': [
    req('initialize', { processId: null, rootUri: null, capabilities: {} }), { method: 'initialized', params: {} },
    open(BROKEN), req('textDocument/completion', at(1, 21)), change(GOOD), req('textDocument/hover', at(1, 21)),
    req('textDocument/nonsense', {}), req('shutdown', null), { method: 'exit' },
  ],
  'completion-contexts': [
    req('initialize', { capabilities: {} }), open(NS),
    req('textDocument/completion', at(1, 37)), req('textDocument/completion', at(1, 38)), req('textDocument/completion', at(0, 5)),
    change(STRUCT), req('textDocument/completion', at(1, 29)), req('textDocument/hover', at(0, 14)), req('textDocument/hover', at(2, 20)),
    req('shutdown', null), { method: 'exit' },
  ],
  'diagnostics': [
    req('initialize', { capabilities: {} }), open(BAD_TYPE), change(UNICODE, 2), change('(defun f () -> i64 1)\n', 3),
    { method: 'textDocument/didClose', params: { textDocument: { uri: URI } }, await: 'diag' },
    req('textDocument/hover', at(0, 1)), req('shutdown', null), { method: 'exit' },
  ],
  'protocol-errors': [
    req('initialize', { capabilities: {} }), req('textDocument/definition', at(0, 0)), req('textDocument/completion', {}),
    req('shutdown', null), req('textDocument/hover', at(0, 0)), { method: 'exit' },
  ],
  'exit-without-shutdown': [req('initialize', { capabilities: {} }), { method: 'exit' }],
};

function frames(buf, out) {
  for (;;) {
    const end = buf.indexOf('\r\n\r\n');
    if (end < 0) return buf;
    const len = Number(/Content-Length: (\d+)/i.exec(buf.subarray(0, end).toString())[1]);
    if (buf.length < end + 4 + len) return buf;
    out.push(JSON.parse(buf.subarray(end + 4, end + 4 + len).toString('utf8')));
    buf = buf.subarray(end + 4 + len);
  }
}

async function run(name, steps, bin, args) {
  const child = spawn(bin, ['lsp', ...args], { stdio: ['pipe', 'pipe', 'ignore'] });
  const seen = []; let buf = Buffer.alloc(0); let nseen = 0;
  const exited = new Promise((r) => child.on('exit', (c) => r(c)));
  child.stdout.on('data', (d) => { buf = frames(Buffer.concat([buf, d]), seen); });
  const lines = []; let id = 0;
  const waitFor = (pred, ms = 30000) => new Promise((resolve) => {
    const t0 = Date.now();
    const tick = () => { const i = seen.findIndex((m, k) => k >= nseen && pred(m)); if (i >= 0) return resolve(i); if (Date.now() - t0 > ms) return resolve(-1); setTimeout(tick, 10); };
    tick();
  });
  const flush = (upto) => { for (; nseen <= upto; nseen++) lines.push({ scenario: name, dir: '<-', msg: seen[nseen] }); };
  for (const s of steps) {
    const msg = { jsonrpc: '2.0', method: s.method, params: s.params };
    if (s.id) msg.id = ++id;
    lines.push({ scenario: name, dir: '->', msg });
    const body = JSON.stringify(msg);
    child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
    if (s.id) { const my = id; const i = await waitFor((m) => m.id === my); if (i >= 0) flush(i); }
    else if (s.await === 'diag') { const i = await waitFor((m) => m.method === 'textDocument/publishDiagnostics'); if (i >= 0) flush(i); }
  }
  const code = await Promise.race([exited, new Promise((r) => setTimeout(() => r('timeout'), 5000))]);
  if (code === 'timeout') child.kill();
  flush(seen.length - 1);
  lines.push({ scenario: name, dir: 'exit', msg: { code } });
  return lines;
}

(async () => {
  const bin = process.argv[2]; const args = process.argv.slice(3);
  for (const [name, steps] of Object.entries(scenarios)) for (const l of await run(name, steps, bin, args)) console.log(JSON.stringify(l));
})();
