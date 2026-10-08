# WebGPU: the second kernel backend, for the browser, node and the native host

Status: built and measured (WEBGPU-1, 2026-10-07). In this tree: the kernel-target row `wgsl-unknown-webgpu` (compiler/types/targets.fib),
the WGSL printer `native.wgsl` (compiler/native/wgsl.fib and wgsl/), `--emit wgsl`, the JS host glue and the wasm host (examples/webgpu/js/),
the example kernels and their CPU references (examples/webgpu/), the tests (compiler/tests/native/wgsl-emit.sh, cases/stdlib 8570-8577,
scripts/webgpu-agree.sh, scripts/mutant-webgpu.sh) and the tool fetcher (scripts/fetch-webgpu-tools.sh). In its own repository: the native
driver `fib-gpu-webgpu` (`ssh://git@localhost:2222/tailoredshapes/fib-gpu-webgpu.git`, tag `v0.0.1`). Every number below was measured on
this machine (RTX 4080 SUPER, driver 610.57.04, wgpu-native v29.0.1.1, Dawn via the `webgpu` npm package 0.6.2, naga-cli 30.0.1, node
v26.10.0, Chromium 153 as a snap). The GPU design this extends is docs/design/gpu.md; the kernel subset, `defkernel`, the index space and
the `Device` protocol are its (sections 3 and 4) and are not restated.

## 1. What it is

A kernel target (gpu.md 4.1) whose output is WGSL, the shading language of WebGPU, instead of PTX: `fibc build --target wgsl-unknown-webgpu
--emit wgsl FILE -o K.wgsl` lowers the program as usual, keeps its kernels and what they reach (the same kernel module `native.kernel`
extracts for PTX) and prints that module as WGSL compute shaders. LLVM has no WGSL backend, so this row is the first whose code generator
is a printer of lIR (as `lir2js` is for JavaScript, compiler/js/) and not an LLVM machine: `--emit llvm/asm/obj/ptx` are refused on it.
The WGSL is run by a driver of the `Device` protocol: `fib-gpu-webgpu` over wgpu-native on the native host (Vulkan here; Metal, D3D12, GL
elsewhere), or the JavaScript glue `fib-webgpu.mjs` over `navigator.gpu` for a fibber program built for wasm32 (Dawn under node, a
browser). The point: the same `defkernel`, three machines (the CPU, CUDA, WebGPU), one result.

## 2. What was measured

`scripts/webgpu-agree.sh --browser`, the one-language guard: examples/webgpu/kernels.fib (vector add in f32 and i32, the register-tiled
f32 GEMM of gpu.md with i32 indices, a device assert) run on every backend, each run printing the FNV-1a hash of its result:

```
cpu: gemm 256 2053739098;vadd 1048576 4050346011;vaddi 1048576 925329787;
ok   cuda (PTX, fib-gpu-cuda): vadd, vaddi, gemm hash as the CPU's (device: NVIDIA GeForce RTX 4080 SUPER sm_89)
ok   webgpu native (WGSL, fib-gpu-webgpu over wgpu-native): vadd, vaddi, gemm hash as the CPU's (device: NVIDIA GeForce RTX 4080 SUPER (610.57.04, Vulkan))
ok   webgpu node (WGSL, the wasm host, Dawn): vadd, vaddi, gemm hash as the CPU's (device: nvidia lovelace nvidia-geforce-rtx-4080-super ..)
note webgpu chromium (WGSL, the wasm host in the browser): vadd and vaddi hash as the CPU's; the GEMM differs (an unfused fma on this host: device: google swiftshader): gemm 256 906634477
webgpu-agree: every run made agrees with the CPU
```

The GEMM is bit for bit the CPU's `fma` chain through CUDA, through wgpu-native and through Dawn on the NVIDIA GPU; on SwiftShader (the
CPU Vulkan Chromium fell back to, section 6.3) it differs because WGSL's `fma` is "inherited from `x * y + z`" in the specification's
accuracy table: a host may or may not fuse it. The agreement test therefore demands bit equality of vadd and vaddi and reports the GEMM's
state per host; a kernel that needs a fused multiply-add on every WebGPU host must compute it another way (section 3.5).

The device assert: `assert-positive` on an input with a zero makes `sync` return `Err -20 a kernel trapped` on wgpu-native, under Dawn and in
Chromium; on a positive input `Ok` (examples/webgpu/host.fib prints both).

`fib-gpu-webgpu/examples/bench.fib` on wgpu-native over Vulkan, the wall clock around launch and sync (the driver has no timestamp queries
yet; the submit is in the number), median of 5 (3 at 4096), every result bit exact against the CPU:

| | WebGPU (WGSL, wgpu-native/Vulkan) | CUDA (PTX, fib-gpu-cuda, GPU events; gpu.md 2) | WebGPU / CUDA |
|---|---:|---:|---:|
| vector add, 2^24 f32 | 0.52 ms, 384 GB/s | 0.32 ms, 626 GB/s | 61% |
| gemm 1024 | 0.366 ms, 5.9 TFLOPS | 0.264 ms, 8.1 | 72% |
| gemm 2048 | 1.96 ms, 8.8 TFLOPS | 1.61 ms, 10.6 | 83% |
| gemm 4096 | 14.3 ms, 9.6 TFLOPS | 12.6 ms, 10.9 | 88% |
| upload (queue.writeBuffer) | 5.3 GB/s | 13.6 to 15.7 GB/s | |
| download (a mapped staging copy) | 0.28 GB/s | 0.4 to 2.4 GB/s | |

Why the gap, honestly: (a) the clock: CUDA's numbers are GPU events around the launch, WebGPU's are the wall clock around submit, poll and
the flag read, which is most of the 0.2 ms difference at 1024 and all of vadd's; (b) the code: WGSL goes through naga to SPIR-V and the
NVIDIA Vulkan compiler, PTX through LLVM's NVPTX backend and the NVIDIA assembler; the GEMM's inner loop is the same 16 fused multiply-adds
and 8 loads per k, and the per-load `switch` on the buffer number (section 3.3) folds away after inlining (the 9.6 TFLOPS say so: a real
switch per load would halve it); (c) the transfers: `queue.writeBuffer` copies through a staging ring, and the download maps a fresh staging
buffer each time (a persistent one is the fix, as pinned memory is CUDA's). The shared-memory version of the GEMM (gpu.md 6.3) would move
both backends; WGSL has `var<workgroup>` ready for it (section 3.6).

## 3. The mapping: the kernel subset to WGSL

The printer (compiler/native/wgsl/) takes the kernel module: SSA lIR with blocks, phis, `let`s. Each lIR function is one WGSL function in
the loop-and-switch form of the JS backend (compiler/js/func.fib): `var L: u32 = 0u; loop { switch L { case 0u: { .. } .. } }`, one case per
block, a branch `L = k;` (the case ends, the loop goes round), a phi a `var ph<pos>` assigned on the edge, a block with one incoming edge
written where that edge is. Every SSA name is a `var` of the function (a value defined in one block is read in another; WGSL `let`s are
block-scoped). Parameters are copied into `var`s of their names (a self tail call assigns them and jumps to the entry; any other tail call
is `return f(..)`). The entry of a kernel is a `@compute @workgroup_size(wg_x, wg_y, wg_z) fn NAME(@builtin ..)` that stores the builtins
into private variables and calls the body `fibw_k_NAME(..)` with the buffers and the scalars of the binding model (3.3).

### 3.1 Types and values

| lIR | WGSL | note |
|---|---|---|
| `i1` | `bool` | `and`/`or` are `&`/`|` (no short circuit in lIR either); `xor`/`add`/`sub` are `!=` |
| `i32` | `i32` | `+ - *` wrap in WGSL as in lIR's unchecked forms; `udiv urem lshr`, the unsigned comparisons and `umin umax` go through `bitcast<u32>` |
| `float` | `f32` | a literal is its shortest decimal (`f32(0.5)`), exact; NaN and the infinities are their bits (`bitcast<f32>(0x7fc00000u)`) |
| `ptr` | `struct Ptr { buf: u32, off: u32 }` | a storage buffer by number and a byte offset (3.3) |
| `{i32, i1}` (the pair of `sadd-overflow` ..) | `struct OvI32 { v: i32, o: bool }` | `extractvalue 0/1` is `.v`/`.o` |
| a string constant (a trap's message) | `Ptr(0u, 0u)` | the message is lost on the device, as on CUDA (gpu.md 3.4) |
| `i64` | **refused**: "i64 (WGSL has no 64-bit integer: index with i32)" | except as an address (3.3): an i64 `let` whose every use is a `getelementptr` offset is folded and never printed |
| `double` | **refused**: "f64 (WGSL has no double: compute in f32)" | |
| `i8`, `i16` | **refused**: "WGSL has no narrow integer" | a byte load (`load-i8`) is refused as "a load of i8" |
| `<N x T>` | **refused**: "a SIMD value (a kernel is scalar code: the GPU supplies the lanes)" | gpu.md 6: the lane model is the CPU's |
| any other aggregate, an `alloca` of several elements, a global that is not a constant, a function pointer, an indirect call | **refused** by name | shared memory is the one global a kernel will have (3.6) |

A refusal names where it is (`the webgpu target cannot take function f_gpu32_f32_at of kernel vadd: ..` or `.. kernel vadd: ..`), what
and why, and this document; it is the first found, the printer goes on and the text is dropped. The existing example kernels of
examples/gpu/kernels.fib index with i64 (`n: i64`, `gpu/global-id`) and are refused by name on this target; examples/webgpu/kernels.fib and
gpu32.fib are the same kernels over i32, which both targets take (section 6: that is the one-language guard, and the reason `gpu32` exists).

### 3.2 Operations

| lIR | WGSL |
|---|---|
| `add sub mul sdiv srem and or xor shl ashr` on i32; `fadd fsub fmul fdiv frem` | the operators (`%` on f32 keeps the dividend's sign, as `frem`) |
| `udiv urem lshr`; `icmp ult ule ugt uge` | the same through `bitcast<u32>` |
| `icmp` on i32, bool, `Ptr` (field by field) | `== != < <= > >=` |
| `fcmp` ordered | the operator; `one` is `(a < b) | (a > b)`; `ord`/`uno` are `(a == a) & (b == b)` and its negation; the unordered forms are `!(ordered opposite)` |
| `sadd-overflow ssub-overflow smul-overflow` | `fibw_sadd_ovf(a, b)` .. of the prelude: the wrapped result and the overflow bit (`(a ^ r) & (b ^ r) < 0`; for `mul`, `r / a != b` with the `-1 * INT_MIN` case) |
| `sext zext trunc` between i32 and bool | `select(i32(0), i32(-1), c)`, `select(i32(0), i32(1), c)`, `(x & 1) != 0` |
| `fptosi fptosi-sat`, `fptoui fptoui-sat`, `sitofp`, `uitofp`, `bitcast` | `i32(x)` (WGSL's conversion saturates), `bitcast<i32>(u32(x))`, `f32(x)`, `f32(bitcast<u32>(x))`, `bitcast<T>(x)` |
| `ptrtoint inttoptr`, a `getelementptr` of several indices or of an element type other than i8/i32/float | **refused**: "pointer arithmetic beyond an index and an offset" |
| `select` | `select(b, a, c)` |
| `fma`, `fmuladd` | `fma(a, b, c)` (2: not promised fused by WGSL) |
| `fsqrt fabs ffloor fceil ftrunc froundeven`, `abs`, `smin smax`, `fminnum fmaxnum`, `cttz ctlz`, `ctpop` | `sqrt abs floor ceil trunc round`, `abs`, `min max`, `min max`, `countTrailingZeros countLeadingZeros`, `countOneBits` |
| `umin umax` | `min max` through `bitcast<u32>` |
| `fcopysign` | the sign bit moved: `bitcast<f32>((bits(a) & 0x7fffffff) | (bits(b) & 0x80000000))` |
| `fround` (ties away from zero) | **refused**: "WGSL rounds ties to even only (use froundeven)" |
| `fmin fmax` (NaN-propagating) | **refused**: "WGSL's min/max do not promise to propagate NaN (use fminnum/fmaxnum)" |
| the reductions, `masked-load`, `gather`, every vector form | **refused** with the SIMD value they take |
| `exp log sin cos pow` .. | not reachable: they are libc calls in fibber, which the extractor already refuses (gpu.md 4.2); WGSL has them, a `gpu/exp` family of builtins (ADR 0014) would map one to one |
| `(sreg tid.x/y/z)`, `(sreg ctaid.x/y/z)`, `(sreg ntid.x/y/z)`, `(sreg nctaid.x/y/z)` | `i32(fibw_lid.x)`, `i32(fibw_wid.x)`, `i32(wg_x)` (the override), `i32(fibw_nwg.x)`: `local_invocation_id`, `workgroup_id`, the workgroup size, `num_workgroups`, stored by the entry into `var<private>`s so a device function can read them (WGSL gives builtins to entry points only) |
| `(barrier)` | `workgroupBarrier()` (3.6 on uniformity) |
| `(trap)` | `fibw_trap()`: sets `var<private> fibw_trapped` and `atomicStore(&fibw_flag.flag, 1u)` (3.4); the function returns, and so does every caller after a call of a function that may trap (`if (fibw_trapped) { return ..; }` after the call: a fixed point over the call graph), so nothing is stored after a trap |
| `unreachable` | `return` of the zero of the result type |
| a call of a function of the module | the call; of anything else (`llvm.*`, libc) **refused** |

### 3.3 Memory: the binding model

WGSL has no raw pointers across buffers: a pointer is one of the kernel's storage buffers and an offset into it. The printer's `Ptr` is
`{buf: u32, off: u32}` (a byte offset: the offsets fibber's accessors compute are bytes), `getelementptr i8 p i` is `Ptr(p.buf, p.off + i)`,
a load of f32 is `fibw_ld_f32(p)`: `bitcast<f32>(fibw_ld_u32(p))` where `fibw_ld_u32` is a `switch p.buf { case 0u: { return buf0[p.off >> 2u]; } .. }`
over the module's buffers (every buffer is `array<u32>`; i32 and f32 are bitcasts of the word; a store is the same the other way). After
inlining, `p.buf` is a constant for every access that starts from a parameter, and the switch is gone: the GEMM's 9.6 TFLOPS is the proof;
a pointer that a device function receives as a parameter keeps the switch until its caller is inlined, which GPU compilers do. Accesses are
4-byte aligned (`>> 2u`); the `(align 1)` of fibber's accessors is irrelevant here (there is no byte access to be misaligned: a byte load is
refused). WebGPU's robust access makes an out-of-range index read 0 and write nothing: the kernel's own bound check (`(when (< i n) ..)`)
is still the rule, as on CUDA, where the same access is undefined. An i64 offset expression (the `sext` of an i32 index times 4, as
gpu32.fib writes it) is printed as u32 arithmetic: the one place an i64 appears, and exact for buffers under 4 GB (WebGPU's own limit is
lower). An alloca'd scalar (a loop variable of `loop`/`recur`: the emitter keeps them in slots) is a `var` of the function; its address
never escapes (refused if it would).

The bindings, group 0, the same for every kernel of a module so that a driver has one layout per module:

| binding | what | the host side |
|---|---|---|
| 0 | `var<uniform> fibw_params: array<vec4<u32>, 4>`: the scalar parameters, 4 bytes each in parameter order (the bits of an i32 or f32; a bool as 0/1), at most 16 | a 64-byte uniform buffer written per launch |
| 1 | `var<storage, read_write> fibw_flag: FibTrap { flag: atomic<u32> }`: the trap flag | 16 bytes, zeroed before each dispatch, read after `sync` |
| 2 .. 2+N-1 | `var<storage, read_write> bufK: array<u32>`: the pointer parameters in order; N is the most any kernel of the module has, at most 7 (WebGPU's default is 8 storage buffers per stage, and the flag is one) | the launch's buffers in argument order; the slots a kernel does not take are bound to dummies (an explicit layout: `auto` would drop the flag from a kernel that never traps, and a buffer bound twice is "writable aliasing", refused) |

The workgroup size is the override constants `wg_x wg_y wg_z` (defaults 64, 1, 1), set at pipeline creation from the launch's block: one
pipeline per kernel and block size, cached by the driver. Both Dawn and naga take overrides in `@workgroup_size` (checked: the JS glue and
wgpu-native set them). The first lines of the WGSL are the launch ABI (gpu.md 6.4 asked for one): `// fib.kernel-sig NAME: T..` (GPU-2's header), one per kernel,
the parameter kinds in order (`ptr`, `i32`, `f32`, `bool`); every driver checks a launch's arguments against it (the count, a buffer for a
scalar or the reverse, one buffer twice) and refuses with the parameter's number. This is the first backend where "a wrong argument is an
Err naming the parameter" holds; the PTX path still lacks it.

### 3.4 The device assert

WGSL has no trap. A fibber `trap` (and the checked `+`/`*` that overflow, the index check) reaches the kernel module as a call of the
runtime's trap entry, which the extractor rewrites to `(trap)`. The printer makes it `fibw_trap()`: the private `fibw_trapped` is set, the
flag buffer's word is `atomicStore`d to 1, the function returns its type's zero, and every caller returns after a call that may trap. The
driver zeroes the flag before every dispatch and reads it after the queue completes (`sync`): a set flag is `Err -20`, the same contract as
CUDA's `trap;` (gpu.md 3.4), with one difference that is better: the device and its buffers are intact afterwards (CUDA loses the context).
Checked arithmetic: WGSL's i32 `+` wraps; the printer's `fibw_sadd_ovf` computes the wrapped sum and the overflow bit from the signs, as LLVM's
intrinsic would, so an overflow is a trap here as on the CPU and on CUDA (`vaddi` carries one; the planted fault "wrap-on-overflow not
checked" is caught by the WGSL checks and the golden).

### 3.5 fma

WGSL's `fma(a, b, c)` is specified as `a * b + c` with an accuracy "inherited" from the two operations: a host may fuse it (every GPU does;
Dawn on NVIDIA and wgpu on NVIDIA gave bit-exact results against the CPU's `simd/fma` chain) or not (SwiftShader did not, section 2).
fibber's `simd/fma` promises one rounding (ADR 0008; a CPU without FMA traps). The honest statement for this target: the printer emits
`fma(..)`; whether it is fused is the host's; a kernel that must be bit-exact across WebGPU hosts uses `simd/muladd` in its meaning (either
rounding) and compares with a tolerance, as gpu.md 2.1 derives one for sums in another order. The agreement test encodes this.

### 3.6 Not in this target yet, with the mapping ready

* **Shared memory** (gpu.md 6.3, `(gpu/shared T N)`): a `var<workgroup> name: array<T, N>` at module scope, one per kernel and declaration,
  and a `Ptr` whose `buf` names it (a third kind of buffer number in the load/store switch, with no bounds concern: a fixed array). When the
  lIR form lands (`(global shared ..)` in address space 3 for NVPTX), the printer maps the global's name to the `var<workgroup>` and
  `addrspacecast` to a `Ptr` of that kind: about 40 lines.
* **The barrier under divergence**: WGSL's uniformity analysis (Tint enforces it; naga is laxer) refuses `workgroupBarrier()` under control
  flow it cannot prove uniform. The loop-and-switch form hides the structure from the analysis: a kernel with a barrier inside a loop may be
  refused by Tint although it is uniform. The fix is structured printing of single-entry regions (the relooper's job); gpu.md 6.2's checker
  rule ("a barrier is not under an index-dependent condition") is the source-level half.
* **Atomics**: `atomicAdd`, `atomicMax` .. on `atomic<i32>`/`atomic<u32>` in storage and workgroup memory; lIR's `atomicrmw` maps one to one
  once a buffer can be declared `array<atomic<u32>>` (the printer would need the element type per buffer: a kernel attribute, or the
  signature's `atomic ptr`).
* **f16**: behind WebGPU's `shader-f16` feature (`enable f16;` in the WGSL, `requiredFeatures` at device creation); lIR has no half type
  (gpu.md 6.5); when it has, `half` is `f16` here, with the feature requested by the driver when the WGSL enables it.
* **64-bit**: never; an i64 kernel is refused by name, and the design accepts it: GPU index arithmetic is 32-bit anyway (CUDA's `int` too).

## 4. The compiler changes

| where | delta |
|---|---|
| compiler/types/targets.fib | the row `row-wgsl-webgpu` (triple `wgsl-unknown-webgpu`, arch `wgsl`, os `webgpu`, object format `wgsl`, a kernel target); `wgsl-row?`; `os-of-triple` knows `-webgpu` |
| compiler/native/wgsl.fib (160 lines), wgsl/wx.fib (170), wgsl/expr.fib (250), wgsl/func.fib (200) | the printer: the module (the prelude, the signatures, the entries), the context and types, the expressions, the functions |
| compiler/native/api.fib | `--emit wgsl`: the kernel module through `native.wgsl` instead of `aot-emit`; every LLVM `--emit` refused on the row |
| compiler/driver/args.fib | the `wgsl` word |
| compiler/native/linkwasm.fib | the library `js` of an extern (`:lib "js"`) is `--allow-undefined` on wasm: the host provides it as `env.NAME` |
| compiler/tests/native/wgsl-emit.sh, wgsl/kernels.wgsl | the tests and the golden |
| cases/stdlib/8570-8577 | the subset accepted and refused (host cases with a `;; webgpu-target = VERDICT | TEXT` line that wgsl-emit.sh checks on the target; floor 1428) |
| scripts/webgpu-agree.sh, mutant-webgpu.sh, fetch-webgpu-tools.sh | the agreement test, six mutants, the tools (ADR 0020) |

Nothing above the lIR changed; the kernel module is byte for byte the one PTX takes (the printer and lair are two consumers of
`native.kernel/kernel-module`). ADR 0003: no function of compiler/ or lib/ is named `wgsl`, `webgpu` or `fibw`. ADR 0014: no builtin was added.

## 5. The hosts and their drivers

### 5.1 The wasm/JS host: `examples/webgpu/js/`

A fibber program built for wasm32-wasi (`--export run`: a reactor) requires the module `webgpu` of examples/webgpu/js/webgpu.fib: the
`Device` protocol as fibber over twelve externs `fib_gpu_*` declared `:lib "js"`, which the module imports from `env`. The glue
`fib-webgpu.mjs` provides them (`wasmImports(fibGpu, memory)`) over a `FibGpu` class that is the driver in JavaScript (open, loadWgsl with an
explicit bind group layout, kernel, buffer, upload with `queue.writeBuffer`, download through a staging copy and `mapAsync`, launch with the
uniform written, the flag zeroed, the bind group, the dispatch, sync with `onSubmittedWorkDone` and the flag read). WebGPU is asynchronous
and a wasm import is a call: the asynchronous imports (`open`, `load`, `download`, `sync`) are `WebAssembly.Suspending` and the export is
called through `WebAssembly.promising`: JavaScript Promise Integration, in node 24+ and Chrome 137+ (checked: node v26.10.0, Chromium 153).
Without it the program would need a worker and `Atomics.wait`. Errors never cross into the module as exceptions: every import answers a
status and keeps its text for `fib_gpu_error`; a host without `navigator.gpu`, without an adapter or without a device makes `open` an `Err`
(`-2`, `-3`, `-4`) and the fibber program decides. `run.mjs` runs the host under node, taking `navigator.gpu` from the `webgpu` npm package
(Dawn, with Tint's validation at pipeline creation: the second validator); `browser.html` + `browser.sh`/`browser.mjs` run it in Chromium
headless with a 40-line WASI shim (the eight calls the runtime makes) and the DevTools protocol relaying the console.

The ABI of the imports (all i32, so no BigInt crosses): `fib_gpu_open() fib_gpu_close() fib_gpu_name(buf cap) fib_gpu_error(buf cap)
fib_gpu_load(wgsl len) -> module fib_gpu_kernel(module name len) -> kernel fib_gpu_buffer(bytes) -> buffer fib_gpu_upload(buffer ptr bytes)
fib_gpu_download(buffer ptr bytes) fib_gpu_launch(kernel gx gy gz bx by bz args n) fib_gpu_sync() fib_gpu_release(buffer)`; a launch's
arguments are a table of 8 bytes each, a kind (0 buffer, 1 i32, 2 f32) and the value's 32 bits.

### 5.2 The native host: `fib-gpu-webgpu` over wgpu-native

The same `ns webgpu` with the same functions, over wgpu-native v29.0.1.1 (`libwgpu_native.so`, `webgpu.h`): a host program `(:require
[webgpu :as g])` builds against either driver by its `-I` path (examples/webgpu/host.fib is built both ways). The driver is 300 lines of
fibber (the protocol, the ownership rules, the signature check, the trap flag, Results) over a C shim of 200 lines (`shim/fibwgpu.c`,
`libfibwgpu.so`, `:lib "fibwgpu"`), and the shim is the honest delta of this package:

* **Callbacks.** webgpu.h's asynchronous calls (`wgpuAdapterRequestDevice`, `wgpuBufferMapAsync`, `wgpuDevicePopErrorScope`) deliver to a C
  function pointer; wgpu-native panics on a null one. fibber has no way to export a function with the C convention (`--export` makes a C
  symbol of a library build, but no `ptr` to it from inside a program; a closure's address is the fibber convention's). The smallest
  addition the language needs: a form that makes a C-convention entry of a fibber function and yields its address, `(defun ^c-callback name
  ..)` or `(extern-callback (fn ..))`, lowered as the `fib.main-tramp`-style trampolines the runtime already writes in lIR (a C-convention
  `define` that calls the fibber function). Reported, not implemented here. With it, three of the shim's functions go.
* **Descriptors.** webgpu.h is an API of structs with `INIT` macros (a buffer, a pipeline, a bind group, a device are each a dozen fields,
  with `WGPUStringView` pairs and `nextInChain` extensions) whose layouts move between releases: writing them from fibber by byte offsets is
  possible and brittle; the shim fills them from the header. This is `fibgen`'s job (a header to externs and layouts) once it is ported.
* **Synchronous waits.** wgpu-native v29 has no `wgpuInstanceWaitAny` (it panics "not implemented"): the shim uses the fact that wgpu-core is
  synchronous (a device request and an error-scope pop deliver inside the call) and `wgpuDevicePoll(wait)` for a map and for `sync`.
  `WGPU_BACKEND=vulkan|gl|metal|dx12` limits the backends (`WGPUInstanceExtras`): a backend the host lacks is "no adapter", `Err -2`.

The contract (`scripts/test.sh --plant`, on this machine): vadd, vaddi and the GEMM bit exact, the assert's two paths; 16 checks of
`specs/ownership-spec.fib` (lent-buffer misuse, release twice, use after release, the three signature mismatches naming the parameter, a
block over WebGPU's 256 invocations, an empty grid, a kernel the module lacks, an oversize download, a zero-byte buffer, a WGSL that does
not compile: each an `Err`); `WGPU_BACKEND=metal` on Linux is `Err -2` from `open`, exit 2, no trap; three planted faults caught (the lent
check removed: the spec fails; the flag never read: the assert check fails; the block size ignored: a hash differs). The loader caveat of
gpu.md 7 applies: a missing `libwgpu_native.so` fails before `main` (the dynamic loader's message), not as an `Err`.

## 6. The one-language guard: one `defkernel`, five runs

examples/webgpu/kernels.fib holds the kernels and, beside them, their CPU references in the same fma order and `main`, which prints the three
reference hashes. examples/webgpu/host.fib is the host program over the `webgpu` driver (either one): it launches the kernels, prints the
same lines and checks them (the wasm build prints the GEMM's hash only: `simd/fma` traps on wasm32, ADR 0008, so the CPU reference is the
native run's). examples/webgpu/host-cuda.fib is the same host over the `cuda` driver. `scripts/webgpu-agree.sh` builds all of it and
compares the lines (section 2). The guard against the two-languages trap (gpu.md 7) holds: the kernels are fibber, built for the host, for
nvptx64 and for wgsl from one file; what differs per backend is the host program's driver module.

### 6.1 node with Dawn

`node examples/webgpu/js/run.mjs host.wasm kernels.wgsl`: the adapter is `nvidia lovelace nvidia-geforce-rtx-4080-super`; every hash the
CPU's; the assert traps. Tint accepted the WGSL with two warnings before the double `return` was removed, none after.

### 6.2 Chromium headless

`examples/webgpu/js/browser.sh host.wasm kernels.wgsl` runs it in the snap's Chromium 153 under `xvfb-run` (Chromium's GPU process needs an X
display even headless: ANGLE's Vulkan display is XCB, and without one the GPU process exits and `requestAdapter` is null) with the DevTools
protocol relaying the page's console. It runs: vadd and vaddi bit exact, the assert traps, exit 0, but on **SwiftShader**, the CPU Vulkan:
the snap's Chromium does not offer the NVIDIA adapter to WebGPU. What was tried: the NVIDIA ICD made visible to the snap
(`VK_DRIVER_FILES` with a copy of `nvidia_icd.json` under `$HOME`, which the snap can read): ANGLE then fails with `VK_KHR_xcb_surface` not
supported (the NVIDIA Vulkan under Xvfb cannot present) and the GPU process exits; the NVIDIA ICD plus the snap's SwiftShader ICD:
ANGLE takes SwiftShader and Dawn offers only SwiftShader; `--use-vulkan=native`, `--use-webgpu-adapter=discrete`, `--use-angle=vulkan`,
`DefaultANGLEVulkan,VulkanFromANGLE`: no hardware adapter in any combination. The exact failure is the sandbox and the display, not the
code: a Chromium outside the snap on a real X session (or with a GPU-backed Xorg) is the run to make; node with Dawn on the same GPU is the
hardware run and it agrees bit for bit.

## 7. What this gives the Metal route

Dawn and wgpu both compile WGSL to MSL (Tint's and naga's MSL writers), so this target reaches the owner's Mac Studio with no fibber work:
`fib-gpu-webgpu` on macOS over wgpu-native's Metal backend (the same shim, `WGPU_BACKEND=metal`), or the wasm host in Safari 26 (WebGPU
shipped). gpu.md 4.4's route (a) (an MSL printer in fibber) is no longer needed for the GPU to be reached from a Mac; it stays the route if
the MSL must be fibber's own. Not run: the Mac was not used in this package (`scripts/mac-check.sh` is the place to add it).

## 8. Tests and tools

* `compiler/tests/native/wgsl-emit.sh` (no GPU): the row; the WGSL's content (the four entries and signatures, the binding model, the
  overrides, `fma(`, `fibw_sadd_ovf(`, the flag store, the index space; no `i64`/`f64`/`ptr<`/runtime/libc) and its equality with the golden
  compiler/tests/native/wgsl/kernels.wgsl (`--update` after an intended change); naga's validation (the tool of fetch-webgpu-tools.sh;
  skipped with a note when absent); the host run of the kernels file; the eight 857x cases on the target; the refusals (`--emit ptx/llvm/obj`
  on the row, `--emit wgsl` on nvptx64 and for the host, an executable); four planted faults in the WGSL caught (a binding off by one, a fixed
  workgroup size, a trap without the flag, a checked `+` as a plain `+`). Output on this box: "wgsl-emit: every check holds".
* `scripts/mutant-webgpu.sh` (no GPU, a stage-2 build per mutant): six mutants of the emitter's source, each caught by wgsl-emit.sh.
* `scripts/webgpu-agree.sh [--browser]` (a GPU, the drivers, node, Chromium): section 2.
* `fib-gpu-webgpu/scripts/test.sh --plant` (a GPU): section 5.2.
* `scripts/fetch-webgpu-tools.sh` (ADR 0020): wgpu-native v29.0.1.1 (sha256 recorded), naga-cli 30.0.1 built from its crates.io tarball
  (sha256 recorded), the `webgpu` npm package 0.6.2 (the registry tarball's sha256 recorded; its postinstall fetches Dawn's prebuilt binary
  for the platform, the package's own step), into `~/.cache/fibber-scratch/tools/`.

## 9. Open items

1. The C-callback form (5.2): the one language addition this backend asks for; then the shim shrinks to the descriptor fills, which
   `fibgen` removes in turn.
2. Timestamp queries in the drivers (the bench is wall clock), a persistent staging buffer for downloads, a uniform ring for launches.
3. Structured printing for the barrier's uniformity (3.6), shared memory, atomics, f16 as they land in lIR.
4. The hardware Chromium run (6.2) on a machine whose Chromium is not sandboxed from the GPU; the Mac (section 7).
5. The `kernels` module name is used by both examples/gpu and examples/webgpu: a program that includes both directories must put
   examples/webgpu first (the drivers' scripts do); renaming one is a one-line change for the lead.
