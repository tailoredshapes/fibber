'use strict';
// Regression cases for what fuzz.js and the load measurements found in the language server (each is the minimal input of a finding), and the
// bounds that keep it so. Each case names the finding it pins; the planted faults of mutants.sh (utf8, nsslice, stack, bound, symbols, cache)
// each make one of them fail.
// usage: node hardening.js --server CMD [--arg W].. [--only NAME]      Exit: 0 all held; 1 a case failed.
const assert = require('node:assert');
const fs = require('node:fs');
const { spawn } = require('node:child_process');

const opt = { args: [] };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  if (a === '--server') opt.server = process.argv[++i]; else if (a === '--arg') opt.args.push(process.argv[++i]); else if (a === '--only') opt.only = process.argv[++i];
  else { console.error(`hardening: unknown option ${a}`); process.exit(2); }
}
if (!opt.server) { console.error('usage: node hardening.js --server CMD [--arg W].. [--only NAME]'); process.exit(2); }

const frame = (bytes) => Buffer.concat([Buffer.from(`Content-Length: ${bytes.length}\r\n\r\n`), bytes]);
class Client {
  constructor() {
    this.child = spawn('bash', ['-c', `ulimit -c 0; ulimit -v ${process.env.ULIMIT_V || 8000000}; exec "$0" "$@"`, opt.server, ...opt.args], { stdio: ['pipe', 'pipe', 'inherit'] });
    this.buf = Buffer.alloc(0); this.msgs = []; this.waiter = null; this.dead = null; this.id = 1;
    this.child.stdin.on('error', () => {});
    this.child.stdout.on('data', (d) => { this.buf = Buffer.concat([this.buf, d]); this.drain(); });
    this.exited = new Promise((res) => this.child.on('exit', (code, sig) => { this.dead = sig ? `signal ${sig}` : `exit ${code}`; this.wake(); res(this.dead); }));
  }
  drain() {
    for (;;) {
      const i = this.buf.indexOf('\r\n\r\n'); if (i < 0) return;
      const n = Number(/Content-Length: (\d+)/i.exec(this.buf.slice(0, i).toString())[1]); if (this.buf.length < i + 4 + n) return;
      this.msgs.push(JSON.parse(this.buf.slice(i + 4, i + 4 + n).toString('utf8'))); this.buf = this.buf.slice(i + 4 + n); this.wake();
    }
  }
  wake() { if (this.waiter) { const w = this.waiter; this.waiter = null; w(); } }
  raw(bytes) { this.child.stdin.write(frame(bytes)); }
  send(m) { this.raw(Buffer.from(JSON.stringify(m))); }
  async next(ms = 60000) {
    const until = Date.now() + ms;
    while (!this.msgs.length) {
      if (this.dead) throw new Error(`the server died: ${this.dead}`);
      if (Date.now() > until) throw new Error(`no answer in ${ms} ms`);
      await new Promise((res) => { this.waiter = res; setTimeout(res, 50); });
    }
    return this.msgs.shift();
  }
  async request(method, params, ms) { const id = this.id++; this.send({ jsonrpc: '2.0', id, method, params }); for (;;) { const m = await this.next(ms); if (m.id === id) return m; } }
  async open(text, ms) { this.send({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri: URI, text } } }); return (await this.next(ms)).params; }
  rss() { try { return Number(/VmRSS:\s+(\d+)/.exec(fs.readFileSync(`/proc/${this.child.pid}/status`, 'utf8'))[1]) / 1024; } catch (_) { return -1; } }
}
const URI = 'file:///tmp/hardening.fib';
const at = (line, character) => ({ textDocument: { uri: URI }, position: { line, character } });
const alive = async (c) => assert.strictEqual((await c.request('initialize', {})).result.serverInfo.name, 'fibref lsp', 'the server still answers');

// Only against a server built with a planted trap (mutants.sh `isolated`: the `(ns` slice fault) and run with FIB_LSP_ISOLATE=1: the handler traps,
// the request is answered -32603 with the trap's message, and the server goes on (a later request on a good buffer answers).
const TRAP_CASES = {
  async 'isolation: a handler that traps answers -32603 with the message and the server goes on'(c) {
    await c.request('initialize', {});
    await c.open('(\u4e2d', 60000);
    const r = await c.request('textDocument/completion', at(0, 1), 60000);
    assert.strictEqual(r.error && r.error.code, -32603, JSON.stringify(r));
    assert(/splits a character/.test(r.error.message), r.error.message);
    const p = await c.open('(ns x)\n(defun f () -> i64 1)\n', 60000);
    assert.deepStrictEqual(p.diagnostics, []);
    const ok = await c.request('textDocument/completion', at(1, 5), 60000);
    assert(Array.isArray(ok.result) && ok.result.length > 0, 'completion answers after the trap');
  },
};

const CASES = process.env.HARDEN_TRAP_BUILD === '1' ? TRAP_CASES : {
  // finding: a body that is not UTF-8 ended the server (exit 1); it must answer -32700 and go on
  async 'framing: a body that is not UTF-8 is answered with -32700 and the server goes on'(c) {
    c.raw(Buffer.from([0x7b, 0xff, 0xfe, 0x7d]));
    assert.strictEqual((await c.next()).error.code, -32700);
    c.raw(Buffer.from([0xc0, 0x80]));
    assert.strictEqual((await c.next()).error.code, -32700);
    await alive(c);
  },
  // finding: `(ns` was tested with a three-byte str-slice, which trapped when a character straddled byte 3 of a form ("splits a character")
  async 'analysis: a buffer with multibyte characters after a form head does not trap (str-slice split a character)'(c) {
    for (const text of ['(ééé', '(中', '}丶\u0017~NF', 'a\u0000(😀(ns', '(nés x)']) { await c.open(text); await c.request('textDocument/completion', at(0, 1), 60000); await c.request('textDocument/hover', at(0, 0), 60000); }
    c.send({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri: URI, text: 'x\ud800y\u0000z' } } }); await c.next();
    await alive(c);
  },
  // finding: Content-Length -1 reached `(array -1 0i8)`: a negative length is a framing error that ends the server by exit, not by a trap
  async 'framing: a negative Content-Length ends the server with exit status 1, not a trap'(c) {
    c.child.stdin.write('Content-Length: -5\r\n\r\n');
    assert.strictEqual(await c.exited, 'exit 1');
  },
  // finding: document symbols of a file of 25000 definitions took over 30 s (each range scanned the text from its start)
  async 'documentSymbol: 15000 definitions answer in a few seconds (line index, not a rescan per range)'(c) {
    await c.request('initialize', {});
    const text = '(ns x)\n' + Array.from({ length: 15000 }, (_, i) => `(defun f${i} () -> i64 1)`).join('\n') + '\n';
    await c.open(text, 120000);
    const t = Date.now(); const r = await c.request('textDocument/documentSymbol', { textDocument: { uri: URI } }, 120000);
    assert.strictEqual(r.result.length, 15000); assert(Date.now() - t < 10000, `took ${Date.now() - t} ms`);
  },
  // finding: `and` over 9000 or more arguments expands to as many nested forms and overflowed the 8 MiB stack in the ownership checker (SIGSEGV)
  async 'stack: and and or over 12000 arguments are checked, not a stack overflow'(c) {
    await c.request('initialize', {});
    for (const op of ['and', 'or']) {
      const p = await c.open(`(ns x)\n(defun main () -> i64 (if (${op} ${'true '.repeat(12000)}) 1 0))`, 120000);
      assert.deepStrictEqual(p.diagnostics, [], `${op}: ${JSON.stringify(p.diagnostics).slice(0, 200)}`);
    }
    await alive(c);
  },
  // the reader's own bound: nesting beyond 1000 is a diagnostic, however deep
  async 'reader: 100000 levels of nesting are a diagnostic'(c) {
    await c.request('initialize', {});
    const p = await c.open('('.repeat(100000), 60000);
    assert.strictEqual(p.diagnostics.length, 1); assert(/nested deeper than 1000/.test(p.diagnostics[0].message), p.diagnostics[0].message);
    const q = await c.open(`(ns x)\n(defun main () -> i64 ${'(+ 1 '.repeat(990)}1${')'.repeat(990)})`, 60000);
    assert.deepStrictEqual(q.diagnostics, [], 'depth 990 checks');
  },
  // the bound on work: a buffer over the limit is not checked and says so; the requests still answer
  async 'bound: a 1 MB buffer is not checked, says so, and the requests answer'(c) {
    await c.request('initialize', {});
    const text = '(ns x)\n' + '(defun f (a: i64) -> i64 (+ a 1))\n'.repeat(30000);
    const p = await c.open(text, 60000);
    assert.strictEqual(p.diagnostics.length, 1); assert(/not checked/.test(p.diagnostics[0].message), p.diagnostics[0].message);
    for (const m of ['completion', 'hover', 'definition']) assert('result' in (await c.request('textDocument/' + m, at(100, 5), 60000)), m);
    assert.strictEqual((await c.request('textDocument/documentSymbol', { textDocument: { uri: URI } }, 60000)).result.length, 20000, 'symbols capped at 20000');
    assert(c.rss() < 1500, `rss ${c.rss()} MB`);
  },
  // the cache: a request on the text the diagnostics were made from does not check the library again
  async 'latency: completion, hover and definition after the open of a checked buffer are quick (the run is cached)'(c) {
    await c.request('initialize', {});
    const text = '(ns x (:require [fib.string :as s]))\n(defstruct P (x: i64))\n(defun f (p: P) -> i64 (. p x))\n(defun g () -> i64 (f (P 1)))\n';
    const p = await c.open(text, 60000); assert.deepStrictEqual(p.diagnostics, []);
    const t = Date.now();
    for (const [m, l, ch] of [['completion', 3, 22], ['hover', 2, 24], ['definition', 3, 22], ['completion', 2, 28], ['hover', 3, 22], ['completion', 1, 5], ['definition', 2, 24], ['hover', 1, 8]]) await c.request('textDocument/' + m, at(l, ch), 60000);
    const ms = (Date.now() - t) / 8; assert(ms < 150, `${ms} ms per request: the library is checked on every request`);
  },
  // memory over a session: edits replace the cached runs, they do not pile up
  async 'memory: 30 didChange of a 35 KB buffer keep the server under 1 GB'(c) {
    await c.request('initialize', {});
    const base = '(ns x)\n' + Array.from({ length: 1000 }, (_, i) => `(defun f${i} (a: i64) -> i64 (+ a ${i}))`).join('\n') + '\n';
    await c.open(base, 120000);
    for (let i = 0; i < 30; i++) { c.send({ jsonrpc: '2.0', method: 'textDocument/didChange', params: { textDocument: { uri: URI }, contentChanges: [{ text: base + `(defun g${i} () -> i64 ${i})\n` }] } }); await c.next(120000); }
    assert(c.rss() < 1024, `rss ${c.rss()} MB`);
  },
};

(async () => {
  const failures = [];
  for (const [name, f] of Object.entries(CASES)) {
    if (opt.only && !name.startsWith(opt.only)) continue;
    const c = new Client(); const t = Date.now();
    try { await f(c); console.log(`ok   ${name} (${Date.now() - t} ms)`); } catch (e) { failures.push(name); console.log(`FAIL ${name}\n     ${String(e.message || e).split('\n').join('\n     ')}`); } finally { c.child.kill('SIGKILL'); }
  }
  console.log(failures.length ? `${failures.length} failed` : 'all passed');
  process.exit(failures.length ? 1 : 0);
})();
