// compiler/tests/wasm/run.mjs: runs a WASI module under node's built-in `node:wasi` (docs/design/wasm.md 4).
//   node run.mjs [--dir HOST[=GUEST]]... [--env NAME=VALUE]... [--stdin FILE] MODULE.wasm [-- ARG..]
// Standard output and error are the process's own; the exit status is the program's (`proc_exit`), or 134 for a trap of the module (an `unreachable`, the runtime's `abort`
// compiles to one), the way a native program's `abort` ends with SIGABRT. A host function the module imports and node does not provide fails when it is called.
import { WASI } from "node:wasi";
import { readFileSync, openSync } from "node:fs";

const args = process.argv.slice(2);
const preopens = {};
const env = { ...process.env };
let stdin = 0;
while (args.length && args[0].startsWith("--") && args[0] !== "--") {
  const flag = args.shift();
  const value = args.shift();
  if (flag === "--dir") { const [h, g] = value.split("="); preopens[g ?? h] = h; }
  else if (flag === "--env") { const i = value.indexOf("="); env[value.slice(0, i)] = value.slice(i + 1); }
  else if (flag === "--stdin") stdin = openSync(value, "r");
  else { console.error("run.mjs: unknown flag " + flag); process.exit(2); }
}
const file = args.shift();
if (args[0] === "--") args.shift();
const wasi = new WASI({ version: "preview1", args: [file, ...args], env, preopens, stdin, returnOnExit: true });
const module = await WebAssembly.compile(readFileSync(file));
const imports = wasi.getImportObject();
// Imports of the module that are not WASI's (`--allow-undefined` builds) trap when called instead of failing the instantiation.
for (const m of WebAssembly.Module.imports(module)) {
  if (m.module === "wasi_snapshot_preview1") continue;
  imports[m.module] ??= {};
  imports[m.module][m.name] ??= () => { throw new WebAssembly.RuntimeError(`import ${m.module}.${m.name} is not provided`); };
}
const instance = await WebAssembly.instantiate(module, imports);
let status;
try {
  status = wasi.start(instance);
} catch (e) {
  if (e instanceof WebAssembly.RuntimeError) { console.error("wasm trap: " + e.message); if (process.env.WASM_TRAP_STACK) console.error(e.stack); status = 134; }
  else throw e;
}
process.exit(status);
