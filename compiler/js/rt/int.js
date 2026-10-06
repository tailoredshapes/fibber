// compiler/js/rt/int.js: integer helpers of the JS runtime. Representation (docs/design/js-backend.md, decision 1): i1 is 0 or 1; i8, i16
// and i32 are JS numbers holding the signed value; i64 is a BigInt holding the signed value. The translator inlines the common operations
// (`(a+b)|0`, `Math.imul`, `BigInt.asIntN(64, a+b)`); what needs a test or a loop is here.

// Division: a zero divisor, and the minimum divided by -1, raise SIGFPE as x86-64's idiv does (lIR leaves both undefined, spec 6.12).
function $sdiv32(a, b) { if (b === 0 || (a === -2147483648 && b === -1)) $signal('SIGFPE'); return (a / b) | 0; }
function $srem32(a, b) { if (b === 0 || (a === -2147483648 && b === -1)) $signal('SIGFPE'); return (a % b) | 0; }
function $udiv32(a, b) { if (b === 0) $signal('SIGFPE'); return ((a >>> 0) / (b >>> 0)) | 0; }
function $urem32(a, b) { if (b === 0) $signal('SIGFPE'); return ((a >>> 0) % (b >>> 0)) | 0; }
// i8 and i16 widen to i32 on x86-64 before dividing, so only a zero divisor faults; the result is cut back by the caller.
function $sdivn(a, b) { if (b === 0) $signal('SIGFPE'); return (a / b) | 0; }
function $sremn(a, b) { if (b === 0) $signal('SIGFPE'); return (a % b) | 0; }
const $MIN64 = -(2n ** 63n);
function $sdiv64(a, b) { if (b === 0n || (a === $MIN64 && b === -1n)) $signal('SIGFPE'); return a / b; }
function $srem64(a, b) { if (b === 0n || (a === $MIN64 && b === -1n)) $signal('SIGFPE'); return a % b; }
function $udiv64(a, b) { if (b === 0n) $signal('SIGFPE'); return BigInt.asIntN(64, BigInt.asUintN(64, a) / BigInt.asUintN(64, b)); }
function $urem64(a, b) { if (b === 0n) $signal('SIGFPE'); return BigInt.asIntN(64, BigInt.asUintN(64, a) % BigInt.asUintN(64, b)); }

// The signed value of the low `bits` bits of a number (bits <= 32) or a BigInt (bits 64), and the unsigned value.
function $sx(bits, v) { return bits === 1 ? (v & 1) : bits === 32 ? (v | 0) : (v << (32 - bits)) >> (32 - bits); }
function $ux(bits, v) { return bits === 32 ? (v >>> 0) : bits === 1 ? (v & 1) : v & ((1 << bits) - 1); }
function $u64(v) { return BigInt.asUintN(64, v); }

// `{ T, i1 }` of sadd/ssub/smul-overflow: the wrapped result and whether the exact result did not fit.
function $sov(op, bits, a, b) {
  if (bits === 64) {
    const r = op === 0 ? a + b : op === 1 ? a - b : a * b;
    const w = BigInt.asIntN(64, r);
    return [w, w === r ? 0 : 1];
  }
  if (bits === 1) { a = -a; b = -b; }
  const r = op === 0 ? a + b : op === 1 ? a - b : (bits === 32 ? Number(BigInt(a) * BigInt(b)) : a * b);
  const lo = -(2 ** (bits - 1)), hi = 2 ** (bits - 1) - 1;
  const ov = (r < lo || r > hi || (op === 2 && bits === 32 && !Number.isSafeInteger(r))) ? 1 : 0;
  let w;
  if (op === 2 && bits === 32) w = Math.imul(a, b); else w = $sx(bits, r);
  return [bits === 1 ? (w & 1) : w, ov];
}
// smul-overflow at 32 bits: the exact product of two i32 can exceed 2^53, so it is computed in BigInt above.
// The overflow intrinsics on vectors: { <N x iK>, <N x i1> }.
function $vsov(op, bits, a, b) {
  const r = a.map((x, i) => $sov(op, bits, x, b[i]));
  return [r.map((p) => p[0]), r.map((p) => p[1])];
}

function $ctpop(bits, v) {
  if (bits === 64) { let x = BigInt.asUintN(64, v), n = 0; while (x) { n += Number(x & 1n); x >>= 1n; } return BigInt(n); }
  let x = $ux(bits, v), n = 0;
  while (x) { n += x & 1; x >>>= 1; }
  return n;
}
// llvm.cttz / llvm.ctlz with is_zero_poison false: a zero has `bits` trailing and leading zeros.
function $cttz(bits, v) {
  let x = bits === 64 ? BigInt.asUintN(64, v) : BigInt($ux(bits, v));
  if (x === 0n) return bits === 64 ? 64n : bits;
  let n = 0; while ((x & 1n) === 0n) { n++; x >>= 1n; }
  return bits === 64 ? BigInt(n) : n;
}
function $ctlz(bits, v) {
  let x = bits === 64 ? BigInt.asUintN(64, v) : BigInt($ux(bits, v));
  let n = 0; for (let i = bits - 1; i >= 0 && ((x >> BigInt(i)) & 1n) === 0n; i--) n++;
  return bits === 64 ? BigInt(n) : n;
}
function $smin(bits, a, b) { return a < b ? a : b; }
function $smax(bits, a, b) { return a > b ? a : b; }
function $umin(bits, a, b) { return bits === 64 ? ($u64(a) < $u64(b) ? a : b) : ($ux(bits, a) < $ux(bits, b) ? a : b); }
function $umax(bits, a, b) { return bits === 64 ? ($u64(a) > $u64(b) ? a : b) : ($ux(bits, a) > $ux(bits, b) ? a : b); }
// llvm.abs with is_int_min_poison false: the minimum is its own absolute value.
function $abs(bits, a) {
  if (bits === 64) return a < 0n ? BigInt.asIntN(64, -a) : a;
  return a < 0 ? $sx(bits, -a) : a;
}

// fptosi/fptoui to iK: in range, the value toward zero; out of range (poison in lIR) the x86-64 answer, the "integer indefinite"
// 0x80000000 or 0x8000000000000000 of cvttsd2si, so that both builds of a program agree on the same mistake.
function $f2s(bits, x) {
  const t = Math.trunc(x);
  if (bits === 64) return (t >= -9223372036854775808 && t < 9223372036854775808) ? BigInt(t) : $MIN64;
  const r = (t >= -2147483648 && t < 2147483648) ? t : -2147483648;
  return $sx(bits, r);
}
function $f2u(bits, x) {
  const t = Math.trunc(x);
  if (bits === 64) {
    if (t >= 0 && t < 18446744073709551616) return BigInt.asIntN(64, BigInt(t));
    return (t > -9223372036854775809 && t < 0) ? BigInt(t) : $MIN64;
  }
  if (bits === 32) return (t >= 0 && t < 4294967296) ? (t | 0) : (t >= -9223372036854775808 && t < 9223372036854775808 ? Number(BigInt.asIntN(32, BigInt(t))) : 0);
  return $sx(bits, (t >= -2147483648 && t < 2147483648) ? t : -2147483648);
}
// fptosi-sat / fptoui-sat: clamped, NaN to 0.
function $f2ssat(bits, x) {
  if (x !== x) return bits === 64 ? 0n : 0;
  if (bits === 64) {
    if (x >= 9223372036854775807) return 2n ** 63n - 1n;
    if (x <= -9223372036854775808) return $MIN64;
    return BigInt(Math.trunc(x));
  }
  const lo = bits === 1 ? -1 : -(2 ** (bits - 1)), hi = bits === 1 ? 0 : 2 ** (bits - 1) - 1;
  const r = Math.max(lo, Math.min(hi, Math.trunc(x)));
  return bits === 1 ? r & 1 : r;
}
function $f2usat(bits, x) {
  if (x !== x || x <= 0) return bits === 64 ? 0n : 0;
  if (bits === 64) return x >= 18446744073709551615 ? -1n : BigInt.asIntN(64, BigInt(Math.trunc(x)));
  const hi = 2 ** bits - 1;
  return $sx(bits, Math.min(hi, Math.trunc(x)));
}
