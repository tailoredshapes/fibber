// Does V8 keep the bits of a NaN? A signalling double NaN through the ways a lir2js program holds values: a scalar, an array literal (a
// double array), a store into one, an array whose elements started as null (generic elements), and the runtime's `$rec` shape. Each
// line counts the iterations of a million that kept every bit. Run: node nan.mjs
const dv = new DataView(new ArrayBuffer(8));
function mk(hi, lo) { dv.setUint32(4, hi, true); dv.setUint32(0, lo, true); return dv.getFloat64(0, true); }
function bits(x) { dv.setFloat64(0, x, true); return dv.getUint32(4, true) * 4294967296 + dv.getUint32(0, true); }
function rec() { const n = arguments.length, r = new Array(n).fill(null); for (let i = 0; i < n; i++) r[i] = arguments[i]; return r; }
for (const [name, hi] of [['signalling', 0x7ff40000], ['quiet', 0x7ffc0000]]) {
  const n = mk(hi, 0x12345678), want = bits(n);
  let lit = 0, store = 0, generic = 0, viaRec = 0, scalar = 0;
  for (let i = 0; i < 1e6; i++) {
    const x = i % 2 ? n : mk(hi, 0x12345678);
    scalar += bits(x) === want;
    lit += bits([x, 1.5][0]) === want;
    const b = [1.5, 2.5]; b[0] = x; store += bits(b[0]) === want;
    const g = [null, null]; g[0] = x; g[1] = 2.5; generic += bits(g[0]) === want;
    viaRec += bits(rec(x, 2.5)[0]) === want;
  }
  console.log(`${name.padEnd(10)} scalar ${scalar} literal ${lit} store ${store} generic ${generic} rec ${viaRec}`);
}
