// compiler/js/rt/stdio.js: C's FILE streams with glibc's buffering, so that a program that dies of a signal loses the same unflushed
// output as the native one: stdout is line-buffered on a terminal and fully buffered (4096 bytes, a pipe's st_blksize) otherwise; stderr is
// unbuffered; exit and a return from main flush every stream, abort and llvm.trap flush none.
const $streams = new Map();   // FILE address -> { fd, buf: number[], mode: 'line' | 'full' | 'none', err }
function $newStream(fd, mode) {
  const f = $static(16, 8);
  $streams.set(f, { fd, buf: [], mode, err: 0 });
  return f;
}
const $stdin = $newStream(0, 'full');
const $stdout = $newStream(1, $tty.isatty(1) ? 'line' : 'full');
const $stderr = $newStream(2, 'none');
function $stream(f) { const s = $streams.get(f); if (!s) $unsupported('a FILE that this runtime did not open'); return s; }
function $flush(s) {
  if (s.buf.length) { const b = Uint8Array.from(s.buf); s.buf = []; if ($writeFd(s.fd, b) < 0n) s.err = 1; }
}
function $flushAll() { for (const s of $streams.values()) $flush(s); }
function $out(f, bytes) {
  const s = $stream(f);
  if (s.mode === 'none') { $flush(s); if ($writeFd(s.fd, bytes) < 0n) s.err = 1; return bytes.length; }
  for (const b of bytes) {
    s.buf.push(b);
    if (s.buf.length >= 4096 || (s.mode === 'line' && b === 10)) $flush(s);
  }
  return bytes.length;
}
$X.printf = (fmt, ...a) => $out($stdout, $format(fmt, a));
$X.fprintf = (f, fmt, ...a) => $out(f, $format(fmt, a));
$X.dprintf = (fd, fmt, ...a) => Number($writeFd(fd, $format(fmt, a)));
$X.snprintf = (buf, n, fmt, ...a) => {
  const b = $format(fmt, a), cap = Number(n);
  if (cap > 0) { const k = Math.min(cap - 1, b.length); $U8.set(b.subarray(0, k), buf - $BIAS); $U8[buf - $BIAS + k] = 0; }
  return b.length;
};
$X.sprintf = (buf, fmt, ...a) => { const b = $format(fmt, a); $U8.set(b, buf - $BIAS); $U8[buf - $BIAS + b.length] = 0; return b.length; };
$X.puts = (p) => { $out($stdout, $cbytes(p)); $out($stdout, Uint8Array.of(10)); return 1; };
$X.fputs = (p, f) => { $out(f, $cbytes(p)); return 1; };
$X.putchar = (c) => { $out($stdout, Uint8Array.of(c & 255)); return c & 255; };
$X.fputc = (c, f) => { $out(f, Uint8Array.of(c & 255)); return c & 255; };
$X.putc = $X.fputc;
$X.fflush = (f) => { if (f === 0) $flushAll(); else $flush($stream(f)); return 0; };
$X.fwrite = (p, sz, n, f) => {
  const k = Number(sz) * Number(n);
  $out(f, $U8.slice(p - $BIAS, p - $BIAS + k));
  return $stream(f).err ? 0n : BigInt(n);
};
$X.fopen = (path, mode) => {
  const m = $ctext(mode).replace(/[bex]/g, '');
  const flags = { r: 0, 'r+': 2, w: 0x241, 'w+': 0x242, a: 0x441, 'a+': 0x442 }[m];
  if (flags === undefined) { $setErrno(22); return 0; }
  const fd = $X.openat(-100, path, flags, 0o666);
  return fd < 0 ? 0 : $newStream(fd, 'full');
};
$X.fclose = (f) => { const s = $stream(f); $flush(s); $streams.delete(f); return $X.close(s.fd); };
$X.fread = (p, sz, n, f) => {
  const s = $stream(f), want = Number(sz) * Number(n);
  let got = 0;
  while (got < want) { const k = Number($X.read(s.fd, p + got, BigInt(want - got))); if (k <= 0) { if (k < 0) s.err = 1; break; } got += k; }
  return BigInt(Math.floor(got / Number(sz)));
};
$X.ferror = (f) => $stream(f).err;
const $strerr = new Map();
$X.strerror = (n) => {
  if (!$strerr.has(n)) {
    const name = Object.entries($os.constants.errno).find(([, v]) => v === n)?.[0];
    $strerr.set(n, $cstr(Buffer.from($ERRTEXT[name] ?? `Unknown error ${n}`, 'utf8')));
  }
  return $strerr.get(n);
};
const $ERRTEXT = { ENOENT: 'No such file or directory', EACCES: 'Permission denied', EEXIST: 'File exists', EISDIR: 'Is a directory',
  ENOTDIR: 'Not a directory', ENOTEMPTY: 'Directory not empty', EBADF: 'Bad file descriptor', EINVAL: 'Invalid argument',
  ENOSPC: 'No space left on device', EPERM: 'Operation not permitted', EIO: 'Input/output error', ESPIPE: 'Illegal seek' };
$X.strtod = (s, e) => $strto(s, e, false);
$X.strtof = (s, e) => $strto(s, e, true);

// libm. The JS functions other than sqrt, floor, ceil, trunc, fabs and fmod are not correctly rounded and may differ from glibc's in
// the last place (docs/design/js-backend.md).
$X.trunc = Math.trunc; $X.truncf = Math.trunc; $X.floor = Math.floor; $X.ceil = Math.ceil; $X.sqrt = Math.sqrt; $X.fabs = Math.abs;
$X.floorf = Math.floor; $X.ceilf = Math.ceil; $X.sqrtf = (x) => Math.fround(Math.sqrt(x)); $X.fabsf = Math.abs;
$X.fmod = (a, b) => a % b; $X.round = $fround;
for (const n of ['sin', 'cos', 'tan', 'exp', 'log', 'log2', 'log10', 'pow', 'atan', 'atan2', 'asin', 'acos', 'sinh', 'cosh', 'tanh',
  'expm1', 'log1p', 'cbrt', 'hypot']) $X[n] = Math[n];
$X.exp2 = (x) => 2 ** x;
