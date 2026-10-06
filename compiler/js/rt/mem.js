// compiler/js/rt/mem.js: typed access to memory for the types the translator does not inline (aggregates, vectors), and the operations
// that need a loop: masked loads and stores, gather and scatter, atomics, bitcasts. A type descriptor ("td") is made by the translator,
// with the layout of LLVM's x86-64 data layout already computed: { k, size } for a scalar, { k: 'v', n, e, size } for a vector,
// { k: 's', size, f: [[offset, td]..] } for a struct, { k: 'a', n, e, stride, size } for an array. A vector of i1 is stored bit-packed,
// as LLVM stores it. Values: a scalar as in rt/int.js; a vector, struct or array as a JS array.
const $i1 = { k: 'i1', size: 1 }, $i8 = { k: 'i8', size: 1 }, $i16 = { k: 'i16', size: 2 }, $i32 = { k: 'i32', size: 4 };
const $i64 = { k: 'i64', size: 8 }, $f32 = { k: 'f32', size: 4 }, $f64 = { k: 'f64', size: 8 }, $ptr = { k: 'ptr', size: 8 };
function $vec(n, e, size) { return { k: 'v', n, e, size }; }
function $struct(size, f) { return { k: 's', size, f }; }
function $arr(n, e, stride, size) { return { k: 'a', n, e, stride, size }; }

function $get(dv, o, td) {
  switch (td.k) {
    case 'i1': return dv.getUint8(o) & 1;
    case 'i8': return dv.getInt8(o);
    case 'i16': return dv.getInt16(o, true);
    case 'i32': return dv.getInt32(o, true);
    case 'i64': return dv.getBigInt64(o, true);
    case 'f32': { const x = dv.getFloat32(o, true); return x === x ? x : $nan32(dv.getUint32(o, true)); }
    case 'f64': return dv.getFloat64(o, true);
    case 'ptr': return dv.getUint32(o, true) + dv.getUint32(o + 4, true) * 4294967296;
    case 'v': {
      const r = new Array(td.n);
      if (td.e.k === 'i1') { for (let i = 0; i < td.n; i++) r[i] = (dv.getUint8(o + (i >> 3)) >> (i & 7)) & 1; return r; }
      for (let i = 0; i < td.n; i++) r[i] = $get(dv, o + i * td.e.size, td.e);
      return r;
    }
    case 's': return td.f.map(([off, t]) => $get(dv, o + off, t));
    case 'a': { const r = new Array(td.n); for (let i = 0; i < td.n; i++) r[i] = $get(dv, o + i * td.stride, td.e); return r; }
  }
}
function $put(dv, o, td, v) {
  switch (td.k) {
    case 'i1': case 'i8': dv.setInt8(o, v); return;
    case 'i16': dv.setInt16(o, v, true); return;
    case 'i32': dv.setInt32(o, v, true); return;
    case 'i64': dv.setBigInt64(o, v, true); return;
    case 'f32': if (v === v) dv.setFloat32(o, v, true); else dv.setUint32(o, $nan64to32(v), true); return;
    case 'f64': dv.setFloat64(o, v, true); return;
    case 'ptr': dv.setUint32(o, v >>> 0, true); dv.setUint32(o + 4, Math.floor(v / 4294967296) >>> 0, true); return;
    case 'v':
      if (td.e.k === 'i1') {
        const nb = (td.n + 7) >> 3;
        for (let b = 0; b < nb; b++) { let x = 0; for (let i = 0; i < 8 && b * 8 + i < td.n; i++) x |= (v[b * 8 + i] & 1) << i; dv.setUint8(o + b, x); }
        return;
      }
      for (let i = 0; i < td.n; i++) $put(dv, o + i * td.e.size, td.e, v[i]);
      return;
    case 's': for (let i = 0; i < td.f.length; i++) $put(dv, o + td.f[i][0], td.f[i][1], v[i]); return;
    case 'a': for (let i = 0; i < td.n; i++) $put(dv, o + i * td.stride, td.e, v[i]); return;
  }
}
function $ld(p, td) { return $get($D, p - $BIAS, td); }
function $st(p, td, v) { $put($D, p - $BIAS, td, v); }
function $zero(td) {
  switch (td.k) {
    case 'i64': return 0n;
    case 'v': return Array.from({ length: td.n }, () => $zero(td.e));
    case 's': return td.f.map(([, t]) => $zero(t));
    case 'a': return Array.from({ length: td.n }, () => $zero(td.e));
    default: return 0;
  }
}

// Masked loads and stores: a lane whose mask bit is 0 is not touched at all.
function $mload(td, p, mask, pass) { const e = td.e; return pass.map((x, i) => mask[i] ? $ld(p + i * e.size, e) : x); }
function $mstore(td, v, p, mask) { const e = td.e; for (let i = 0; i < td.n; i++) if (mask[i]) $st(p + i * e.size, e, v[i]); }
function $gather(td, ptrs, mask, pass) { return pass.map((x, i) => mask[i] ? $ld(ptrs[i], td.e) : x); }
function $scatter(td, v, ptrs, mask) { for (let i = 0; i < td.n; i++) if (mask[i]) $st(ptrs[i], td.e, v[i]); }

// Atomics, single-threaded: a plain read and write. op is the RmwOp word.
function $rmw(op, td, p, v) {
  const old = $ld(p, td);
  const b64 = td.k === 'i64', bits = td.k === 'i8' ? 8 : td.k === 'i16' ? 16 : 32;
  const w = (x) => b64 ? BigInt.asIntN(64, x) : $sx(bits, x);
  let n;
  switch (op) {
    case 'xchg': n = v; break;
    case 'add': n = w(old + v); break;
    case 'sub': n = w(old - v); break;
    case 'and': n = old & v; break;
    case 'nand': n = b64 ? BigInt.asIntN(64, ~(old & v)) : $sx(bits, ~(old & v)); break;
    case 'or': n = old | v; break;
    case 'xor': n = old ^ v; break;
    case 'max': n = old > v ? old : v; break;
    case 'min': n = old < v ? old : v; break;
    case 'umax': n = b64 ? $umax(64, old, v) : $umax(bits, old, v); break;
    case 'umin': n = b64 ? $umin(64, old, v) : $umin(bits, old, v); break;
    case 'fadd': n = td.k === 'f32' ? Math.fround(old + v) : old + v; break;
    case 'fsub': n = td.k === 'f32' ? Math.fround(old - v) : old - v; break;
    case 'fmax': n = $fmaxnum(old, v); break;
    case 'fmin': n = $fminnum(old, v); break;
  }
  $st(p, td, n);
  return old;
}
function $cmpxchg(td, p, expected, nv) {
  const old = $ld(p, td);
  if (old === expected) { $st(p, td, nv); return [old, 1]; }
  return [old, 0];
}

// Lane-wise helpers for vector operations: the translator passes the scalar operation as a function.
function $vm1(f, a) { const r = new Array(a.length); for (let i = 0; i < a.length; i++) r[i] = f(a[i]); return r; }
function $vm2(f, a, b) { const r = new Array(a.length); for (let i = 0; i < a.length; i++) r[i] = f(a[i], b[i]); return r; }
function $vm3(f, a, b, c) { const r = new Array(a.length); for (let i = 0; i < a.length; i++) r[i] = f(a[i], b[i], c[i]); return r; }
function $vsel(c, a, b) { return a.map((x, i) => c[i] ? x : b[i]); }
function $vins(v, x, i) { const r = v.slice(); if (i >= 0 && i < r.length) r[i] = x; return r; }
function $vext(v, i) { return v[i]; }
function $shuffle(a, b, m) { const n = a.length; return m.map((k) => k < n ? a[k] : b[k - n]); }
function $ins(agg, v, path) {
  const r = agg.slice();
  if (path.length === 1) r[path[0]] = v; else r[path[0]] = $ins(agg[path[0]], v, path.slice(1));
  return r;
}
function $fold(f, v) { let a = v[0]; for (let i = 1; i < v.length; i++) a = f(a, v[i]); return a; }
function $foldFrom(f, a, v) { for (let i = 0; i < v.length; i++) a = f(a, v[i]); return a; }
// A store whose value and pointer both have effects, evaluated in lIR's order (value first).
function $stv(v, p, td) { $st(p, td, v); }
// getelementptr with a vector index: one address per lane.
function $vgep(p, idx, size) { return idx.map((i) => p + Number(i) * size); }
