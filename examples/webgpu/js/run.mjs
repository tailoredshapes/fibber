// examples/webgpu/js/run.mjs: runs the wasm host of the WebGPU kernels under node (docs/design/webgpu.md 6).
//   node run.mjs HOST.wasm KERNELS.wgsl [--dawn DIR]
// HOST.wasm is examples/webgpu/host.fib built for wasm32-wasi with `--export run`; KERNELS.wgsl is what `fibc build --target
// wgsl-unknown-webgpu --emit wgsl` wrote. `navigator.gpu` comes from the `webgpu` npm package (Dawn) when node has none: --dawn names
// the directory with its node_modules (default $FIB_WEBGPU_NODE, else ~/.cache/fibber-scratch/tools/webgpu-node, scripts/fetch-webgpu-tools.sh).
// Exit status: the module's `run` (0 every hash is the reference's, 1 a mismatch, 2 a driver error), 3 when node has no JSPI.
import { WASI } from "node:wasi";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { FibGpu, wasmImports } from "./fib-webgpu.mjs";

const args = process.argv.slice(2);
let dawn = process.env.FIB_WEBGPU_NODE ?? `${homedir()}/.cache/fibber-scratch/tools/webgpu-node`;
const rest = [];
while (args.length) { const a = args.shift(); if (a === "--dawn") dawn = args.shift(); else rest.push(a); }
const [wasmFile, wgslFile] = rest;
if (!wasmFile || !wgslFile) { console.error("usage: node run.mjs HOST.wasm KERNELS.wgsl [--dawn DIR]"); process.exit(2); }
if (typeof WebAssembly.Suspending !== "function") { console.error("run.mjs: this node has no JSPI (WebAssembly.Suspending): node 24+ is needed"); process.exit(3); }

export async function gpuOf(dawnDir) {
  if (globalThis.navigator?.gpu) return globalThis.navigator.gpu;
  try {
    const require = createRequire(dawnDir + "/");
    const { create, globals } = require("webgpu");
    Object.assign(globalThis, globals);
    return create([]);
  } catch (e) {
    return undefined;   // no WebGPU in this host: `open` is the Err
  }
}

const wgsl = readFileSync(wgslFile, "utf8");
const fib = new FibGpu(await gpuOf(dawn));
const wasi = new WASI({ version: "preview1", args: [wasmFile], env: process.env, returnOnExit: true });
const module = await WebAssembly.compile(readFileSync(wasmFile));
let memory;
const imports = { ...wasi.getImportObject(), env: wasmImports(fib, () => memory) };
const instance = await WebAssembly.instantiate(module, imports);
memory = instance.exports.memory;
wasi.initialize(instance);
const text = new TextEncoder().encode(wgsl);
const p = instance.exports.malloc(text.length + 1);
new Uint8Array(memory.buffer, p, text.length).set(text);
const run = WebAssembly.promising(instance.exports.run);
const status = Number(await run(p, BigInt(text.length)));
process.exit(status);
