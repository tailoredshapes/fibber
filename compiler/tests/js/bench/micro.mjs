// Micro-measurements behind the design decisions of docs/design/js-backend.md. Run: node micro.mjs
const N = 20_000_000;
function time(name, f) {
  f(); // warm
  const t0 = performance.now(); const r = f(); const t1 = performance.now();
  console.log(`${name.padEnd(44)} ${(t1 - t0).toFixed(1).padStart(8)} ms  ${r}`);
}
// (1) integers: sum of i*i over i < N, wrapping at 64 bits (an LCG to keep the values large)
time('i64 BigInt + asIntN', () => {
  let s = 0n, x = 1n;
  for (let i = 0; i < N; i++) { x = BigInt.asIntN(64, x * 6364136223846793005n + 1442695040888963407n); s = BigInt.asIntN(64, s + x); }
  return s;
});
time('i64 as pair of i32 (hi, lo), Math.imul', () => {
  let shi = 0, slo = 0, xhi = 0, xlo = 1;
  const ahi = 0x5851f42d, alo = 0x4c957f2d, chi = 0x14057b7e, clo = 0xf767814f;
  for (let i = 0; i < N; i++) {
    // x = x*a + c (64-bit wrap) with 16-bit limbs for the low product
    const a0 = alo & 0xffff, a1 = alo >>> 16, x0 = xlo & 0xffff, x1 = xlo >>> 16;
    let p00 = x0 * a0, p01 = x0 * a1, p10 = x1 * a0;
    let lo = p00 + ((p01 + p10) & 0xffff) * 65536;
    let carry = Math.floor(lo / 4294967296) + Math.floor((p01 + p10) / 65536) + x1 * a1;
    lo = lo >>> 0;
    let hi = (Math.imul(xhi, alo) + Math.imul(xlo, ahi) + carry) | 0;
    let l2 = lo + clo; let h2 = (hi + chi + (l2 > 0xffffffff ? 1 : 0)) | 0; xlo = l2 >>> 0; xhi = h2;
    let l3 = slo + xlo; shi = (shi + xhi + (l3 > 0xffffffff ? 1 : 0)) | 0; slo = l3 >>> 0;
  }
  return (BigInt(shi) << 32n) | BigInt(slo);
});
time('i32 only (|0, Math.imul) for scale', () => {
  let s = 0, x = 1;
  for (let i = 0; i < N; i++) { x = (Math.imul(x, 1664525) + 1013904223) | 0; s = (s + x) | 0; }
  return s;
});
time('i64 BigInt add only, small values', () => {
  let s = 0n;
  for (let i = 0n; i < BigInt(N); i++) s = BigInt.asIntN(64, s + i);
  return s;
});
time('f64 counter + Number check (f64-when-small)', () => {
  let s = 0;
  for (let i = 0; i < N; i++) { s = s + i; if (s > 9007199254740991) throw 0; }
  return s;
});
// (2) memory: write then read 1M i32 cells 20 times
const buf = new ArrayBuffer(1 << 24, { maxByteLength: 1 << 30 });
const dv = new DataView(buf), i32 = new Int32Array(buf);
time('DataView getInt32/setInt32 (le)', () => {
  let s = 0;
  for (let r = 0; r < 20; r++) for (let p = 0; p < 4 << 20; p += 4) { dv.setInt32(p, p + r, true); s = (s + dv.getInt32(p, true)) | 0; }
  return s;
});
time('Int32Array[p>>2]', () => {
  let s = 0;
  for (let r = 0; r < 20; r++) for (let p = 0; p < 4 << 20; p += 4) { i32[p >> 2] = p + r; s = (s + i32[p >> 2]) | 0; }
  return s;
});
time('DataView getBigInt64 (i64 loads)', () => {
  let s = 0n;
  for (let r = 0; r < 5; r++) for (let p = 0; p < 4 << 20; p += 8) { dv.setBigInt64(p, BigInt(p), true); s = BigInt.asIntN(64, s + dv.getBigInt64(p, true)); }
  return s;
});
// (3) control flow: a loop of N with a branch, structured vs loop-switch
time('structured loop', () => {
  let s = 0;
  for (let i = 0; i < N; i++) { if (i & 1) s = (s + i) | 0; else s = (s ^ i) | 0; }
  return s;
});
time('while(true) switch(label)', () => {
  let s = 0, i = 0, L = 0;
  for (;;) switch (L) {
    case 0: L = 1; continue;
    case 1: if (i < N) { L = 2; continue; } L = 4; continue;
    case 2: if (i & 1) { s = (s + i) | 0; } else { s = (s ^ i) | 0; } L = 3; continue;
    case 3: i = i + 1; L = 1; continue;
    case 4: return s;
  }
});
// (4) calls: direct recursion vs trampolined tail calls
const TAIL = {}; let tf = null, ta0 = 0, ta1 = 0;
function cnt(n, acc) { return n === 0 ? acc : cnt(n - 1, (acc + n) | 0); }
function cntT(n, acc) { if (n === 0) return acc; tf = cntT; ta0 = n - 1; ta1 = (acc + n) | 0; return TAIL; }
time('direct recursion depth 5000, x2000', () => { let s = 0; for (let k = 0; k < 2000; k++) s = (s + cnt(5000, 0)) | 0; return s; });
time('trampoline 10M tail calls', () => {
  let r = cntT(N / 2, 0); while (r === TAIL) r = tf(ta0, ta1); return r;
});
time('self tail call as loop 10M', () => { let n = N / 2, acc = 0; for (;;) { if (n === 0) return acc; acc = (acc + n) | 0; n = n - 1; } });
