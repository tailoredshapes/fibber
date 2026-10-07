// examples/wasm/kernels.mjs: calls the exports of kernels.fib from node with typed arrays (docs/design/wasm.md 5). No build step beyond `fibc build`.
//   node examples/wasm/kernels.mjs kernels.wasm
import { WASI } from "node:wasi";
import { readFileSync } from "node:fs";

const wasi = new WASI({ version: "preview1", args: ["kernels"], env: {} });
const { instance } = await WebAssembly.instantiate(readFileSync(process.argv[2]), wasi.getImportObject());
wasi.initialize(instance); // wasi-libc's reactor start; the fibber runtime itself starts on the first export call
const x = instance.exports;

console.log("add(40, 2) =", x.add(40n, 2n)); // an i64 is a BigInt on the JavaScript side

// A typed array in, a number out: the module makes the memory (its own malloc), JavaScript fills it through a view of the linear memory.
const a = Float64Array.from([1, 2, 3, 4]);
const b = Float64Array.from([10, 20, 30, 40]);
const pa = x.malloc(a.byteLength), pb = x.malloc(b.byteLength);
new Float64Array(x.memory.buffer, pa, a.length).set(a); // the view is made after malloc: a call can grow (and so move) the memory
new Float64Array(x.memory.buffer, pb, b.length).set(b);
console.log("dot =", x.dot(pa, pb, BigInt(a.length)));
x.free(pa); x.free(pb);

console.log("sumSquares(1000) =", x.sumSquares(1000n)); // allocates vectors inside the module
console.log("vecLength(5) =", x.vecLength(5n));
