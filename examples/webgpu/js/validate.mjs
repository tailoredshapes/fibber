// examples/webgpu/js/validate.mjs FILE.wgsl: compiles a WGSL file with Dawn (Tint, the WebGPU implementation of Chromium) through the `webgpu` npm package and
// prints every message in full; exit 0 when there is no error, 1 on an error, 3 when the package or an adapter is missing (the test skips with a note).
// Tint is the strict validator: its uniformity analysis refuses a `workgroupBarrier` that naga accepts. The package comes from scripts/fetch-webgpu-tools.sh (FIB_WEBGPU_NODE).
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
const dir = process.env.FIB_WEBGPU_NODE ?? `${homedir()}/.cache/fibber-scratch/tools/webgpu-node`;
let gpu;
try { const { create, globals } = createRequire(dir + "/")("webgpu"); Object.assign(globalThis, globals); gpu = create([]); } catch { process.exit(3); }
const adapter = await gpu.requestAdapter();
if (!adapter) process.exit(3);
const code = readFileSync(process.argv[2], "utf8");
// A module that says `// fib.requires: subgroups` (native.wgsl.subgroup) needs the device feature; an adapter without it rejects the module by name, exit 4.
const needSub = /^\/\/ fib\.requires: subgroups/m.test(code);
if (needSub && (!adapter.features.has("subgroups") || process.env.FIB_WEBGPU_NO_SUBGROUPS)) {
  console.log("rejected: the module requires the WebGPU `subgroups` feature (it uses warp shuffles) and this adapter does not have it");
  process.exit(4);
}
const device = await adapter.requestDevice({ requiredFeatures: needSub ? ["subgroups"] : [] });
device.pushErrorScope("validation");
const mod = device.createShaderModule({ code });
const info = await mod.getCompilationInfo();
const err = await device.popErrorScope();
let bad = err !== null;
for (const m of info.messages) { console.log(m.type, `${m.lineNum}:${m.linePos}`, m.message); if (m.type === "error") bad = true; }
if (err) console.log(err.message);
process.exit(bad ? 1 : 0);
