'use strict';
// Replays the recorded transcripts of the Rust language server (compiler/tests/golden/fibref/lsp-transcripts.jsonl, five scenarios) against a
// server of this tree and compares what comes back, message by message, in order: the protocol's shape, the error codes, the ranges of the
// diagnostics, the exit statuses. What it does NOT compare is the library: the recording came from the seed-1 library, so the items of a
// completion are compared by category (the sequence of "local", "this module", library, "core", "prelude" runs; the buffer's own items exactly).
// Then it runs scenarios of its own (framing, protocol errors, definition, document symbols) with exact expectations.
//
// usage: node replay.js --server CMD [--arg WORD].. [--transcripts FILE] [--only SCENARIO] [--fault KIND]
//   --server CMD   the executable; --arg WORD (repeatable) its arguments (`lsp` for `fibc`; none for the test program serve)
//   --fault KIND   plants a fault between the server and the comparator, to show the comparator can fail (exit status 1 expected):
//                  range (a diagnostic's start moves one character), drop (a diagnostic is dropped), code (-32601 becomes -32600),
//                  framing (the first Content-Length is one byte too long)
// Exit: 0 every check held; 1 a check failed; 2 usage.

const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const opt = { args: [], transcripts: path.join(__dirname, '..', 'golden', 'fibref', 'lsp-transcripts.jsonl') };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  if (a === '--server') opt.server = process.argv[++i];
  else if (a === '--arg') opt.args.push(process.argv[++i]);
  else if (a === '--transcripts') opt.transcripts = process.argv[++i];
  else if (a === '--only') opt.only = process.argv[++i];
  else if (a === '--fault') opt.fault = process.argv[++i];
  else { console.error(`replay: unknown option ${a}`); process.exit(2); }
}
if (!opt.server) { console.error('usage: node replay.js --server CMD [--arg WORD].. [--transcripts FILE] [--only SCENARIO] [--fault KIND]'); process.exit(2); }

// a planted fault may leave the client waiting for bytes that never come: fail fast then
const TIMEOUT_MS = opt.fault ? 4000 : 30000;

/** One server process: framed JSON-RPC both ways; a planted fault edits the raw bytes it sends. */
class Client {
  constructor(fault) {
    this.child = spawn(opt.server, opt.args, { stdio: ['pipe', 'pipe', 'inherit'] });
    this.buf = Buffer.alloc(0);
    this.queue = [];
    this.waiters = [];
    this.broken = null;
    this.fault = fault;
    this.faulted = false;
    this.exited = new Promise((resolve) => this.child.on('exit', (code, sig) => { this.done = true; this.wake(); resolve(sig ? `signal ${sig}` : code); }));
    this.child.stdin.on('error', () => {});
    this.child.stdout.on('data', (d) => { this.buf = Buffer.concat([this.buf, this.plant(d)]); this.drain(); });
  }

  plant(d) {
    if (this.fault === 'framing' && !this.faulted) {
      const text = d.toString('latin1');
      const m = /Content-Length: (\d+)/.exec(text);
      if (m) { this.faulted = true; return Buffer.from(text.replace(m[0], `Content-Length: ${Number(m[1]) + 1}`), 'latin1'); }
    }
    return d;
  }

  mutate(msg) {
    if (this.fault === 'range' && !this.faulted && msg.method === 'textDocument/publishDiagnostics' && msg.params.diagnostics.length) {
      this.faulted = true; msg.params.diagnostics[0].range.start.character += 1;
    } else if (this.fault === 'drop' && !this.faulted && msg.method === 'textDocument/publishDiagnostics' && msg.params.diagnostics.length) {
      this.faulted = true; msg.params.diagnostics.pop();
    } else if (this.fault === 'code' && !this.faulted && msg.error && msg.error.code === -32601) {
      this.faulted = true; msg.error.code = -32600;
    }
    return msg;
  }

  drain() {
    for (;;) {
      if (this.buf.length === 0) return;
      const end = this.buf.indexOf('\r\n\r\n');
      if (end < 0) return;
      const m = /^Content-Length: (\d+)$/.exec(this.buf.subarray(0, end).toString('latin1'));
      if (!m) { this.broken = `bad frame header: ${JSON.stringify(this.buf.subarray(0, end + 4).toString('latin1'))}`; this.wake(); return; }
      const start = end + 4;
      const len = Number(m[1]);
      if (this.buf.length < start + len) return;
      const body = this.buf.subarray(start, start + len).toString('utf8');
      this.buf = this.buf.subarray(start + len);
      try { this.queue.push(this.mutate(JSON.parse(body))); } catch (e) { this.broken = `body is not JSON: ${JSON.stringify(body)}`; this.wake(); return; }
      this.wake();
    }
  }

  wake() { const w = this.waiters; this.waiters = []; w.forEach((f) => f()); }

  send(msg) { this.raw(Buffer.from(frame(JSON.stringify({ jsonrpc: '2.0', ...msg })), 'utf8')); }
  raw(bytes) { this.child.stdin.write(bytes); }

  /** The next message from the server; rejects on a framing failure, a timeout, or the process ending first. */
  next() {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`timed out after ${TIMEOUT_MS} ms waiting for a message`)), TIMEOUT_MS);
      const check = () => {
        if (this.broken) { clearTimeout(timer); reject(new Error(this.broken)); return; }
        if (this.queue.length) { clearTimeout(timer); resolve(this.queue.shift()); return; }
        if (this.done) { clearTimeout(timer); reject(new Error('the server ended before sending the expected message')); return; }
        this.waiters.push(check);
      };
      check();
    });
  }

  request(id, method, params) { this.send({ id, method, params }); return this.next(); }
}

function frame(body) { return `Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`; }

// ---- comparison

/** A completion item's category: what the sequence of categories must match; the buffer's own items are compared exactly. */
function category(item) {
  if (item.documentation === 'local' || item.documentation === 'this module' || item.documentation === 'core' || item.documentation === 'prelude') return item.documentation;
  return 'library';
}

function runs(items) {
  const out = [];
  for (const i of items) { const c = category(i); if (out[out.length - 1] !== c) out.push(c); }
  return out;
}

function isItemList(v) { return Array.isArray(v) && v.every((i) => i && typeof i.label === 'string' && 'kind' in i); }

function compareItems(want, got, where) {
  assert(isItemList(got), `${where}: not a list of items: ${JSON.stringify(got).slice(0, 200)}`);
  for (const i of got) {
    assert.deepStrictEqual(Object.keys(i), ['label', 'kind', 'detail', 'documentation'], `${where}: item keys of ${i.label}`);
    assert(typeof i.kind === 'number' && typeof i.detail === 'string' && typeof i.documentation === 'string', `${where}: item types of ${i.label}`);
  }
  assert.deepStrictEqual(runs(got), runs(want), `${where}: the categories of the items`);
  const own = (l) => l.filter((i) => category(i) === 'local' || category(i) === 'this module');
  assert.deepStrictEqual(own(got), own(want), `${where}: the buffer's own items`);
  const kinds = new Set(got.map((i) => i.kind));
  for (const k of new Set(want.map((i) => i.kind))) assert(kinds.has(k), `${where}: no item of kind ${k}`);
}

/** Deep comparison; `capabilities` may have more keys than the recording (definition and symbols are new); completion lists by category. */
function compare(want, got, where) {
  if (isItemList(want) && want.length > 0 && 'detail' in want[0]) return compareItems(want, got, where);
  if (want && typeof want === 'object' && !Array.isArray(want)) {
    assert(got && typeof got === 'object' && !Array.isArray(got), `${where}: expected an object, got ${JSON.stringify(got)}`);
    const extra = where.endsWith('capabilities') ? new Set(['definitionProvider', 'documentSymbolProvider']) : new Set();
    const keys = Object.keys(got).filter((k) => !extra.has(k));
    assert.deepStrictEqual(keys, Object.keys(want), `${where}: keys`);
    for (const k of Object.keys(want)) compare(want[k], got[k], `${where}.${k}`);
    return undefined;
  }
  if (Array.isArray(want)) {
    assert(Array.isArray(got) && got.length === want.length, `${where}: expected ${want.length} elements, got ${JSON.stringify(got)}`);
    want.forEach((w, i) => compare(w, got[i], `${where}[${i}]`));
    return undefined;
  }
  assert.deepStrictEqual(got, want, `${where}`);
  return undefined;
}

// What the port answers differently on purpose: the recording is of a server without go to definition, so its -32601 for a definition request
// becomes the port's answer for a document that is not open (null), as completion and hover give.
const OVERRIDES = [{ method: 'textDocument/definition', want: (e) => (e.error && e.error.code === -32601 ? { jsonrpc: '2.0', id: e.id, result: null } : e) }];

function adjust(method, expected) {
  let e = expected;
  for (const o of OVERRIDES) if (o.method === method) e = o.want(e);
  return e;
}

// ---- the recorded scenarios

function loadScenarios(file) {
  const by = new Map();
  for (const line of fs.readFileSync(file, 'utf8').split('\n').filter(Boolean)) {
    const e = JSON.parse(line);
    if (!by.has(e.scenario)) by.set(e.scenario, []);
    by.get(e.scenario).push(e);
  }
  return by;
}

async function replay(name, events, fault) {
  const c = new Client(fault);
  let n = 0;
  try {
    for (let i = 0; i < events.length; i++) {
      const e = events[i];
      if (e.dir === '->') {
        c.send(Object.fromEntries(Object.entries(e.msg).filter(([k]) => k !== 'jsonrpc')));
      } else if (e.dir === '<-') {
        const got = await c.next();
        const prev = events.slice(0, i).reverse().find((x) => x.dir === '->');
        compare(adjust(prev.msg.method, e.msg), got, `${name} #${++n} (${prev.msg.method})`);
      } else if (e.dir === 'exit') {
        const code = await Promise.race([c.exited, new Promise((r) => setTimeout(() => r('timeout'), 10000))]);
        assert.strictEqual(code, e.msg.code, `${name}: exit status`);
      }
    }
    assert.strictEqual(c.queue.length, 0, `${name}: unexpected extra messages ${JSON.stringify(c.queue).slice(0, 300)}`);
  } finally { c.child.kill('SIGKILL'); }
}

// ---- scenarios of this port's own, with exact expectations

const URI = 'file:///nonexistent/fibber-test/a.fib';
const open = (text) => ({ method: 'textDocument/didOpen', params: { textDocument: { uri: URI, languageId: 'fibber', version: 1, text } } });
const at = (line, character) => ({ textDocument: { uri: URI }, position: { line, character } });
const INIT = { method: 'initialize', params: { capabilities: {} }, id: 1 };

async function finish(c) {
  assert.strictEqual((await c.request(99, 'shutdown', null)).result, null);
  c.send({ method: 'exit' });
  assert.strictEqual(await c.exited, 0, 'exit status after shutdown');
}

const OWN = {
  async 'framing: split writes, an extra header, multi-byte bodies'(c) {
    const body = JSON.stringify({ jsonrpc: '2.0', ...INIT });
    const bytes = Buffer.from(`Content-Length: ${Buffer.byteLength(body)}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n${body}`);
    for (let i = 0; i < bytes.length; i += 7) { c.raw(bytes.subarray(i, i + 7)); await new Promise((r) => setTimeout(r, 2)); }
    assert.strictEqual((await c.next()).id, 1);
    const text = '(defun f () -> str "héllo 😀")\n';
    c.send(open(text));
    const d = await c.next();
    assert.deepStrictEqual(d.params.diagnostics, [], 'a multi-byte buffer checks');
    await finish(c);
  },
  async 'framing: a body that is not JSON answers -32700 with a null id and the server goes on'(c) {
    c.raw(Buffer.from(frame('{')));
    const e = await c.next();
    assert.strictEqual(e.error.code, -32700);
    assert.strictEqual(e.id, null);
    assert.strictEqual((await c.request(1, 'initialize', { capabilities: {} })).id, 1);
    await finish(c);
  },
  async 'framing: a malformed header ends the server with status 1'(c) {
    c.raw(Buffer.from('Foo: 1\r\n\r\n{}'));
    assert.strictEqual(await c.exited, 1);
  },
  async 'framing: end of input without exit is status 1'(c) {
    assert.strictEqual((await c.request(1, 'initialize', { capabilities: {} })).id, 1);
    c.child.stdin.end();
    assert.strictEqual(await c.exited, 1);
  },
  async 'protocol: a missing id notification and a response from the client get no reply'(c) {
    c.send({ method: 'initialized', params: {} });
    c.send({ id: 7, result: null });
    c.send({ method: '$/cancelRequest', params: { id: 3 } });
    assert.strictEqual((await c.request(1, 'initialize', { capabilities: {} })).id, 1);
    await finish(c);
  },
  async 'completion: arity clauses fold into one item named by the bare name; alias exports list'(c) {
    await c.request(1, 'initialize', { capabilities: {} });
    c.send(open('(ns a (:require [fib.string :as s]))\n(defun f (v: (Vec i64)) -> i64 (count v))\n(defun g () -> i64 (f [1]))\n'));
    assert.deepStrictEqual((await c.next()).params.diagnostics, []);
    const items = (await c.request(2, 'textDocument/completion', at(2, 20))).result;
    const labels = items.map((i) => i.label);
    assert(labels.includes('map'), 'map among the labels');
    assert(!labels.some((l) => /\$\d+$/.test(l)), `no clause names: ${labels.filter((l) => /\$\d+$/.test(l))}`);
    assert.strictEqual(labels.filter((l) => l === 'map').length, 1, 'map once');
    assert(items.find((i) => i.label === 'map').detail.includes('fn'), 'a scheme as detail');
    assert(labels.includes('s/'), 'the alias as a module item');
    const ex = (await c.request(3, 'textDocument/completion', at(2, 22))).result;
    assert(Array.isArray(ex));
    const hover = (await c.request(4, 'textDocument/hover', at(0, 0))).result;
    assert.strictEqual(hover, null, 'no hover on the ns form');
    await finish(c);
  },
  async 'hover: an overloaded name shows every arity'(c) {
    await c.request(1, 'initialize', { capabilities: {} });
    c.send(open('(defun g (v: (Vec i64)) -> (Vec i64) (vec (map inc v)))\n'));
    await c.next();
    const hover = (await c.request(2, 'textDocument/hover', at(0, 44))).result;
    const value = hover.contents.value;
    assert(value.startsWith('```fibber\nmap : '), value);
    assert.strictEqual((value.match(/^map : /gm) || []).length >= 2, true, `one line per arity: ${value}`);
    await finish(c);
  },
  async 'definition: a library function lands in the library file, on its name'(c) {
    await c.request(1, 'initialize', { capabilities: {} });
    c.send(open('(defun g (v: (Vec i64)) -> (Vec i64) (filterv even? v))\n(defun h (v: (Vec i64)) -> (Vec i64) (vec (map inc v)))\n'));
    assert.deepStrictEqual((await c.next()).params.diagnostics, []);
    for (const [id, line, ch, name] of [[2, 0, 40, 'filterv'], [3, 1, 44, 'map']]) {
      const r = (await c.request(id, 'textDocument/definition', at(line, ch))).result;
      assert(r && r.uri.startsWith('file:///') && r.uri.endsWith('.fib') && r.uri !== URI, `a library file for ${name}: ${JSON.stringify(r)}`);
      const lines = fs.readFileSync(decodeURIComponent(r.uri.slice(7)), 'utf8').split('\n');
      assert.strictEqual(r.range.start.line, r.range.end.line);
      assert.strictEqual(lines[r.range.start.line].slice(r.range.start.character, r.range.end.character), name, `the range of ${name} spans ${name}`);
    }
    await finish(c);
  },
  async 'documentSymbol: the top-level definitions with their ranges'(c) {
    await c.request(1, 'initialize', { capabilities: {} });
    c.send(open('(defstruct P (x: i64))\n(defun inc (k: i64) -> i64 (+ k 1))\n(def limit: i64 3)\n'));
    await c.next();
    const r = (await c.request(2, 'textDocument/documentSymbol', { textDocument: { uri: URI } })).result;
    const flat = r.map((s) => [s.name, s.kind, s.range.start.line, s.selectionRange.start.line, s.selectionRange.start.character, s.selectionRange.end.character]);
    assert.deepStrictEqual(flat, [['P', 23, 0, 0, 11, 12], ['inc', 12, 1, 1, 7, 10], ['limit', 13, 2, 2, 5, 10]]);
    assert.deepStrictEqual(r[1].range, { start: { line: 1, character: 0 }, end: { line: 1, character: 35 } });
    assert.strictEqual((await c.request(3, 'textDocument/documentSymbol', { textDocument: { uri: 'file:///not/open.fib' } })).result, null);
    assert.strictEqual((await c.request(4, 'textDocument/documentSymbol', {})).error.code, -32602);
    await finish(c);
  },
  async 'definition: a local, a top-level definition, a field and nothing under whitespace'(c) {
    await c.request(1, 'initialize', { capabilities: {} });
    const src = '(defstruct P (x: i64 y: i64))\n(defun gx (p: P) -> i64 (. p y))\n(defun h (k: i64) -> i64 (gx (P k k)))\n(defun main () -> i64 (h 1))\n';
    c.send(open(src));
    assert.deepStrictEqual((await c.next()).params.diagnostics, []);
    const def = async (id, l, ch) => (await c.request(id, 'textDocument/definition', at(l, ch))).result;
    const rangeOf = (r) => r && [r.uri, r.range.start.line, r.range.start.character, r.range.end.line, r.range.end.character];
    assert.deepStrictEqual(rangeOf(await def(2, 2, 32)), [URI, 2, 10, 2, 11], 'the parameter k of h, at its use');
    assert.deepStrictEqual(rangeOf(await def(3, 2, 26)), [URI, 1, 7, 1, 9], 'gx, a top-level definition');
    assert.deepStrictEqual(rangeOf(await def(4, 1, 29)), [URI, 0, 21, 0, 22], 'the field y of P after (. p');
    assert.strictEqual(await def(5, 3, 0), null, 'no name under the open parenthesis');
    assert.strictEqual((await c.request(6, 'textDocument/definition', {})).error.code, -32602);
    await finish(c);
  },
};

// ---- main

async function main() {
  const failures = [];
  const run = async (name, f) => {
    try { await f(); console.log(`ok   ${name}`); } catch (e) { failures.push(name); console.log(`FAIL ${name}\n     ${String(e.message || e).split('\n').join('\n     ')}`); }
  };
  const scenarios = loadScenarios(opt.transcripts);
  for (const [name, events] of scenarios) {
    if (opt.only && opt.only !== name) continue;
    await run(`transcript ${name} (${events.length} events)`, () => replay(name, events, opt.fault));
  }
  if (!opt.only) {
    for (const [name, f] of Object.entries(OWN)) {
      await run(name, async () => { const c = new Client(opt.fault); try { await f(c); } finally { c.child.kill('SIGKILL'); } });
    }
  }
  console.log(failures.length ? `${failures.length} failed` : 'all passed');
  process.exit(failures.length ? 1 : 0);
}

main();
