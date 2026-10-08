// compiler/tests/native/gpu/wgsl_run.mjs: runs the WGSL of fib.gpu's atomics and reductions on a WebGPU device (Dawn through the `webgpu` npm package of
// scripts/fetch-webgpu-tools.sh) and compares with the CPU, as cuda_run.py does for PTX. The launch follows the binding model of native.wgsl: group 0, binding 0 the
// uniform of scalar parameters (4 bytes each), binding 1 the trap flag, bindings 2.. the pointer parameters; the workgroup size is the override constant wg_x.
//   node wgsl_run.mjs reduce REDUCE.wgsl REF.txt | atomics ATOMICS.wgsl | badgrid REDUCE.wgsl
// One line per check, `ok ...` or `FAIL ...`. Exit 0 when every check holds, 1 on a failure, 3 when there is no adapter or no package (the caller skips).
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";

const dir = process.env.FIB_WEBGPU_NODE ?? `${homedir()}/.cache/fibber-scratch/tools/webgpu-node`;
let gpu;
try { const { create, globals } = createRequire(dir + "/")("webgpu"); Object.assign(globalThis, globals); gpu = create([]); } catch { process.exit(3); }
const adapter = await gpu.requestAdapter();
if (!adapter) process.exit(3);
const device = await adapter.requestDevice();
let bad = 0;
const check = (ok, text) => { console.log((ok ? "ok   " : "FAIL ") + text); if (!ok) bad++; };

class Module {
  constructor(path) {
    const code = readFileSync(path, "utf8");
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

const [mode, a, b] = process.argv.slice(2);
if (mode === "reduce") await reduceMode(a, b); else if (mode === "atomics") await atomicsMode(a); else await badgridMode(a);
process.exit(bad ? 1 : 0);
