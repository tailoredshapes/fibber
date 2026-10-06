// Memory access variants: DataView vs typed arrays, fixed vs resizable ArrayBuffer. Run: node mem.mjs
function time(name, f) {
  f(); const t0 = performance.now(); const r = f(); const t1 = performance.now();
  console.log(`${name.padEnd(48)} ${(t1 - t0).toFixed(1).padStart(8)} ms  ${r}`);
}
const fixed = new ArrayBuffer(1 << 24);
const rsz = new ArrayBuffer(1 << 24, { maxByteLength: 1 << 30 });
for (const [nm, buf] of [['fixed', fixed], ['resizable', rsz]]) {
  const dv = new DataView(buf), i32 = new Int32Array(buf), u8 = new Uint8Array(buf);
  time(`DataView i32 ${nm}`, () => { let s = 0; for (let r = 0; r < 20; r++) for (let p = 0; p < 4 << 20; p += 4) { dv.setInt32(p, p + r, true); s = (s + dv.getInt32(p, true)) | 0; } return s; });
  time(`Int32Array ${nm}`, () => { let s = 0; for (let r = 0; r < 20; r++) for (let p = 0; p < 4 << 20; p += 4) { i32[p >> 2] = p + r; s = (s + i32[p >> 2]) | 0; } return s; });
  time(`DataView i32 via helper fn, p-4096 ${nm}`, () => {
    const ld = (p) => dv.getInt32(p - 4096, true), st = (p, v) => dv.setInt32(p - 4096, v, true);
    let s = 0; for (let r = 0; r < 20; r++) for (let p = 4096; p < (4 << 20) + 4096; p += 4) { st(p, p + r); s = (s + ld(p)) | 0; } return s; });
  time(`Int32Array helper, aligned check ${nm}`, () => {
    const ld = (p) => (p & 3) ? dv.getInt32(p, true) : i32[p >> 2];
    let s = 0; for (let r = 0; r < 20; r++) for (let p = 0; p < 4 << 20; p += 4) { i32[p >> 2] = p + r; s = (s + ld(p)) | 0; } return s; });
}
