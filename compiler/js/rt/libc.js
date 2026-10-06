// compiler/js/rt/libc.js: the C library functions a translated module may `declare`, as JS shims ($X.name). A declared function with no
// shim here is translated to a stub that stops the program with `lir2js: unsupported: @name` (exit status 70) when it is called. The first
// slice is single-threaded: a mutex is always free, and pthread_create, pthread_cond_wait and pthread_join are unsupported.
const $X = Object.create(null);

// ---------------------------------------------------------------- malloc: power-of-two size classes, a free list per class
// A block has a 16-byte header (its class at p-16) and is 16-byte aligned, as glibc's malloc on x86-64.
const $free = [];
function $malloc(n) {
  let c = 5; while ((1 << c) < n + 16 && c < 30) c++;
  if (c >= 30 && n + 16 > 2 ** 30) { while (2 ** c < n + 16) c++; }
  const fl = $free[c];
  let b = fl && fl.length ? fl.pop() : 0;
  if (b === 0) { b = $sbrk(2 ** c, 16); if (b === 0) return 0; }
  $D.setUint32(b - $BIAS, c, true);
  return b + 16;
}
function $freeBlock(p) {
  if (p === 0) return;
  const c = $D.getUint32(p - 16 - $BIAS, true);
  ($free[c] ??= []).push(p - 16);
}
function $capacity(p) { return 2 ** $D.getUint32(p - 16 - $BIAS, true) - 16; }
$X.malloc = (n) => $malloc(Number(n));
$X.free = (p) => $freeBlock(p);
$X.calloc = (n, s) => { const sz = Number(n) * Number(s); const p = $malloc(sz); if (p) $U8.fill(0, p - $BIAS, p - $BIAS + sz); return p; };
$X.realloc = (p, n) => {
  n = Number(n);
  if (p === 0) return $malloc(n);
  if (n <= $capacity(p)) return p;
  const q = $malloc(n); if (q === 0) return 0;
  $U8.copyWithin(q - $BIAS, p - $BIAS, p - $BIAS + $capacity(p));
  $freeBlock(p);
  return q;
};
$X.aligned_alloc = (a, n) => { a = Number(a); if (a <= 16) return $malloc(Number(n)); $unsupported('aligned_alloc above 16'); };
$X.memcpy = (d, s, n) => { n = Number(n); if (n) $U8.copyWithin(d - $BIAS, s - $BIAS, s - $BIAS + n); return d; };
$X.memmove = $X.memcpy;
$X.memset = (d, v, n) => { n = Number(n); if (n) $U8.fill(v & 255, d - $BIAS, d - $BIAS + n); return d; };
$X.memcmp = (a, b, n) => {
  n = Number(n);
  for (let i = 0; i < n; i++) { const x = $U8[a - $BIAS + i], y = $U8[b - $BIAS + i]; if (x !== y) return x - y; }
  return 0;
};
$X.strlen = (p) => BigInt($cbytes(p).length);
$X.strcmp = (a, b) => { const x = $cbytes(a), y = $cbytes(b); for (let i = 0; ; i++) { const c = (x[i] ?? 0) - (y[i] ?? 0); if (c || i >= x.length) return c; } };

// ---------------------------------------------------------------- errno, the environment, the clock
const $errnoCell = () => $X.$errno ??= $static(4, 4);
function $setErrno(n) { $D.setInt32($errnoCell() - $BIAS, n, true); }
$X.__errno_location = () => $errnoCell();
const $envCache = new Map();
$X.getenv = (p) => {
  const k = $ctext(p), v = process.env[k];
  if (v === undefined) return 0;
  if (!$envCache.has(k)) $envCache.set(k, $cstr(Buffer.from(v, 'utf8')));
  return $envCache.get(k);
};
$X.clock_gettime = (clk, ts) => {
  let ns;
  if (clk === 0) ns = BigInt(Date.now()) * 1000000n; else ns = process.hrtime.bigint();
  $D.setBigInt64(ts - $BIAS, ns / 1000000000n, true); $D.setBigInt64(ts - $BIAS + 8, ns % 1000000000n, true);
  return 0;
};
$X.nanosleep = (req) => {
  const ms = Number($D.getBigInt64(req - $BIAS, true)) * 1000 + Number($D.getBigInt64(req - $BIAS + 8, true)) / 1e6;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
  return 0;
};
$X.sysconf = (n) => n === 84 ? 1n : $unsupported(`sysconf(${n})`);
$X.sched_yield = () => 0;
$X.getpid = () => process.pid;
$X.madvise = () => 0;
$X.exit = (c) => { throw new $Exit('exit', c); };
$X.abort = () => $signal('SIGABRT');

// ---------------------------------------------------------------- threads: one thread
const $tls = new Map(); let $keys = 0;
$X.pthread_key_create = (k) => { $D.setUint32(k - $BIAS, $keys++, true); return 0; };
$X.pthread_setspecific = (k, v) => { $tls.set(k, v); return 0; };
$X.pthread_getspecific = (k) => $tls.get(k) ?? 0;
for (const n of ['pthread_mutex_init', 'pthread_mutex_lock', 'pthread_mutex_unlock', 'pthread_cond_init', 'pthread_cond_broadcast',
  'pthread_cond_signal', 'pthread_detach', 'pthread_mutex_destroy', 'pthread_cond_destroy']) $X[n] = () => 0;

// ---------------------------------------------------------------- memory maps (anonymous ones only)
$X.mmap = (addr, len, prot, flags, fd) => {
  if (!(flags & 0x20)) $unsupported('mmap of a file');
  const n = Number(len), p = $sbrk(n, 4096);
  if (p === 0) return -1 >>> 0;
  $U8.fill(0, p - $BIAS, p - $BIAS + n);
  return p;
};
$X.munmap = () => 0;
// A page with no access cannot be made: every access would have to test it.
$X.mprotect = (p, n, prot) => prot === 0 ? $unsupported('mprotect PROT_NONE (no guard pages)') : 0;

// ---------------------------------------------------------------- files: descriptors, and FILE streams with C's buffering
const $fdpos = new Map();               // the file position of each descriptor this runtime opened (node has no lseek)
function $osErr(e) { $setErrno($os.constants.errno[e.code] ?? 5); return -1; }
$X.write = (fd, p, n) => $writeFd(fd, $U8.subarray(p - $BIAS, p - $BIAS + Number(n)));
function $writeFd(fd, bytes) {
  try {
    const pos = $fdpos.has(fd) ? $fdpos.get(fd) : null;
    let done = 0;
    while (done < bytes.length) done += $fs.writeSync(fd, bytes, done, bytes.length - done, pos === null ? null : pos + done);
    if (pos !== null) $fdpos.set(fd, pos + done);
    return BigInt(done);
  } catch (e) { return BigInt($osErr(e)); }
}
$X.read = (fd, p, n) => {
  try {
    const pos = $fdpos.has(fd) ? $fdpos.get(fd) : null;
    const k = $fs.readSync(fd, $U8, p - $BIAS, Number(n), pos);
    if (pos !== null) $fdpos.set(fd, pos + k);
    return BigInt(k);
  } catch (e) { return BigInt($osErr(e)); }
};
$X.openat = (dir, path, flags, mode) => {
  if (dir !== -100 && !$ctext(path).startsWith('/')) $unsupported('openat relative to a directory descriptor');
  try { const fd = $fs.openSync($ctext(path), flags & ~0x80000, mode ?? 0o666); $fdpos.set(fd, (flags & 0x400) ? $fs.fstatSync(fd).size : 0); return fd; }
  catch (e) { return $osErr(e); }
};
$X.open = (path, flags, mode) => $X.openat(-100, path, flags, mode);
$X.close = (fd) => { try { $fs.closeSync(fd); $fdpos.delete(fd); return 0; } catch (e) { return $osErr(e); } };
$X.lseek = (fd, off, whence) => {
  if (!$fdpos.has(fd)) { $setErrno(29); return -1n; }
  if (whence < 0 || whence > 2) { $setErrno(22); return -1n; }
  const base = whence === 0 ? 0 : whence === 1 ? $fdpos.get(fd) : $fs.fstatSync(fd).size;
  const p = base + Number(off);
  if (p < 0) { $setErrno(22); return -1n; }
  $fdpos.set(fd, p); return BigInt(p);
};
$X.isatty = (fd) => $tty.isatty(fd) ? 1 : 0;
$X.unlink = (p) => { try { $fs.unlinkSync($ctext(p)); return 0; } catch (e) { return $osErr(e); } };
$X.mkdir = (p, m) => { try { $fs.mkdirSync($ctext(p), m); return 0; } catch (e) { return $osErr(e); } };
$X.rmdir = (p) => { try { $fs.rmdirSync($ctext(p)); return 0; } catch (e) { return $osErr(e); } };
