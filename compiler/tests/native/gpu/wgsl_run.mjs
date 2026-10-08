// compiler/tests/native/gpu/wgsl_run.mjs: runs the WGSL of fib.gpu's atomics and reductions on a WebGPU device (Dawn through the `webgpu` npm package of
// scripts/fetch-webgpu-tools.sh) and compares with the CPU, as cuda_run.py does for PTX. The launch follows the binding model of native.wgsl: group 0, binding 0 the
// uniform of scalar parameters (4 bytes each), binding 1 the trap flag, bindings 2.. the pointer parameters; the workgroup size is the override constant wg_x.
//   node wgsl_run.mjs reduce REDUCE.wgsl REF.txt | atomics ATOMICS.wgsl | branch BRANCH.wgsl | badgrid REDUCE.wgsl | warp WARP.wgsl REF.txt | probe WARP.wgsl | f32atomics F32.wgsl
// One line per check, `ok ...` or `FAIL ...`. Exit 0 when every check holds, 1 on a failure, 3 when there is no adapter or no package (the caller skips), 4 when the module says
// `// fib.requires: subgroups` and the adapter lacks the feature (GPU-5: rejected by name; FIB_WEBGPU_NO_SUBGROUPS=1 plays an adapter without it).
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";

const dir = process.env.FIB_WEBGPU_NODE ?? `${homedir()}/.cache/fibber-scratch/tools/webgpu-node`;
let gpu;
try { const { create, globals } = createRequire(dir + "/")("webgpu"); Object.assign(globalThis, globals); gpu = create([]); } catch { process.exit(3); }
const adapter = await gpu.requestAdapter();
if (!adapter) process.exit(3);
const [mode, a, b] = process.argv.slice(2);
const needSub = /^\/\/ fib\.requires: subgroups/m.test(readFileSync(a, "utf8"));
if (needSub && (!adapter.features.has("subgroups") || process.env.FIB_WEBGPU_NO_SUBGROUPS)) {
  console.log("rejected: the module requires the WebGPU `subgroups` feature (it uses warp shuffles) and this adapter does not have it");
  process.exit(4);
}
const device = await adapter.requestDevice({ requiredFeatures: needSub ? ["subgroups"] : [] });
let bad = 0;
const check = (ok, text) => { console.log((ok ? "ok   " : "FAIL ") + text); if (!ok) bad++; };

class Module {
  constructor(path) {
    const code = readFileSync(path, "utf8");
    this.code = code;
    this.nbufs = (code.match(/var<storage, read_write> buf\d+/g) ?? []).length;
    this.module = device.createShaderModule({ code });
    const entries = [{ binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
                     { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } }];
    for (let i = 0; i < this.nbufs; i++) entries.push({ binding: 2 + i, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } });
    this.bgl = device.createBindGroupLayout({ entries });
    this.layout = device.createPipelineLayout({ bindGroupLayouts: [this.bgl] });
  }
  buffer(typed) {
    const b = device.createBuffer({ size: Math.max(16, (typed.byteLength + 3) & ~3), usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(b, 0, typed.buffer, typed.byteOffset, typed.byteLength);
    return b;
  }
  async read(b, n, Type) {
    const st = device.createBuffer({ size: n * 4, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
    const enc = device.createCommandEncoder();
    enc.copyBufferToBuffer(b, 0, st, 0, n * 4);
    device.queue.submit([enc.finish()]);
    await st.mapAsync(GPUMapMode.READ);
    const out = new Type(st.getMappedRange().slice(0));
    st.unmap();
    return out;
  }
  // args: ["p", GPUBuffer] or ["i", int]. Returns whether the kernel trapped.
  async launch(name, grid, block, ...args) {
    const params = new Int32Array(16);
    let k = 0;
    const bufs = [];
    for (const [kind, v] of args) { if (kind === "i") params[k++] = v; else bufs.push(v); }
    while (bufs.length < this.nbufs) bufs.push(this.buffer(new Int32Array(4)));
    const pbuf = device.createBuffer({ size: 64, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(pbuf, 0, params);
    const flag = this.buffer(new Uint32Array(4));
    const pipe = device.createComputePipeline({ layout: this.layout, compute: { module: this.module, entryPoint: name, constants: { wg_x: block } } });
    const bg = device.createBindGroup({ layout: this.bgl, entries: [{ binding: 0, resource: { buffer: pbuf } }, { binding: 1, resource: { buffer: flag } },
      ...bufs.map((b, i) => ({ binding: 2 + i, resource: { buffer: b } }))] });
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipe); pass.setBindGroup(0, bg); pass.dispatchWorkgroups(grid);
    pass.end();
    device.queue.submit([enc.finish()]);
    return (await this.read(flag, 1, Uint32Array))[0] !== 0;
  }
}
const P = (v) => ["p", v], I = (v) => ["i", v];

function lcg(n, seed) {
  const out = new Array(n); let s = BigInt(seed);
  for (let i = 0; i < n; i++) { s = (s * 1664525n + 1013904223n) % 4294967296n; out[i] = Number(s); }
  return out;
}
const f32bits = (x) => new Int32Array(new Float32Array([x]).buffer)[0];

async function reduceMode(path, refPath) {
  const refs = new Map();
  for (const line of readFileSync(refPath, "utf8").split("\n")) {
    const w = line.trim().split(/\s+/);
    if (w[0] === "ref") refs.set(`${w[1]} ${w[2]} ${w[3]} ${w[4]}`, w.slice(5));
  }
  const m = new Module(path);
  const keys = [...refs.keys()].map((k) => k.split(" ")).sort((a, b) => Number(b[1]) - Number(a[1]) || Number(a[2]) - Number(b[2]) || a[0].localeCompare(b[0]));
  for (const [name, ns, bs, ks] of keys) {
    if (name === "sum-f32") continue;
    const n = Number(ns), blocks = Number(bs), block = Number(ks);
    const want = refs.get(`${name} ${ns} ${bs} ${ks}`);
    const s = lcg(n, n);
    const xi = Int32Array.from(s, (v) => v - 2 ** 31);
    const xf = Float32Array.from(s, (v) => Math.fround((v >>> 8) / 2 ** 23 - 1));
    const tag = `${name} n=${n} grid=${blocks} block=${block}`;
    if (["sum-i32", "max-i32", "min-i32"].includes(name)) {
      const ident = { "sum-i32": 0, "max-i32": -(2 ** 31), "min-i32": 2 ** 31 - 1 }[name];
      const out = m.buffer(new Int32Array([ident]));
      const trapped = await m.launch("reduce_" + name.replace("-", "_"), blocks, block, P(m.buffer(xi)), P(out), I(n), I(blocks));
      const got = (await m.read(out, 1, Int32Array))[0];
      check(!trapped && got === Number(want[0]), `${tag}: device ${got}, fibber's CPU ${want[0]}`);
    } else if (name === "max-f32") {
      const out = m.buffer(new Float32Array([-3.4028235e38]));
      const trapped = await m.launch("reduce_max_f32", blocks, block, P(m.buffer(xf)), P(out), I(n), I(blocks));
      const got = f32bits((await m.read(out, 1, Float32Array))[0]);
      check(!trapped && got === Number(want[0]), `${tag}: device bits ${got}, CPU bits ${want[0]}`);
    } else if (name === "partials") {
      const out = m.buffer(new Float32Array(blocks));
      const trapped = await m.launch("reduce_partials_f32", blocks, block, P(m.buffer(xf)), P(out), I(n), I(blocks));
      const got = Array.from(await m.read(out, blocks, Float32Array), f32bits);
      const exp = want[0].split(",").map(Number);
      check(!trapped && got.length === exp.length && got.every((v, i) => v === exp[i]), `${tag}: ${blocks} partials bit for bit with the CPU's tree order`);
      if (m.code.includes("fn reduce_sum_f32(")) { // GPU-5: the atomic f32 sum, a compare-and-exchange loop in WGSL
        const po = m.buffer(new Float32Array([0]));
        const tr = await m.launch("reduce_sum_f32", blocks, block, P(m.buffer(xf)), P(po), I(n), I(blocks));
        const sref = refs.get(`sum-f32 ${ns} ${bs} ${ks}`), cpu = new Float32Array(new Int32Array([Number(sref[0])]).buffer)[0], bound = Number(sref[1]);
        const g = (await m.read(po, 1, Float32Array))[0];
        const tight = Math.min(bound, 0.1); // the documented bound is loose for this data (3e4 against a sum of 276); the measured differences are about 5e-3
        check(!tr && Math.abs(g - cpu) <= tight, `sum-f32 n=${n} grid=${blocks} block=${block}: device ${g}, CPU ${cpu}, |diff| ${Math.abs(g - cpu).toExponential(2)} within ${tight} (documented bound ${bound.toExponential(2)})`);
      }
    }
  }
}

async function atomicsMode(path) {
  const m = new Module(path), n = 100000, s = lcg(n, 5), grid = Math.ceil(n / 256);
  const data = Int32Array.from(s, (v) => (v >>> 12) & 15);
  let out = m.buffer(new Int32Array(16));
  await m.launch("hist", grid, 256, P(m.buffer(data)), P(out), I(n));
  let got = Array.from(await m.read(out, 16, Int32Array));
  const want = Array.from({ length: 16 }, (_, k) => data.filter((v) => v === k).length);
  check(got.every((v, i) => v === want[i]), `hist n=${n}: 16 bins equal the CPU's count (${got.slice(0, 3)}..)`);
  const xs = Int32Array.from(s, (v) => v - 2 ** 31), limit = 1000000;
  out = m.buffer(new Int32Array([0, -(2 ** 31), 2 ** 31 - 1, 0]));
  await m.launch("tally", grid, 256, P(m.buffer(xs)), P(out), I(n), I(limit));
  got = Array.from(await m.read(out, 4, Int32Array));
  const umax = Math.max(...Array.from(xs, (x) => x >>> 0)) | 0;
  const w4 = [xs.filter((x) => x > limit).length, Math.max(...xs), Math.min(...xs), umax];
  check(got.every((v, i) => v === w4[i]), `tally n=${n}: add, max, min, umax [${got}] equal the CPU's [${w4}]`);
  for (const k of [n, 5]) {
    const slots = m.buffer(new Int32Array(8)), won = m.buffer(new Int32Array(4));
    await m.launch("claim", Math.ceil(k / 256), 256, P(slots), P(won), I(k));
    const w = (await m.read(won, 1, Int32Array))[0], sl = Array.from(await m.read(slots, 8, Int32Array));
    check(w === Math.min(k, 8) && sl.every((v, i) => v === 0 || (v - 1) % 8 === i) && sl.filter((v) => v).length === Math.min(k, 8),
          `claim n=${k}: compare-and-swap gave ${w} winners, one per slot, each slot holds a thread of its own residue`);
  }
  const blocks = Math.ceil(n / 128);
  out = m.buffer(new Int32Array(blocks).fill(-(2 ** 31)));
  await m.launch("block_max", blocks, 128, P(m.buffer(xs)), P(out), I(n));
  got = Array.from(await m.read(out, blocks, Int32Array));
  const wb = Array.from({ length: blocks }, (_, b) => Math.max(...xs.slice(b * 128, (b + 1) * 128)));
  check(got.every((v, i) => v === wb[i]), `block-max n=${n}: ${blocks} block maxima through workgroup atomics equal the CPU's`);
}

async function badgridMode(path) {
  const m = new Module(path);
  const out = m.buffer(new Int32Array([0]));
  const trapped = await m.launch("reduce_sum_i32", 4, 1, P(m.buffer(new Int32Array([1, 2, 3, 4]))), P(out), I(4), I(3));
  check(trapped, "a wrong blocks argument (3 for a grid of 4) sets the trap flag the driver reads");
}

// ---- GPU-5: warp shuffles. The grid reductions by shuffle against the same reference lines (the sum of the partials within the documented bound), the primitives against a model of the
// warp, and a block smaller than the warp ending in a device trap.
async function warpMode(path, refPath) {
  const refs = new Map();
  for (const line of readFileSync(refPath, "utf8").split("\n")) {
    const w = line.trim().split(/\s+/);
    if (w[0] === "ref") refs.set(`${w[1]} ${w[2]} ${w[3]} ${w[4]}`, w.slice(5));
  }
  const m = new Module(path), width = adapter.info.subgroupMaxSize;
  const keys = [...refs.keys()].map((k) => k.split(" ")).filter((k) => Number(k[3]) >= width && k[0] !== "sum-f32");
  for (const [name, ns, bs, ks] of keys) {
    const n = Number(ns), blocks = Number(bs), block = Number(ks), want = refs.get(`${name} ${ns} ${bs} ${ks}`);
    const s = lcg(n, n), xi = Int32Array.from(s, (v) => v - 2 ** 31), xf = Float32Array.from(s, (v) => Math.fround((v >>> 8) / 2 ** 23 - 1));
    const tag = `warp ${name} n=${n} grid=${blocks} block=${block}`;
    if (["sum-i32", "max-i32", "min-i32"].includes(name)) {
      const ident = { "sum-i32": 0, "max-i32": -(2 ** 31), "min-i32": 2 ** 31 - 1 }[name];
      const out = m.buffer(new Int32Array([ident]));
      const trapped = await m.launch("reduce_warp_" + name.replace("-", "_"), blocks, block, P(m.buffer(xi)), P(out), I(n), I(blocks));
      const got = (await m.read(out, 1, Int32Array))[0];
      check(!trapped && got === Number(want[0]), `${tag}: device ${got}, fibber's CPU ${want[0]}`);
    } else if (name === "max-f32") {
      const out = m.buffer(new Float32Array([-3.4028235e38]));
      const trapped = await m.launch("reduce_warp_max_f32", blocks, block, P(m.buffer(xf)), P(out), I(n), I(blocks));
      const got = f32bits((await m.read(out, 1, Float32Array))[0]);
      check(!trapped && got === Number(want[0]), `${tag}: device bits ${got}, CPU bits ${want[0]}`);
    } else if (name === "partials") {
      const out = m.buffer(new Float32Array(blocks));
      const trapped = await m.launch("reduce_warp_partials_f32", blocks, block, P(m.buffer(xf)), P(out), I(n), I(blocks));
      const got = Array.from(await m.read(out, blocks, Float32Array)).reduce((x, y) => x + y, 0);
      const sref = refs.get(`sum-f32 ${ns} ${bs} ${ks}`), cpu = new Float32Array(new Int32Array([Number(sref[0])]).buffer)[0], bound = Number(sref[1]);
      const tight = Math.min(bound, 0.1); // measured differences are about 5e-3: a kernel that wrote zeros is within the documented bound of nothing worth having
      check(!trapped && Math.abs(got - cpu) <= tight, `${tag}: sum of the ${blocks} partials ${got}, CPU ${cpu}, |diff| ${Math.abs(got - cpu).toExponential(2)} within ${tight} (documented bound ${bound.toExponential(2)})`);
    }
  }
  const small = m.buffer(new Int32Array([0]));
  const trapped = await m.launch("reduce_warp_sum_i32", 2, 16, P(m.buffer(new Int32Array(32).fill(1))), P(small), I(32), I(2));
  check(trapped, "warp: a block of 16 threads, less than the warp, sets the trap flag");
}

async function probeMode(path) {
  const m = new Module(path), blocks = 4, block = 64, threads = blocks * block;
  const out = m.buffer(new Int32Array(threads * 8));
  const trapped = await m.launch("warp_probe", blocks, block, P(out), I(threads));
  const g = Array.from(await m.read(out, threads * 8, Int32Array));
  const S = g[1], v = (t) => 7 * t + 3, f32 = (x) => f32bits(x);
  let wrong = 0, first = "";
  const note = (t, what, got, want) => { wrong++; if (!first) first = `thread ${t} ${what}: ${got}, model ${want}`; };
  const bal = [...Array(Math.min(S, 32)).keys()].filter((l) => l % 3 === 0).reduce((x, l) => x | (1 << l), 0);
  for (let t = 0; t < threads; t++) {
    const base = t - (t % S), lane = t % S, o = t * 8;
    if (g[o] !== lane) note(t, "lane-id", g[o], lane);
    if (g[o + 1] !== S) note(t, "subgroup-size", g[o + 1], S);
    if (lane + 1 < S && g[o + 2] !== v(t + 1)) note(t, "shfl-down 1", g[o + 2], v(t + 1));
    if (lane - 2 >= 0 && g[o + 3] !== v(t - 2)) note(t, "shfl-up 2", g[o + 3], v(t - 2));
    if ((lane ^ 5) < S && g[o + 4] !== v(base + (lane ^ 5))) note(t, "shfl-xor 5", g[o + 4], v(base + (lane ^ 5)));
    if (g[o + 5] !== v(base + 3)) note(t, "shfl-idx 3", g[o + 5], v(base + 3));
    if (g[o + 6] !== bal) note(t, "ballot", g[o + 6], bal);
    if (lane + 2 < S && g[o + 7] !== f32(Math.fround(v(t + 2)))) note(t, "shfl-down f32 2", g[o + 7], f32(Math.fround(v(t + 2))));
  }
  check(!trapped && wrong === 0, `probe: ${threads} threads, subgroup size ${S}: lane, size, shuffles down/up/xor/idx (i32 and f32) and the ballot equal the model of the warp${first ? "; first: " + first : ""}`);
}

// ---- GPU-5: f32 atomics through compare-and-exchange on the u32 bits.
async function f32AtomicsMode(path) {
  const m = new Module(path), n = 100000, s = lcg(n, 9), grid = Math.ceil(n / 256);
  const xf = Float32Array.from(s, (v) => Math.fround((v >>> 8) / 2 ** 23 - 1));
  let out = m.buffer(new Float32Array([0]));
  let trapped = await m.launch("f32_sum", grid, 256, P(m.buffer(xf)), P(out), I(n));
  const got = (await m.read(out, 1, Float32Array))[0];
  let cpu = 0, abs = 0; for (const x of xf) { cpu = Math.fround(cpu + x); abs += Math.abs(x); }
  const bound = n * 2 ** -24 * abs;
  const tight = Math.min(bound, 0.05); // the documented bound is 297 for this data (a sum of 16); the measured differences are about 3e-4
  check(!trapped && Math.abs(got - cpu) <= tight, `f32 atomic add n=${n}: device ${got}, CPU ${cpu}, |diff| ${Math.abs(got - cpu).toExponential(2)} within ${tight} (documented bound ${bound.toExponential(2)})`);
  out = m.buffer(new Float32Array([-3.4028235e38, 3.4028235e38]));
  trapped = await m.launch("f32_minmax", grid, 256, P(m.buffer(xf)), P(out), I(n));
  const mm = Array.from(await m.read(out, 2, Float32Array));
  check(!trapped && mm[0] === Math.max(...xf) && mm[1] === Math.min(...xf), `f32 atomic max/min n=${n}: device [${mm}] equal the CPU's [${Math.max(...xf)},${Math.min(...xf)}] exactly`);
}

// The kernels of branch.fib: a thread-dependent branch around a barrier in one function (the relooper, native.wgsl.func), against the CPU's answers.
async function branchMode(path) {
  const m = new Module(path);
  for (const n of [99968, 100000]) {
    const grid = Math.ceil(n / 128), len = grid * 128;
    const data = Int32Array.from(lcg(len, 9), (v, i) => (i < n ? (v >>> 12) & 0xffff : 0));
    const pd = m.buffer(data);
    const run = async (name, outLen) => {
      const out = m.buffer(new Int32Array(outLen));
      const trapped = await m.launch(name, grid, 128, P(pd), P(out), I(n));
      return trapped ? null : Array.from(await m.read(out, outLen, Int32Array));
    };
    const same = (got, want, what) => check(got !== null && got.length >= want.length && want.every((v, i) => got[i] === v), `${what} n=${n}: equals the CPU's (${got && got.slice(0, 3)}..)`);
    const base = (i) => i - (i % 128);
    if (n % 128 === 0) {
      same(await run("rev", len), Array.from({ length: n }, (_, i) => data[base(i) + 127 - (i % 128)]), "rev (a thread-dependent load, a barrier, a read of another thread's slot)");
      const par = Array.from(data, (v, i) => (i % 2 === 0 ? v + 1000 : v * 2));
      same(await run("parity", len), Array.from({ length: len }, (_, i) => par[i ^ 1]), "parity (an if/else, a barrier after the join)");
    } else {
      same(await run("tree_sum", grid), Array.from({ length: grid }, (_, b) => data.slice(b * 128, (b + 1) * 128).reduce((x, y) => x + y, 0)), `tree-sum (${grid} block sums of the branchy tree)`);
      same(await run("early", len), Array.from({ length: n }, (_, i) => data[base(i) + 127 - (i % 128)] + 1), "early (a barrier, then a branch that skips the tail)");
    }
  }
}

if (mode === "reduce") await reduceMode(a, b); else if (mode === "atomics") await atomicsMode(a); else if (mode === "branch") await branchMode(a); else if (mode === "warp") await warpMode(a, b);
else if (mode === "probe") await probeMode(a); else if (mode === "f32atomics") await f32AtomicsMode(a); else await badgridMode(a);
process.exit(bad ? 1 : 0);
