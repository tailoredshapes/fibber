// compiler/js/rt/float.js: floating-point helpers of the JS runtime. A double is a JS number; a float is a JS number that Math.fround has
// rounded: one rounding of the exact double result of + - * / sqrt of two floats is the correctly rounded float result (53 >= 2*24+2), so
// `Math.fround(a op b)` is IEEE single arithmetic. What JS lacks is fma, which is here, exact, through BigInt.

// The exact value of a finite double as m * 2^e, m a BigInt (signed), e an integer.
const $fdv = new DataView(new ArrayBuffer(8));
function $decompose(x) {
  $fdv.setFloat64(0, x, true);
  const hi = $fdv.getUint32(4, true), lo = $fdv.getUint32(0, true);
  const be = (hi >>> 20) & 0x7ff;
  let m = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
  let e;
  if (be === 0) e = -1074; else { m |= 1n << 52n; e = be - 1075; }
  return [hi >>> 31 ? -m : m, e];
}
// The number m * 2^e rounded once, to nearest with ties to even, to `prec` bits of mantissa with the minimum exponent `emin` (the exponent
// of the least subnormal bit): 53 and -1074 for double, 24 and -149 for float. m is a non-zero BigInt.
function $roundTo(m, e, prec, emin) {
  const neg = m < 0n; if (neg) m = -m;
  const len = m.toString(2).length;
  let s = Math.max(len - prec, emin - e);
  if (s > 0) {
    const sb = BigInt(s), q = m >> sb, r = m - (q << sb), half = 1n << (sb - 1n);
    m = (r > half || (r === half && (q & 1n))) ? q + 1n : q;
    e += s;
  }
  let v = Number(m);                            // exact: at most prec + 1 bits
  while (e > 0) { const k = Math.min(e, 1000); v *= 2 ** k; e -= k; }
  while (e < 0) { const k = Math.min(-e, 1000); v /= 2 ** k; e += k; }   // exact: the result is representable or infinite
  return neg ? -v : v;
}
// llvm.fma: a*b+c with one rounding. Non-finite operands and an exactly zero sum take JS's own a*b+c, which is exact in those cases.
function $fmaWith(a, b, c, prec, emin) {
  if (!Number.isFinite(a) || !Number.isFinite(b) || !Number.isFinite(c) || a === 0 || b === 0) return a * b + c;
  const [ma, ea] = $decompose(a), [mb, eb] = $decompose(b);
  let m = ma * mb, e = ea + eb;
  if (c !== 0) {
    const [mc, ec] = $decompose(c);
    if (ec < e) { m <<= BigInt(e - ec); e = ec; m += mc; } else m += mc << BigInt(ec - e);
  }
  if (m === 0n) return a * b + c;               // an exact zero: the product is -c, representable; JS gives the IEEE sign
  return $roundTo(m, e, prec, emin);
}
function $fma64(a, b, c) { return $fmaWith(a, b, c, 53, -1074); }
function $fma32(a, b, c) {
  const r = $fmaWith(a, b, c, 24, -149);
  return Math.fround(r);                        // only an overflow to infinity changes here
}
// i64 to float with one rounding: Math.fround(Number(v)) rounds twice above 2^53, so the low bits are folded into a sticky bit first.
function $i2f32(v) {
  const neg = v < 0n; let m = neg ? -v : v;
  const len = m.toString(2).length;
  if (len > 53) {
    const s = BigInt(len - 53), q = m >> s;
    m = (q << s) === m ? q : (q | 1n);
    return Math.fround((neg ? -1 : 1) * Number(m) * 2 ** (len - 53));
  }
  return Math.fround(Number(neg ? -m : m));
}
function $u2f32(v) { return $i2f32(BigInt.asUintN(64, v)); }

// llvm.minimum / maximum: NaN if either is; -0 below +0.
function $fmin(a, b) { if (a !== a || b !== b) return NaN; if (a === 0 && b === 0) return Object.is(a, -0) ? a : b; return a < b ? a : b; }
function $fmax(a, b) { if (a !== a || b !== b) return NaN; if (a === 0 && b === 0) return Object.is(a, -0) ? b : a; return a > b ? a : b; }
// llvm.minnum / maxnum: a NaN operand gives the other; between -0 and +0 either (x86-64 gives the second operand of minsd: this picks b).
function $fminnum(a, b) { if (a !== a) return b; if (b !== b) return a; return a < b ? a : b; }
function $fmaxnum(a, b) { if (a !== a) return b; if (b !== b) return a; return a > b ? a : b; }
// llvm.round: ties away from zero (Math.round rounds ties up).
function $fround(x) { if (!Number.isFinite(x)) return x; const t = Math.trunc(x); const d = x - t; return Math.abs(d) >= 0.5 ? t + Math.sign(x) : (t === 0 ? x * 0 : t); }
// llvm.roundeven: ties to even.
function $froundeven(x) {
  if (!Number.isFinite(x) || Math.abs(x) >= 4503599627370496) return x;
  const f = Math.floor(x), d = x - f;
  const r = d < 0.5 ? f : d > 0.5 ? f + 1 : (f % 2 === 0 ? f : f + 1);
  return r === 0 ? (x < 0 || Object.is(x, -0) ? -0 : 0) : r;
}
function $fcopysign(a, b) {
  const sb = b < 0 || Object.is(b, -0) || (b !== b && $nanSign(b));
  const m = Math.abs(a);
  return sb ? -m : m;
}
function $nanSign(x) { $fdv.setFloat64(0, x, true); return ($fdv.getUint32(4, true) >>> 31) === 1; }
// trunc/floor/ceil keep the sign of zero, as Math's do.
function $ftrunc(x) { return Math.trunc(x); }

// A float NaN keeps its 23 payload bits, its quiet bit among them, as a QUIET double NaN that carries them in its low word with the
// marker bit 0x40000 of the high word: DataView's getFloat32 widens through the hardware's conversion, which quiets a signalling NaN, and
// V8 quiets a signalling double NaN when it stores it in a double array (measured: compiler/tests/js/bench/nan.mjs), but keeps a quiet
// one's payload.
function $nan32(b) {
  $fdv.setUint32(4, ((b >>> 31) << 31 | 0x7ff80000 | 0x40000) >>> 0, true);
  $fdv.setUint32(0, b & 0x7fffff, true);
  return $fdv.getFloat64(0, true);
}
function $nan64to32(x) {
  $fdv.setFloat64(0, x, true);
  const hi = $fdv.getUint32(4, true), lo = $fdv.getUint32(0, true);
  let p;
  if ((hi & 0x7ffc0000) === 0x7ffc0000) p = lo & 0x7fffff;   // a float NaN, as $nan32 made it
  else p = ((hi & 0x7ffff) << 3) | (lo >>> 29) | 0x400000;   // a double NaN narrowed as fptrunc does: the top bits, quiet
  return ((hi >>> 31) << 31 | 0x7f800000 | p) >>> 0;
}
// fpext and fptrunc of a NaN as x86-64 does them (cvtss2sd, cvtsd2ss): the payload moves to the other end of the wider mantissa and the
// result is quiet.
function $fpext(x) {
  if (x === x) return x;
  const b = $nan64to32(x);
  $fdv.setUint32(4, ((b >>> 31) << 31 | 0x7ff80000 | ((b & 0x7fffff) >>> 3)) >>> 0, true);
  $fdv.setUint32(0, ((b & 7) << 29) >>> 0, true);
  return $fdv.getFloat64(0, true);
}
function $fptrunc(x) { return x === x ? Math.fround(x) : $nan32($nan64to32(x) | 0x400000); }
function $lf32(o) { const x = $D.getFloat32(o, true); return x === x ? x : $nan32($D.getUint32(o, true)); }
function $sf32(o, v) { if (v === v) $D.setFloat32(o, v, true); else $D.setUint32(o, $nan64to32(v), true); }

// Bitcast between non-aggregate types of one size: write as the source type, read as the target (tds of rt/mem.js).
const $bcbuf = new ArrayBuffer(1024 * 8);
const $bcD = new DataView($bcbuf);
function $bitcast(v, from, to) {
  $new8(); $put($bcD, 0, from, v);
  return $get($bcD, 0, to);
}
function $new8() { new Uint8Array($bcbuf).fill(0); }
