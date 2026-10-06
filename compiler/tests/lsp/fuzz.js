'use strict';
// Fuzzes the language server (and so the front end it calls: reader, expander, type checker, ownership checker, the scope reader) on hostile
// buffers, over the protocol (decision D5: a front end must be total on garbage input, a malformed buffer is diagnostics, never a trap).
// Every trap, crash, hang (a per-request deadline) or stack overflow is a FINDING: the input is saved under --out as JSON (the buffer as base64
// text, the steps that were being run) and `--minimise FILE` shrinks it (line, then character, delta debugging).
//
// Inputs (all deterministic in --seed): random bytes, lisp-ish token soup, prefixes of the corpus files (cases/ownership, compiler/lsp,
// compiler/tests/reader), mutated corpus files, nesting of 1k/10k/100k openers, huge strings and symbols and numbers, NULs, lone surrogates and
// every kind of bad JSON-RPC message (raw frames: invalid UTF-8, truncated JSON, wrong types, huge or negative positions).
// Per buffer: didOpen (analyse), then completion, hover, definition at a few positions (every position for a small buffer with --every),
// documentSymbol, a didChange to a mutated copy, didClose.
//
// usage: node fuzz.js --server CMD [--arg W].. [--seed N] [--count N] [--deadline MS] [--out DIR] [--kinds a,b,..] [--every]
//        node fuzz.js --server CMD [--arg W].. --minimise FILE [--deadline MS]
//        node fuzz.js --selftest        (the generator and the detector on a fake server that crashes, hangs and answers: must find all)
// Runs the server under `ulimit -v ULIMIT_V` (default 8000000, KB). Exit: 0 no finding; 1 findings; 2 usage.

const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const opt = { args: [], seed: 1, count: 100, deadline: 20000, out: null, kinds: null, every: false };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const v = () => process.argv[++i];
  if (a === '--server') opt.server = v();
  else if (a === '--arg') opt.args.push(v());
  else if (a === '--seed') opt.seed = Number(v());
  else if (a === '--count') opt.count = Number(v());
  else if (a === '--deadline') opt.deadline = Number(v());
  else if (a === '--out') opt.out = v();
  else if (a === '--kinds') opt.kinds = v().split(',');
  else if (a === '--every') opt.every = true;
  else if (a === '--minimise') opt.minimise = v();
  else if (a === '--selftest') opt.selftest = true;
  else if (a === '--corpus-root') opt.root = v();
  else { console.error(`fuzz: unknown option ${a}`); process.exit(2); }
}
const ROOT = opt.root || path.resolve(__dirname, '..', '..', '..');

// ---- deterministic randomness (mulberry32)
function rng(seed) {
  let s = seed >>> 0;
  const next = () => { s = (s + 0x6D2B79F5) >>> 0; let t = s; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
  const int = (n) => Math.floor(next() * n);
  return { next, int, pick: (xs) => xs[int(xs.length)] };
}

// ---- the corpus
function walk(dir, ext) { try { return fs.readdirSync(dir).filter((f) => f.endsWith(ext)).sort().map((f) => path.join(dir, f)); } catch (_) { return []; } }
function corpus() {
  const files = [...walk(path.join(ROOT, 'cases/ownership'), '.fib'), ...walk(path.join(ROOT, 'compiler/lsp'), '.fib'), ...walk(path.join(ROOT, 'compiler/tests/reader'), '.fib'),
    ...walk(path.join(ROOT, 'compiler/tests/lsp'), '.fib')];
  // the reader edge inputs are many and tiny: the programs (cases/ownership, the server's own sources) count three times each
  const w = (f) => (f.includes('/tests/reader/') ? 1 : 3);
  return files.filter((f) => fs.statSync(f).size < 200000).flatMap((f) => Array.from({ length: w(f) }, () => ({ file: f, text: fs.readFileSync(f, 'utf8') })));
}

// ---- hostile buffers
const TOKENS = ['(', ')', '[', ']', '{', '}', '#(', '#{', "'", '`', '~', '~@', '@', '^', '&', '\\', '#', '#_', '"', '""', '"\\', '"\\u', '"\\u{', ';', ';;', '\n', ' ', '\t', '\r', '\0',
  ':', '::', ':kw', '.', '..', '/', 'a/b', 'a/', '/b', '-', '+', '1', '-1', '0x', '0xff', '1e', '1e9', '1.5', '1/2', '99999999999999999999', '1i8', '1u8', '3.', '.5', 'nil', 'true',
  'defun', 'defstruct', 'defenum', 'defmacro', 'defrecord', 'defprotocol', 'ns', 'let', 'fn', 'match', 'loop', 'recur', 'cond', 'if', 'do', 'try-let', 'if-some', 'trap',
  '(ns x)', '(defun f () -> i64 1)', '(defun main () -> i64 0)', '(defstruct P (x: i64))', '(:use fib.core)', '(:require [fib.string :as s])', '(. p x)', '(s/trim "a")',
  '->', 'i64', 'str', '(Vec i64)', '(Option', '(fn (i64) i64)', 'é', '中', '😀', '\ud800', '\udc00', ' ', '﻿', '\u0000'];
function soup(r, n) { let s = ''; for (let i = 0; i < n; i++) s += r.pick(TOKENS) + (r.int(3) === 0 ? ' ' : ''); return s; }
function randomChars(r, n) { let s = ''; for (let i = 0; i < n; i++) { const k = r.int(10); s += String.fromCharCode(k < 6 ? 32 + r.int(95) : k < 8 ? r.int(32) : k < 9 ? 128 + r.int(0x700) : r.int(0xffff)); } return s; }
function mutate(r, text) {
  let s = text;
  for (let k = 1 + r.int(4); k > 0; k--) {
    const at = r.int(s.length + 1);
    switch (r.int(7)) {
      case 0: s = s.slice(0, at) + s.slice(at + 1 + r.int(8)); break;
      case 1: s = s.slice(0, at) + r.pick(TOKENS) + s.slice(at); break;
      case 2: s = s.slice(0, at) + randomChars(r, 1 + r.int(5)) + s.slice(at); break;
      case 3: { const e = Math.min(s.length, at + r.int(200)); s = s.slice(0, e) + s.slice(at, e) + s.slice(e); break; }
      case 4: s = s.slice(0, at); break;
      case 5: { const lines = s.split('\n'); const i = r.int(lines.length); lines.splice(i, 1); s = lines.join('\n'); break; }
      default: s = s.slice(0, at) + ')'.repeat(1 + r.int(4)) + s.slice(at);
    }
  }
  return s;
}
// 1000 is the reader's bound ("forms nested deeper than 1000 levels"): the depths below it reach the expander, the checkers and the scope reader
const NEST = [100, 500, 900, 990, 1000, 10000, 100000];
function nesting(r) {
  const n = r.pick(NEST);
  const open = r.pick(['(', '(+ 1 ', '(do ', '(if true ', '(let ((x 1)) ', '(fn () ', '(Vec ', '(f ', '(. ', "(match x ((some _) ", '[', '{', '#(', "'(", '(f ', '(let ((x ', '(do ', '(if ', '(defun f () -> i64 ', '"', '`(~', '(. ', '(match x ((some ']);
  const close = r.pick([')', ']', '}', '']);
  const head = r.pick(['', '(ns x)\n', '(ns x)\n(defun main () -> i64 ', '(ns x)\n(def v: ', '(ns x)\n(defun f () -> ', '(ns x)\n(defstruct S (a: ']);
  return head + open.repeat(n) + (r.int(2) ? 'x' : '1') + (r.int(2) ? close.repeat(n) : '');
}
function huge(r) {
  const n = r.pick([100000, 1000000]);
  switch (r.int(7)) {
    case 0: return '(ns x)\n(def s: str "' + 'a'.repeat(n) + (r.int(2) ? '"' : '') + ')';
    case 1: return '(ns x)\n(def ' + 'a'.repeat(n) + ': i64 1)';
    case 2: return '(ns x)\n(def n: i64 ' + '9'.repeat(n) + ')';
    case 3: return '; ' + 'x'.repeat(n) + '\n(ns x)';
    case 4: return '(ns x)\n' + '(defun f () -> i64 1)\n'.repeat(Math.floor(n / 40));
    case 5: return '(ns x)\n(def s: str "' + '\\u{1F600}'.repeat(Math.floor(n / 8)) + '")';
    default: return '(ns x)\n(def s: str "' + '\n'.repeat(n) + '")';
  }
}
// Flat but long: the constructs whose expansion or checking recurses on their length (cond, let, and/or, match arms, threading, fields, variants)
function flat(r) {
  const n = r.pick([500, 2000, 10000, 50000]);
  const rep = (f) => Array.from({ length: n }, (_, i) => f(i)).join(' ');
  const main = (body) => '(ns x)\n(defun main () -> i64 ' + body + ')';
  switch (r.int(10)) {
    case 0: return main('(cond ' + rep((i) => `(= ${i} 1) ${i}`) + ' :else 0)');
    case 1: return main('(let (' + rep((i) => `(v${i} ${i})`) + ') v0)');
    case 2: return main('(do ' + rep(() => '1') + ')');
    case 3: return main('(+ ' + rep(() => '1') + ')');
    case 4: return main('(if (and ' + rep(() => 'true') + ') 1 0)');
    case 5: return main('(if (or ' + rep(() => 'false') + ') 1 0)');
    case 6: return main('(match 1 ' + rep((i) => `(${i} ${i})`) + ' (_ 0))');
    case 7: return main('(->> 1 ' + rep(() => '(+ 1)') + ')');
    case 8: return '(ns x)\n(defstruct S (' + rep((i) => `f${i}: i64`) + '))\n(defenum E ' + rep((i) => `V${i}`) + ')';
    default: return main('(len [' + rep((i) => String(i)) + '])');
  }
}
function genBuffer(r, corp, kind) {
  const c = r.pick(corp);
  switch (kind) {
    case 'bytes': return randomChars(r, 1 + r.int(2000));
    case 'soup': return soup(r, 1 + r.int(300));
    case 'prefix': return c.text.slice(0, r.int(c.text.length + 1));
    case 'mutated': return mutate(r, c.text);
    case 'nesting': return nesting(r);
    case 'huge': return huge(r);
    case 'flat': return flat(r);
    case 'corpus': return c.text;
    default: throw new Error(`no kind ${kind}`);
  }
}
const KINDS = ['bytes', 'soup', 'prefix', 'mutated', 'nesting', 'huge', 'flat', 'corpus', 'frames'];

// ---- bad protocol frames (raw bytes; the server must answer or ignore, never die)
function frame(bytes) { return Buffer.concat([Buffer.from(`Content-Length: ${bytes.length}\r\n\r\n`), bytes]); }
function badFrames(r) {
  const j = (o) => frame(Buffer.from(JSON.stringify(o)));
  const uri = 'file:///tmp/fuzz.fib';
  const pos = (l, c) => ({ line: l, character: c });
  const odd = [null, true, -1, 0, 1.5, 1e30, -1e30, 9223372036854775807, 9223372036854775808, '', 'x', [], {}, [[]], { a: 1 }];
  const methods = ['textDocument/completion', 'textDocument/hover', 'textDocument/definition', 'textDocument/documentSymbol', 'initialize', 'shutdown', 'textDocument/didOpen',
    'textDocument/didChange', 'textDocument/didClose', 'textDocument/didSave', '', 'x', '$/cancelRequest', 'exit'].filter((m) => m !== 'exit' && m !== 'shutdown');
  const out = [];
  out.push(j({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri, text: 'a\ud800b\u0000c' } } }));
  for (let k = 0; k < 6; k++) {
    const m = r.pick(methods);
    const wrong = r.pick(odd);
    switch (r.int(8)) {
      case 0: out.push(j({ jsonrpc: '2.0', id: k + 10, method: m, params: wrong })); break;
      case 1: out.push(j({ jsonrpc: '2.0', id: r.pick(odd), method: m, params: { textDocument: { uri }, position: pos(r.pick(odd), r.pick(odd)) } })); break;
      case 2: out.push(j({ jsonrpc: '2.0', id: k + 10, method: m, params: { textDocument: r.pick(odd), position: pos(0, 0) } })); break;
      case 3: out.push(j({ jsonrpc: '2.0', id: k + 10, method: 'textDocument/hover', params: { textDocument: { uri }, position: pos(r.pick([-1, 0, 1, 99, 2 ** 31, 2 ** 53, 1e18]), r.pick([-1, 0, 1, 99, 2 ** 31, 2 ** 53, 1e18])) } })); break;
      case 4: out.push(j({ jsonrpc: '2.0', method: 'textDocument/didChange', params: { textDocument: { uri }, contentChanges: r.pick([[], [{}], [{ text: 5 }], [{ text: '(' }, { text: ')' }], wrong]) } })); break;
      case 5: out.push(frame(Buffer.from('{"jsonrpc":"2.0","id":1,"method":' + '['.repeat(r.pick([1000, 100000])) ))); break;
      case 6: out.push(frame(Buffer.from('['.repeat(r.pick([1000, 100000, 1000000]))))); break;
      default: out.push(frame(Buffer.from(Array.from({ length: 1 + r.int(60) }, () => r.int(256))))); // random bytes, mostly invalid UTF-8 / JSON
    }
  }
  out.push(frame(Buffer.from([0xff, 0xfe, 0x7b, 0x7d])));
  out.push(frame(Buffer.from([0xc0, 0x80])));
  out.push(frame(Buffer.from('{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///tmp/\xff"}}}', 'latin1')));
  return out;
}

// ---- a server under test
class Server {
  constructor() {
    const sh = `ulimit -c 0; ulimit -v ${process.env.ULIMIT_V || 8000000}; exec "$0" "$@"`;
    this.child = spawn('bash', ['-c', sh, opt.server, ...opt.args], { stdio: ['pipe', 'pipe', 'pipe'] });
    this.buf = Buffer.alloc(0); this.msgs = []; this.waiter = null; this.err = ''; this.dead = null;
    this.child.stdin.on('error', () => {});
    this.child.stderr.on('data', (d) => { this.err = (this.err + d.toString('latin1')).slice(-2000); });
    this.child.stdout.on('data', (d) => { this.buf = Buffer.concat([this.buf, d]); this.drain(); });
    this.exited = new Promise((res) => this.child.on('exit', (code, sig) => { this.dead = sig ? `signal ${sig}` : `exit ${code}`; this.wake(); res(this.dead); }));
  }
  drain() {
    for (;;) {
      const i = this.buf.indexOf('\r\n\r\n'); if (i < 0) return;
      const m = /Content-Length: (\d+)/i.exec(this.buf.slice(0, i).toString('latin1')); if (!m) { this.buf = this.buf.slice(i + 4); continue; }
      const n = Number(m[1]); if (this.buf.length < i + 4 + n) return;
      try { this.msgs.push(JSON.parse(this.buf.slice(i + 4, i + 4 + n).toString('utf8'))); } catch (e) { this.msgs.push({ unparseable: true }); }
      this.buf = this.buf.slice(i + 4 + n); this.wake();
    }
  }
  wake() { if (this.waiter) { const w = this.waiter; this.waiter = null; w(); } }
  send(m) { this.child.stdin.write(Buffer.isBuffer(m) ? m : frame(Buffer.from(JSON.stringify(m)))); }
  // the next message, or {fault: 'crash'|'hang', why}
  async next(ms) {
    const until = Date.now() + ms;
    while (this.msgs.length === 0) {
      if (this.dead) return { fault: 'crash', why: this.dead + (this.err ? ' stderr: ' + this.err.trim().slice(-300) : '') };
      const left = until - Date.now(); if (left <= 0) return { fault: 'hang', why: `no answer in ${ms} ms` };
      await new Promise((res) => { this.waiter = res; setTimeout(res, Math.min(left, 100)); });
    }
    return { msg: this.msgs.shift() };
  }
  kill() { try { this.child.kill('SIGKILL'); } catch (_) { /* gone */ } }
}

// The steps of one buffer: [{method, params, expect: 'diag' | 'reply' | 'none'}]
const URI = 'file:///tmp/fuzz.fib';
function steps(r, text, every) {
  const lines = text.split('\n');
  const at = [[0, 0], [lines.length - 1, lines[lines.length - 1].length], [0, 3]];
  if (every && text.length < 400) for (let l = 0; l < lines.length; l++) for (let c = 0; c <= lines[l].length; c++) at.push([l, c]);
  else for (let k = 0; k < 3; k++) { const l = r.int(lines.length); at.push([l, r.int(lines[l].length + 1)]); }
  at.push([lines.length + 5, 7], [0, 100000]);
  const out = [{ method: 'textDocument/didOpen', params: { textDocument: { uri: URI, text } }, expect: 'diag' }];
  for (const [l, c] of at) for (const m of ['completion', 'hover', 'definition']) out.push({ method: 'textDocument/' + m, params: { textDocument: { uri: URI }, position: { line: l, character: c } }, expect: 'reply' });
  out.push({ method: 'textDocument/documentSymbol', params: { textDocument: { uri: URI } }, expect: 'reply' });
  out.push({ method: 'textDocument/didChange', params: { textDocument: { uri: URI }, contentChanges: [{ text: mutate(r, text) }] }, expect: 'diag' });
  out.push({ method: 'textDocument/didClose', params: { textDocument: { uri: URI } }, expect: 'diag' });
  return out;
}

let nextId = 100;
async function runSteps(srv, list, deadline) {
  for (let i = 0; i < list.length; i++) {
    const s = list[i];
    if (s.raw) { // a notification gets no answer: a ping request after the frame says when the server is done with it
      const id = nextId++; srv.send(s.raw); srv.send({ jsonrpc: '2.0', id, method: 'fuzz/ping', params: {} });
      for (;;) { const a = await srv.next(deadline); if (a.fault) return { ...a, step: i, method: 'raw frame' }; if (a.msg && a.msg.id === id) break; }
      continue;
    }
    if (s.expect === 'reply') srv.send({ jsonrpc: '2.0', id: nextId++, method: s.method, params: s.params }); else srv.send({ jsonrpc: '2.0', method: s.method, params: s.params });
    const a = await srv.next(deadline);
    if (a.fault) return { ...a, step: i, method: s.method, position: s.params.position };
  }
  return null;
}
async function fresh() { const srv = new Server(); srv.send({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {} }); const a = await srv.next(opt.deadline); return a.fault ? { srv, fault: a } : { srv }; }

// ---- a finding and its minimiser
function save(f) {
  if (!opt.out) return;
  fs.mkdirSync(opt.out, { recursive: true });
  const name = path.join(opt.out, `finding-${f.kind}-${f.seed}-${f.n}.json`);
  fs.writeFileSync(name, JSON.stringify({ ...f, text: f.text === undefined ? undefined : Buffer.from(f.text, 'utf8').toString('base64'), steps: undefined, fault: f.fault }, null, 1));
  f.file = name;
}
async function failsOn(text, step) {
  const { srv, fault } = await fresh(); if (fault) { srv.kill(); return true; }
  const r = await runSteps(srv, [{ method: 'textDocument/didOpen', params: { textDocument: { uri: URI, text } }, expect: 'diag' }, ...(step ? [step] : [])], opt.deadline);
  srv.kill(); await srv.exited; return !!r;
}
async function minimise(text, step) {
  let cur = text; const test = (t) => failsOn(t, step);
  if (!(await test(cur))) return { text: cur, note: 'does not reproduce' };
  for (const unit of ['\n', null]) {
    let parts = unit ? cur.split(unit) : Array.from(cur);
    let n = 2;
    while (parts.length >= 2) {
      const size = Math.ceil(parts.length / n); let reduced = false;
      for (let i = 0; i < parts.length; i += size) {
        const cand = parts.slice(0, i).concat(parts.slice(i + size));
        if (cand.length && (await test(cand.join(unit || '')))) { parts = cand; n = Math.max(n - 1, 2); reduced = true; break; }
      }
      if (!reduced) { if (n >= parts.length) break; n = Math.min(parts.length, n * 2); }
    }
    cur = parts.join(unit || '');
  }
  return { text: cur };
}

async function main() {
  if (opt.selftest) return selftest();
  if (!opt.server) { console.error('usage: node fuzz.js --server CMD [--arg W].. [--seed N] [--count N] [--deadline MS] [--out DIR] [--kinds a,b] [--every] | --minimise FILE | --selftest'); process.exit(2); }
  if (opt.minimise) {
    const f = JSON.parse(fs.readFileSync(opt.minimise, 'utf8'));
    const text = Buffer.from(f.text, 'base64').toString('utf8');
    const step = f.step && f.step.method !== 'textDocument/didOpen' && f.step.params ? f.step : null;
    const m = await minimise(text, step);
    console.log(JSON.stringify({ minimal: m.text.length > 300 ? m.text.slice(0, 300) + '...' : m.text, bytes: m.text.length, note: m.note }));
    fs.writeFileSync(opt.minimise.replace(/\.json$/, '') + '.min.fib', m.text);
    return;
  }
  const corp = corpus(); if (!corp.length) { console.error('fuzz: no corpus under ' + ROOT); process.exit(2); }
  const kinds = opt.kinds || KINDS; const r = rng(opt.seed); const findings = [];
  let { srv, fault } = await fresh();
  const t0 = Date.now(); let ran = 0;
  for (let n = 0; n < opt.count; n++) {
    const kind = kinds[n % kinds.length]; let f = null; let text; let list;
    if (fault) { f = { kind: 'startup', fault }; ({ srv, fault } = await fresh()); }
    if (kind === 'frames') { list = badFrames(r).map((raw) => ({ raw })); text = undefined; }
    else { text = genBuffer(r, corp, kind); list = steps(r, text, opt.every); }
    const res = f ? null : await runSteps(srv, list, opt.deadline);
    ran++; if (process.env.FUZZ_VERBOSE) console.log(`input ${n} ${kind} ${text === undefined ? "frames" : Buffer.byteLength(text) + " bytes"} ${Date.now() - t0} ms`);
    if (res) {
      const step = res.step !== undefined && list[res.step] && !list[res.step].raw ? list[res.step] : null;
      const fd = { kind: res.fault, seed: opt.seed, n, input: kind, why: res.why, method: res.method, position: res.position, bytes: text === undefined ? 0 : Buffer.byteLength(text), text, step: step ? { method: step.method, params: step.params, expect: step.expect } : null };
      save(fd); findings.push(fd);
      console.log(`FINDING ${res.fault} [${kind} #${n}] at step ${res.step} ${res.method || ''} ${JSON.stringify(res.position || '')}: ${res.why}${fd.file ? ' -> ' + fd.file : ''}`);
      srv.kill(); await srv.exited; ({ srv, fault } = await fresh());
    }
  }
  srv.send({ jsonrpc: '2.0', id: 9, method: 'shutdown' }); srv.send({ jsonrpc: '2.0', method: 'exit' }); await Promise.race([srv.exited, new Promise((res) => setTimeout(res, 3000))]); srv.kill();
  console.log(`fuzz: ${ran} inputs, ${findings.length} findings, ${Math.round((Date.now() - t0) / 1000)} s (seed ${opt.seed})`);
  process.exit(findings.length ? 1 : 0);
}

// ---- self test: a fake server that answers, crashes on '(((' and hangs on a NUL; the fuzzer must report both and reach the end
async function selftest() {
  const fake = path.join(require('node:os').tmpdir(), `fuzz-fake-${process.pid}.js`);
  fs.writeFileSync(fake, `
let buf=Buffer.alloc(0);const out=(m)=>{const b=Buffer.from(JSON.stringify(m));process.stdout.write('Content-Length: '+b.length+'\\r\\n\\r\\n');process.stdout.write(b);};
process.stdin.on('data',d=>{buf=Buffer.concat([buf,d]);for(;;){const i=buf.indexOf('\\r\\n\\r\\n');if(i<0)return;const n=+/Content-Length: (\\d+)/.exec(buf.slice(0,i).toString())[1];if(buf.length<i+4+n)return;const t=buf.slice(i+4,i+4+n).toString();buf=buf.slice(i+4+n);
let m;try{m=JSON.parse(t)}catch(e){out({jsonrpc:'2.0',id:null,error:{code:-32700,message:'x'}});continue}
if(t.includes('(((((')) process.exit(77); if(t.includes('\\\\u0000')) {continue;}
if(m.id!==undefined)out({jsonrpc:'2.0',id:m.id,result:null});else if(m.method==='textDocument/didOpen'||m.method==='textDocument/didChange'||m.method==='textDocument/didClose')out({jsonrpc:'2.0',method:'textDocument/publishDiagnostics',params:{}});}});`);
  opt.server = process.execPath; opt.args = [fake]; opt.deadline = 5000;
  const r = rng(7); const probes = [['plain', 'hello (ns x)'], ['crash', '(' .repeat(50)], ['hang', 'a\u0000b'], ['plain2', '(ns y)']]; let crash = 0; let hang = 0; let ok = 0;
  for (const [name, text] of probes) {
    const { srv } = await fresh(); const res = await runSteps(srv, steps(r, text, false), opt.deadline); srv.kill();
    if (!res) ok++; else if (res.fault === 'crash') crash++; else if (res.fault === 'hang') hang++;
    console.log(`selftest ${name}: ${res ? res.fault + ' (' + res.why.slice(0, 40) + ')' : 'answered'}`);
  }
  const ms = minimiseProbe();
  fs.unlinkSync(fake);
  const good = ok === 2 && crash === 1 && hang === 1 && ms;
  console.log(good ? 'selftest: ok (found the crash and the hang, passed the clean inputs)' : 'selftest: FAILED');
  process.exit(good ? 0 : 1);
}
function minimiseProbe() { return typeof minimise === 'function' && typeof rng(1).next() === 'number'; }

main().catch((e) => { console.error(e); process.exit(2); });
