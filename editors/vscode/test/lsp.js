'use strict';
// Spawns the real `fibref lsp` and does initialize, didOpen (a broken
// buffer, then a good one), completion, hover, shutdown and exit over
// stdio, asserting what comes back. No vscode needed. The binary is
// $FIBREF, else $CARGO_TARGET_DIR/debug/fibref, else ../../target/debug/fibref;
// with none of them the test says so and exits 0 (skipped, not passed).

const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

function findBinary() {
  const candidates = [
    process.env.FIBREF,
    process.env.CARGO_TARGET_DIR && path.join(process.env.CARGO_TARGET_DIR, 'debug', 'fibref'),
    path.join(__dirname, '..', '..', '..', 'target', 'debug', 'fibref'),
  ];
  return candidates.find((c) => c && fs.existsSync(c));
}

/** A client of one server process: framed JSON-RPC both ways. */
class Client {
  constructor(bin) {
    this.child = spawn(bin, ['lsp'], { stdio: ['pipe', 'pipe', 'inherit'] });
    this.buf = Buffer.alloc(0);
    this.waiting = [];
    this.seen = [];
    this.exited = new Promise((resolve) => this.child.on('exit', (code) => resolve(code)));
    this.child.stdout.on('data', (d) => {
      this.buf = Buffer.concat([this.buf, d]);
      this.drain();
    });
  }

  drain() {
    for (;;) {
      const end = this.buf.indexOf('\r\n\r\n');
      if (end < 0) return;
      const m = /Content-Length: (\d+)/i.exec(this.buf.subarray(0, end).toString());
      assert(m, 'a Content-Length header');
      const start = end + 4;
      const len = Number(m[1]);
      if (this.buf.length < start + len) return;
      const msg = JSON.parse(this.buf.subarray(start, start + len).toString('utf8'));
      this.buf = this.buf.subarray(start + len);
      this.seen.push(msg);
      this.waiting = this.waiting.filter((w) => !w(msg));
    }
  }

  send(msg) {
    const body = JSON.stringify({ jsonrpc: '2.0', ...msg });
    this.child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  }

  /** Resolves with the first message (already seen or to come) that `pred` accepts. */
  expect(pred, what) {
    const hit = this.seen.find(pred);
    if (hit) return Promise.resolve(hit);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`timed out waiting for ${what}`)), 20000);
      this.waiting.push((m) => {
        if (!pred(m)) return false;
        clearTimeout(timer);
        resolve(m);
        return true;
      });
    });
  }

  request(id, method, params) {
    this.send({ id, method, params });
    return this.expect((m) => m.id === id, `the answer to ${method}`);
  }
}

const URI = 'file:///nonexistent/fibber-test/a.fib';
const isDiagnostics = (n) => (m) =>
  m.method === 'textDocument/publishDiagnostics' && m.params.uri === URI && m.params.diagnostics.length === n;

async function main() {
  const bin = findBinary();
  if (!bin) {
    console.log('SKIPPED: no fibref binary (set FIBREF or build with cargo build -p fibref)');
    return;
  }
  const c = new Client(bin);
  const init = await c.request(1, 'initialize', { processId: null, rootUri: null, capabilities: {} });
  assert.strictEqual(init.result.capabilities.hoverProvider, true);
  assert.deepStrictEqual(init.result.capabilities.completionProvider.triggerCharacters, ['(', '/', '.', ':', ' ']);
  c.send({ method: 'initialized', params: {} });

  const broken = '(defun f (x: i64) -> i64\n  (let ((y 1)) (+ x ';
  c.send({
    method: 'textDocument/didOpen',
    params: { textDocument: { uri: URI, languageId: 'fibber', version: 1, text: broken } },
  });
  const diag = await c.expect(isDiagnostics(1), 'one diagnostic for the broken buffer');
  assert.strictEqual(diag.params.diagnostics[0].severity, 1);

  const at = { textDocument: { uri: URI }, position: { line: 1, character: 21 } };
  const items = (await c.request(2, 'textDocument/completion', at)).result;
  const labels = items.map((i) => i.label);
  for (const want of ['x', 'y', 'map', 'let']) assert(labels.includes(want), `${want} among ${labels.length} items`);
  const map = items.find((i) => i.label === 'map');
  assert(map.detail.length > 0, 'a signature as detail');

  const good = '(defun inc (k: i64) -> i64 (+ k 1))\n(defun g () -> i64 (inc 1))\n';
  c.send({
    method: 'textDocument/didChange',
    params: { textDocument: { uri: URI, version: 2 }, contentChanges: [{ text: good }] },
  });
  await c.expect(isDiagnostics(0), 'no diagnostics for the good buffer');
  const hover = await c.request(3, 'textDocument/hover', {
    textDocument: { uri: URI },
    position: { line: 1, character: 21 },
  });
  assert(hover.result.contents.value.includes('inc : (fn'), JSON.stringify(hover.result));

  const bad = await c.request(4, 'textDocument/nonsense', {});
  assert.strictEqual(bad.error.code, -32601);

  assert.strictEqual((await c.request(5, 'shutdown', null)).result, null);
  c.send({ method: 'exit' });
  assert.strictEqual(await c.exited, 0);
  console.log('ok: initialize, didOpen, completion, didChange, hover, errors, shutdown, exit');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
