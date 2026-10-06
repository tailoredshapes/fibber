// compiler/js/rt/fmt.js: printf's formatting and strtod, as glibc does them. Floating-point conversions are exact: the decimal digits come
// from the exact binary value through BigInt, rounded to nearest with ties to even (glibc's rule; Number.prototype.toFixed rounds ties
// away from zero and would print 0.5 with %.0f as 1 where glibc prints 0).

const $P10 = [];
function $pow10(n) { return $P10[n] ?? ($P10[n] = 10n ** BigInt(n)); }
// round(num / den), ties to even.
function $divRound(num, den) {
  const q = num / den, r2 = (num % den) * 2n;
  return (r2 > den || (r2 === den && (q & 1n))) ? q + 1n : q;
}
// round(|x| * 10^s) for a finite x, as a BigInt.
function $scaled(x, s) {
  const [m0, e] = $decompose(x);
  const m = m0 < 0n ? -m0 : m0;
  let num = m, den = 1n;
  if (s >= 0) num *= $pow10(s); else den *= $pow10(-s);
  if (e >= 0) num <<= BigInt(e); else den <<= BigInt(-e);
  return $divRound(num, den);
}
// The digits of %f: |x| with `prec` decimals.
function $fixed(x, prec) {
  let d = $scaled(x, prec).toString();
  if (prec === 0) return d;
  d = d.padStart(prec + 1, '0');
  return d.slice(0, d.length - prec) + '.' + d.slice(d.length - prec);
}
// The digits and exponent of %e: [first digit + '.' + prec digits, exponent].
function $expo(x, prec) {
  const ax = Math.abs(x);
  if (ax === 0) return [prec > 0 ? '0.' + '0'.repeat(prec) : '0', 0];
  // k exactly: 10^k <= |x| < 10^(k+1), decided on the exact value (Math.log10 may be off by one near a power of ten), and only then
  // the rounding, which may carry into one more digit.
  let k = Math.floor(Math.log10(ax));
  while ($cmpPow10(x, k) < 0) k--;
  while ($cmpPow10(x, k + 1) >= 0) k++;
  let q = $scaled(x, prec - k);
  if (q === $pow10(prec + 1)) { q = $pow10(prec); k++; }
  const s = q.toString();
  return [prec > 0 ? s[0] + '.' + s.slice(1) : s, k];
}
// The sign of |x| - 10^k, exactly.
function $cmpPow10(x, k) {
  const [m0, e] = $decompose(x);
  let a = m0 < 0n ? -m0 : m0, b = 1n;
  if (e >= 0) a <<= BigInt(e); else b <<= BigInt(-e);
  if (k >= 0) b *= $pow10(k); else a *= $pow10(-k);
  return a < b ? -1 : a > b ? 1 : 0;
}
function $expStr(k, up) { return (up ? 'E' : 'e') + (k < 0 ? '-' : '+') + String(Math.abs(k)).padStart(2, '0'); }
function $stripZeros(s) { if (s.indexOf('.') < 0) return s; s = s.replace(/0+$/, ''); return s.endsWith('.') ? s.slice(0, -1) : s; }

function $floatBody(x, conv, prec, alt) {
  const up = conv === 'F' || conv === 'E' || conv === 'G';
  if (!Number.isFinite(x)) return x !== x ? (up ? 'NAN' : 'nan') : (up ? 'INF' : 'inf');
  if (prec < 0) prec = 6;
  if (conv === 'f' || conv === 'F') { const s = $fixed(x, prec); return alt && prec === 0 ? s + '.' : s; }
  if (conv === 'e' || conv === 'E') { const [m, k] = $expo(x, prec); return (alt && prec === 0 ? m + '.' : m) + $expStr(k, up); }
  const P = prec === 0 ? 1 : prec;
  const [, X] = $expo(x, P - 1);
  let s;
  if (P > X && X >= -4) s = $fixed(x, P - 1 - X);
  else { const [m] = $expo(x, P - 1); s = alt ? m : $stripZeros(m); return s + $expStr(X, up); }
  return alt ? s : $stripZeros(s);
}
function $negative(x) { return x < 0 || Object.is(x, -0) || (x !== x && $nanSign(x)); }

// printf: the bytes of the formatted text. fmt is the format's address, args the variadic values.
function $format(fmt, args) {
  const f = $cbytes(fmt);
  const out = []; let ai = 0, i = 0;
  const next = () => args[ai++];
  while (i < f.length) {
    const c = f[i++];
    if (c !== 37) { out.push(c); continue; }
    let flags = '', width = -1, prec = -1, len = '';
    while (i < f.length && '-+ #0'.includes(String.fromCharCode(f[i]))) flags += String.fromCharCode(f[i++]);
    if (f[i] === 42) { i++; width = Number(next()); if (width < 0) { flags += '-'; width = -width; } }
    else { let w = ''; while (f[i] >= 48 && f[i] <= 57) w += String.fromCharCode(f[i++]); if (w) width = Number(w); }
    if (f[i] === 46) {
      i++;
      if (f[i] === 42) { i++; prec = Number(next()); if (prec < 0) prec = -1; }
      else { let p = ''; while (f[i] >= 48 && f[i] <= 57) p += String.fromCharCode(f[i++]); prec = p ? Number(p) : 0; }
    }
    while (i < f.length && 'hlLqjzt'.includes(String.fromCharCode(f[i]))) len += String.fromCharCode(f[i++]);
    const conv = String.fromCharCode(f[i++]);
    const piece = $conv(conv, flags, width, prec, len, next);
    for (const b of piece) out.push(b);
  }
  return Uint8Array.from(out);
}
function $pad(body, sign, flags, width, zeroOk) {
  const n = sign.length + body.length;
  if (width <= n) return sign + body;
  if (flags.includes('-')) return sign + body + ' '.repeat(width - n);
  if (flags.includes('0') && zeroOk) return sign + '0'.repeat(width - n) + body;
  return ' '.repeat(width - n) + sign + body;
}
function $intArg(v, len, signed) {
  const bits = len === 'hh' ? 8 : len === 'h' ? 16 : (len === '' ? 32 : 64);
  const b = typeof v === 'bigint' ? v : BigInt(Math.trunc(Number(v)));
  return signed ? BigInt.asIntN(bits, b) : BigInt.asUintN(bits, b);
}
function $conv(conv, flags, width, prec, len, next) {
  const enc = (s) => Buffer.from(s, 'latin1');
  switch (conv) {
    case '%': return enc('%');
    case 'd': case 'i': case 'u': case 'x': case 'X': case 'o': {
      const signed = conv === 'd' || conv === 'i';
      let v = $intArg(next(), len, signed);
      const neg = v < 0n; if (neg) v = -v;
      let body = v.toString(conv === 'o' ? 8 : conv === 'u' || signed ? 10 : 16);
      if (conv === 'X') body = body.toUpperCase();
      if (prec === 0 && v === 0n) body = '';
      if (prec > body.length) body = '0'.repeat(prec - body.length) + body;
      if (flags.includes('#') && conv === 'o' && !body.startsWith('0')) body = '0' + body;
      let sign = neg ? '-' : signed && flags.includes('+') ? '+' : signed && flags.includes(' ') ? ' ' : '';
      if (flags.includes('#') && v !== 0n && (conv === 'x' || conv === 'X')) sign = conv === 'x' ? '0x' : '0X';
      return enc($pad(body, sign, flags, width, prec < 0));
    }
    case 'c': {
      const ch = Uint8Array.of(Number($intArg(next(), 'hh', false))), sp = enc(' '.repeat(Math.max(0, width - 1)));
      return flags.includes('-') ? Buffer.concat([ch, sp]) : Buffer.concat([sp, ch]);
    }
    case 's': {
      const p = Number(next());
      let b = p === 0 ? enc('(null)') : Buffer.from($cbytes(p));
      if (prec >= 0) b = b.subarray(0, prec);
      const padn = Math.max(0, width - b.length);
      return flags.includes('-') ? Buffer.concat([b, enc(' '.repeat(padn))]) : Buffer.concat([enc(' '.repeat(padn)), b]);
    }
    case 'p': { const p = Number(next()); return enc($pad(p === 0 ? '(nil)' : '0x' + p.toString(16), '', flags, width, false)); }
    case 'f': case 'F': case 'e': case 'E': case 'g': case 'G': {
      const x = Number(next());
      const sign = $negative(x) ? '-' : flags.includes('+') ? '+' : flags.includes(' ') ? ' ' : '';
      return enc($pad($floatBody(x, conv, prec, flags.includes('#')), sign, flags, width, Number.isFinite(x)));
    }
    default: $unsupported(`printf conversion %${conv}`);
  }
}

// strtod / strtof: the longest prefix that is a number; endptr gets its end. Hexadecimal floats are not read (unsupported).
const $NUMRE = /^[ \t\n\v\f\r]*([+-]?)(?:(inf(?:inity)?|nan(?:\([0-9A-Za-z_]*\))?)|((?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?))/i;
function $strto(s, endp, single) {
  const bytes = $cbytes(s);
  const text = Buffer.from(bytes).toString('latin1');
  const m = $NUMRE.exec(text);
  let v = 0, used = 0;
  if (m && /^[ \t\n\v\f\r]*[+-]?0[xX]/.test(text)) $unsupported('strtod of a hexadecimal number');
  if (m) {
    used = m[0].length;
    const neg = m[1] === '-';
    if (m[2]) v = /^n/i.test(m[2]) ? NaN : Infinity;
    else {
      v = single ? $decToF32(m[3]) : Number(m[3]);
      const nonzero = /[1-9]/.test(m[3].replace(/[eE].*$/, ''));
      if (!Number.isFinite(v) || (v === 0 && nonzero)) $setErrno(34);
    }
    if (neg) v = -v;
  }
  if (endp !== 0) $sp(endp - $BIAS, s + used);
  return v;
}
// A decimal text rounded once to float (Math.fround(Number(t)) would round twice).
function $decToF32(t) {
  const mm = /^([0-9]*)\.?([0-9]*)(?:[eE]([+-]?[0-9]+))?$/.exec(t);
  const digits = (mm[1] + mm[2]).replace(/^0+/, '') || '0';
  let e10 = (mm[3] ? Number(mm[3]) : 0) - mm[2].length;
  const D = BigInt(digits);
  if (D === 0n) return 0;
  if (e10 > 60) return Infinity;
  if (e10 >= 0) return Math.fround($roundTo(D * $pow10(e10), 0, 24, -149));
  if (e10 < -400) return 0;
  const den = $pow10(-e10);
  const s = Math.max(0, den.toString(2).length - D.toString(2).length + 60);
  const num = D << BigInt(s), q = num / den, sticky = num % den === 0n ? 0n : 1n;
  return Math.fround($roundTo((q << 1n) | sticky, -s - 1, 24, -149));
}
