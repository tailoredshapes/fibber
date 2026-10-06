// compiler/js/rt/core.js: the core of the JS runtime of lir2js (docs/design/js-backend.md): memory, stack, function table, the end of the
// process (exit status, trap signals). The runtime files are concatenated, in the order lir2js names them, ahead of the translated module,
// into one ES module: they share one scope and have no imports or exports. Every runtime name starts with `$`.
//
// Memory is one ArrayBuffer of a fixed size, reserved at start (FIB_JS_HEAP_MB, default 4096): V8 commits its pages lazily, so a 4 GB
// buffer costs 45 MB of resident memory, and a buffer that never grows never invalidates a view, so the translated code reads and writes it
// through `$D` (a DataView) directly. A pointer is a JS number: the address, which is the byte offset plus $BIAS. Addresses below $BIAS (the
// first page, null included) are negative offsets, which the DataView refuses with a RangeError: the SIGSEGV of a null dereference.
import * as $fs from 'node:fs';
import * as $tty from 'node:tty';
import * as $os from 'node:os';

const $BIAS = 4096;
const $HEAP = (Number(process.env.FIB_JS_HEAP_MB) || 4096) * 1048576;
const $buf = new ArrayBuffer($HEAP);
const $D = new DataView($buf);
const $U8 = new Uint8Array($buf);
const $STACK = 8 << 20;              // the lIR stack (allocas): 8 MiB, as the native main thread's
const $STACK_END = $BIAS + $STACK;
let $SP = $BIAS + 64;                // grows up; a function that allocas saves it on entry and restores it on return and before a tail call
let $brk = $STACK_END;               // the static data and the heap above it

// A block of `size` bytes aligned to `align` from the bump pointer; static data and the allocator's fresh blocks come from here.
function $sbrk(size, align) {
  const p = Math.ceil($brk / align) * align;
  if (p + size - $BIAS > $HEAP) return 0;
  $brk = p + size;
  return p;
}
// A static object of the module (a global, a string constant), zeroed.
function $static(size, align) {
  const p = $sbrk(Math.max(size, 1), Math.max(align, 1));
  if (p === 0) $die('lir2js: out of memory for static data (raise FIB_JS_HEAP_MB)');
  return p;
}
// `(alloca T n)`: size in bytes, align a power of two.
function $alloca(size, align) {
  const p = Math.ceil($SP / align) * align;
  if (p + size > $STACK_END) $signal('SIGSEGV');   // the native stack overflows into its guard page
  $SP = p + size;
  return p;
}
// The bytes of a JS byte list at a new static address, with a NUL (`(string "..")`).
function $cstr(bytes) {
  const p = $static(bytes.length + 1, 1);
  $U8.set(bytes, p - $BIAS);
  return p;
}
// The bytes at p up to the NUL, and the same as text (UTF-8).
function $cbytes(p) {
  const o = p - $BIAS; let e = o;
  if (o < 0) $signal('SIGSEGV');
  while ($U8[e] !== 0) e++;
  return $U8.subarray(o, e);
}
function $ctext(p) { return Buffer.from($cbytes(p)).toString('utf8'); }

// Pointers in memory: 8 bytes, little endian.
function $lp(o) { return $D.getUint32(o, true) + $D.getUint32(o + 4, true) * 4294967296; }
function $sp(o, v) { $D.setUint32(o, v >>> 0, true); $D.setUint32(o + 4, Math.floor(v / 4294967296) >>> 0, true); }

// ---------------------------------------------------------------- functions as addresses
// A function's address is $FBASE + 16 * its index in $FT; no heap address reaches that far.
const $FBASE = 2 ** 44;
const $FT = [];
function $fnaddr(f) { $FT.push(f); return $FBASE + 16 * ($FT.length - 1); }
function $fn(p) {
  const i = (p - $FBASE) / 16;
  const f = $FT[i];
  if (f === undefined) $signal('SIGSEGV');
  return f;
}

// ---------------------------------------------------------------- tail calls
// A tail call to another function returns $TAIL after setting $tf and $ta; whoever called a function that may tail call runs the
// trampoline `$tl`, so the JS stack does not grow (JS has no guaranteed tail calls).
const $TAIL = { tail: true };
let $tf = null, $ta = null;
function $tl(r) {
  while (r === $TAIL) r = $tf.apply(null, $ta);
  return r;
}

// ---------------------------------------------------------------- the end of the process
class $Exit extends Error { constructor(kind, v) { super(kind); this.kind = kind; this.v = v; } }
// Dies of a signal, as the native process would: nothing buffered is flushed (llvm.trap and abort flush nothing).
function $signal(name) { throw new $Exit('signal', name); }
function $trap() { $signal('SIGILL'); }
function $die(msg) { throw new $Exit('die', msg); }
function $unsupported(what) { throw new $Exit('die', `lir2js: unsupported: ${what}`); }

function $finish(e) {
  if (e instanceof $Exit) {
    if (e.kind === 'exit') { $flushAll(); process.exit(e.v & 255); }
    if (e.kind === 'die') { $flushAll(); $fs.writeSync(2, e.v + '\n'); process.exit(70); }
    process.kill(process.pid, e.v);
    return;
  }
  if (e instanceof RangeError && /call stack/.test(e.message)) { process.kill(process.pid, 'SIGSEGV'); return; }
  if (e instanceof RangeError && /bounds|offset/i.test(e.message)) { process.kill(process.pid, 'SIGSEGV'); return; }
  $flushAll();
  $fs.writeSync(2, `lir2js: internal error: ${e && e.stack || e}\n`);
  process.exit(70);
}

// Runs main (argc, argv when it takes them) and exits with its status.
function $start(main, nparams) {
  let status;
  if (process.env.FIB_JS_UNLINK) { try { $fs.unlinkSync(process.env.FIB_JS_UNLINK); } catch (e) { /* already gone */ } }
  try {
    if ($pending.length) $unsupported($pending.join(', '));
    if (nparams === 2) {
      const words = [process.env.FIB_JS_ARGV0 || 'a.out', ...process.argv.slice(2)];
      const argv = $static(8 * (words.length + 1), 8);
      words.forEach((w, i) => $sp(argv - $BIAS + 8 * i, $cstr(Buffer.from(w, 'utf8'))));
      status = main(words.length, argv);
    } else status = main();
    status = $tl(status);
  } catch (e) { $finish(e); return; }
  $flushAll();
  process.exit(status & 255);
}

// ---------------------------------------------------------------- small helpers the translated code calls
// A declared function with no shim: calling it stops the program.
function $stub(name) { return (...a) => $unsupported(`@${name} (no shim in compiler/js/rt)`); }
// A shim's result as an i64.
function $c64(v) { return typeof v === 'bigint' ? v : BigInt(v); }
// select with both operands evaluated (lIR evaluates every operand).
function $sel(c, a, b) { return c ? a : b; }
// The fcmp predicates that read an operand twice.
function $fone(a, b) { return a < b || a > b ? 1 : 0; }
function $fueq(a, b) { return a < b || a > b ? 0 : 1; }
function $ford(a, b) { return a === a && b === b ? 1 : 0; }
function $funo(a, b) { return a === a && b === b ? 0 : 1; }
// A variable of the C library (`declare-global`): the address of a cell holding its value. Any other name is unsupported when the
// program starts (a variable cannot be stubbed by a function that stops when called).
function $extGlobal(name) {
  const cell = $static(8, 8);
  if (name === 'stdout') $sp(cell - $BIAS, $stdout);
  else if (name === 'stderr') $sp(cell - $BIAS, $stderr);
  else if (name === 'stdin') $sp(cell - $BIAS, $stdin);
  else if (name === 'environ') {
    const env = Object.entries(process.env).map(([k, v]) => `${k}=${v}`);
    const arr = $static(8 * (env.length + 1), 8);
    env.forEach((e, i) => $sp(arr - $BIAS + 8 * i, $cstr(Buffer.from(e, 'utf8'))));
    $sp(cell - $BIAS, arr);
  } else $pending.push(`global @${name}`);
  return cell;
}
const $pending = [];
