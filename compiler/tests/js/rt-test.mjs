// compiler/tests/js/rt-test.mjs: unit tests of the JS runtime of lir2js (compiler/js/rt) against references that do not share its code:
//   fma (double): the exact a*b+c written as a decimal and read back by V8's correctly rounded string-to-double;
//   fma (float): round-to-odd in double, then Math.fround (Boldo and Melquiond: exact for a target of 24 bits from 53);
//   printf's conversions and strtof: glibc itself, through a C program built with `cc` from the same inputs.
// Run: node compiler/tests/js/rt-test.mjs [--count N]    exit 0 when every check holds; each failure is printed.
// A planted fault (RT_TEST_PLANT=fma|printf|strtof) breaks the function under test, to show the test can fail.
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const rt = path.join(here, '../../js/rt');
const files = ['core.js', 'int.js', 'float.js', 'mem.js', 'fmt.js', 'libc.js', 'stdio.js'];
const count = Number(process.argv[process.argv.indexOf('--count') + 1]) || 3000;
const tmp = fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'rt-test-'));
const plant = process.env.RT_TEST_PLANT || '';
let src = files.map((f) => fs.readFileSync(path.join(rt, f), 'utf8')).join('\n');
if (plant === 'fma') src = src.replace('(r > half || (r === half && (q & 1n)))', '(r >= half)');
if (plant === 'printf') src = src.replace("return (r2 > den || (r2 === den && (q & 1n))) ? q + 1n : q;", 'return r2 >= den ? q + 1n : q;');
if (plant === 'strtof') src = src.replace('v = single ? $decToF32(m[3]) : Number(m[3]);', 'v = single ? Math.fround(Number(m[3])) : Number(m[3]);');
src += '\nexport { $fma64, $fma32, $format, $cstr, $strto, $decompose };\n';
fs.writeFileSync(path.join(tmp, 'rt.mjs'), src);
process.env.FIB_JS_HEAP_MB = '64';
const R = await import(path.join(tmp, 'rt.mjs'));

let failures = 0, checks = 0;
function check(ok, what) { checks++; if (!ok) { failures++; if (failures <= 20) console.log('FAIL', what); } }

// ---------------------------------------------------------------- random operands with edge cases mixed in
let seed = 0x9e3779b97f4a7c15n;
function rnd64() { seed ^= seed << 13n; seed &= (1n << 64n) - 1n; seed ^= seed >> 7n; seed ^= seed << 17n; seed &= (1n << 64n) - 1n; return seed; }
const dv = new DataView(new ArrayBuffer(8));
function rndDouble(spread) {
  const r = rnd64();
  const exp = 1023 + Number(r % BigInt(2 * spread + 1)) - spread;
  dv.setBigUint64(0, (BigInt(exp & 0x7ff) << 52n) | (rnd64() & ((1n << 52n) - 1n)) | ((r >> 63n) << 63n));
  return dv.getFloat64(0);
}
const edges = [0, -0, 1, -1, 2 ** -1074, -(2 ** -1074), 2 ** -1022, 1.7976931348623157e308, 0.1, 1 / 3, Infinity, -Infinity, NaN];

// ---------------------------------------------------------------- fma, double
function exactDecimal(a, b, c) {
  const [ma, ea] = R.$decompose(a), [mb, eb] = R.$decompose(b), [mc, ec] = R.$decompose(c);
  let m = ma * mb, e = ea + eb;
  if (ec < e) { m = (m << BigInt(e - ec)) + mc; e = ec; } else m += mc << BigInt(ec - e);
  if (m === 0n) return null;
  return e >= 0 ? (m << BigInt(e)).toString() : `${m * 5n ** BigInt(-e)}e${e}`;   // m / 2^k = m * 5^k / 10^k
}
function fmaRef(a, b, c) {
  if (![a, b, c].every(Number.isFinite) || a === 0 || b === 0) return a * b + c;
  const d = exactDecimal(a, b, c);
  return d === null ? a * b + c : Number(d);
}
const same = (x, y) => Object.is(x, y) || (x !== x && y !== y);
for (let i = 0; i < count; i++) {
  const spread = i % 3 === 0 ? 1000 : 60;
  const a = i < 169 ? edges[i % 13] : rndDouble(spread), b = i < 169 ? edges[Math.floor(i / 13)] : rndDouble(spread);
  // c near -a*b makes the cancellation that a two-rounding a*b+c gets wrong
  const c = i % 2 ? -(a * b) * (1 + rndDouble(0) * 2 ** -40) : rndDouble(spread);
  const got = R.$fma64(a, b, c), want = fmaRef(a, b, c);
  check(same(got, want), `fma64(${a}, ${b}, ${c}) = ${got}, want ${want}`);
}
// The case one rounding gets right and two get wrong: (1 + 2^-52)(1 - 2^-52) - 1 = -2^-104 exactly; a*b rounds to 1 first.
check(R.$fma64(1 + 2 ** -52, 1 - 2 ** -52, -1) === -(2 ** -104), 'fma64 keeps the bits a*b+c loses');

// ---------------------------------------------------------------- fma, float
function twoSum(a, b) { const s = a + b, bb = s - a, err = (a - (s - bb)) + (b - bb); return [s, err]; }
function nextToward(x, dir) { dv.setFloat64(0, x); let bits = dv.getBigUint64(0); bits += (x > 0) === (dir > 0) ? 1n : -1n; dv.setBigUint64(0, bits); return dv.getFloat64(0); }
function fma32Ref(a, b, c) {
  if (![a, b, c].every(Number.isFinite)) return Math.fround(a * b + c);
  const p = a * b;                                         // exact: 24 + 24 bits
  let [s, err] = twoSum(p, c);
  if (err !== 0 && s !== 0) { dv.setFloat64(0, s); if ((dv.getBigUint64(0) & 1n) === 0n) s = nextToward(s, err); }   // round to odd
  return Math.fround(s);
}
for (let i = 0; i < count; i++) {
  const a = Math.fround(rndDouble(30)), b = Math.fround(rndDouble(30));
  const c = i % 2 ? Math.fround(-(a * b) * (1 + rndDouble(0) * 2 ** -20)) : Math.fround(rndDouble(30));
  const got = R.$fma32(a, b, c), want = fma32Ref(a, b, c);
  check(same(got, want), `fma32(${a}, ${b}, ${c}) = ${got}, want ${want}`);
}

// ---------------------------------------------------------------- printf and strtof against glibc
const formats = ['%.0f', '%.1f', '%.3f', '%f', '%.17g', '%g', '%.3g', '%e', '%.0e', '%.10e', '%#.0f', '%+.2e', '%12.4f', '%-12.3g|', '%08.3f', '%G', '%.20f'];
const values = [0.5, 1.5, 2.5, -0.5, 0.125, 1e-5, 123456789, 9.9995, 1e300, 5e-324, -0, 0, 0.1, 2 / 3, 1e15, 1e16, 1e21, 999999.5, 0.000123456];
for (let i = 0; values.length < 120; i++) values.push(rndDouble(i % 2 ? 30 : 300));
// 1 + 2^-24 + 2^-60: a double rounds it to the midpoint 1 + 2^-24, which then rounds to even (1); once rounded, it is 1 + 2^-23.
const decs = [`${(2n ** 60n + 2n ** 36n + 1n) * 5n ** 60n}e-60`, '1.1', '0.1', '3.4028235e38', '3.4028236e38', '1e-45', '7e-46', '1.17549435e-38', '16777217', '0.333333343267', '1e39', '2.5e-46'];
for (let i = 0; decs.length < 80; i++) decs.push(`${(rnd64() % 100000000n)}e${Number(rnd64() % 90n) - 50}`);
const csrc = `#include <stdio.h>\n#include <stdlib.h>\nint main(void){\n` +
  formats.flatMap((f) => values.map((v) => `printf("${f}\\n", (double)(${Object.is(v, -0) ? '-0.0' : v === Infinity ? '1e999' : v.toPrecision(17)}));`)).join('\n') +
  '\n' + decs.map((d) => `{float x = strtof("${d}", 0); unsigned u; __builtin_memcpy(&u, &x, 4); printf("%08x\\n", u);}`).join('\n') + '\nreturn 0;}\n';
fs.writeFileSync(path.join(tmp, 'ref.c'), csrc);
let glibc = null;
try { execFileSync('cc', ['-O0', '-o', path.join(tmp, 'ref'), path.join(tmp, 'ref.c')]); glibc = execFileSync(path.join(tmp, 'ref')).toString().split('\n'); }
catch (e) { console.log('cc is not available: printf and strtof are not checked'); failures++; }
if (glibc) {
  let k = 0;
  for (const f of formats) for (const v of values) {
    const got = Buffer.from(R.$format(R.$cstr(Buffer.from(f + '\0').subarray(0, f.length)), [v])).toString();
    check(got === glibc[k], `printf("${f}", ${v}) = "${got}", glibc "${glibc[k]}"`); k++;
  }
  for (const d of decs) {
    const x = R.$strto(R.$cstr(Buffer.from(d)), 0, true);
    dv.setFloat32(0, x); const got = dv.getUint32(0).toString(16).padStart(8, '0');
    check(got === glibc[k], `strtof("${d}") = ${got}, glibc ${glibc[k]}`); k++;
  }
}
fs.rmSync(tmp, { recursive: true, force: true });
console.log(`${checks} checks, ${failures} failed`);
process.exit(failures ? 1 : 0);
