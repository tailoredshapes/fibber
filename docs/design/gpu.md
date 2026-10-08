# fibber on a GPU: a kernel target, a device protocol, drivers

Status: **phase 1 built** (GPU-2, 2026-10-07), on the decision of docs/design/decisions-2026-10-04.md (GPU): the kernel target and `fib.gpu` are
core, `defkernel` is the deliberate opt-in, a program with a kernel built for a platform with no kernel target is refused. Section 0 is what
is built and where; sections 1 to 10 are the design as GPU-1 wrote it, kept as the record (what they call missing is in section 0's table
when it landed). The driver is `fib-gpu-cuda` (`ssh://git@localhost:2222/tailoredshapes/fib-gpu-cuda.git`, tag `v0.1.0`). Every number
was measured on this machine (RTX 4080 SUPER, 16 GB, driver 610.57.04, CUDA 13.4, LLVM 21).

## 0. As built (phase 1, GPU-2)

| what | where | the rule |
|---|---|---|
| `defkernel` as a core form | expand.top `kernel-defun`; types.lower `defun-kernel?`; types.decls `kernels` | spec/syntax.md 3.22: `(defkernel name (param*) body+)` is `(defun name (param*) :kernel -> unit (unsafe (do body+)))`; the mark is read by the checker and the emitter |
| the kernel-subset checker | compiler/own/kernel.fib, after inference, before the ownership pass | spec/types.md 2.17: one error per kernel at its source position, the path when it is in a reached function (`:kernel-pure` is inferred); cases/stdlib 8530-8542 (thirteen refusals), scripts/mutant-gpu.sh (ten rules removed one at a time, each caught by a case) |
| the `gpu/*` builtins | compiler/emit/lower/gpu.fib; lib/fib/gpu.fib (facade), lib/fib/gpu/host.fib | spec/types.md 2.17: `(sreg ..)`, `(barrier)`, `(global shared ..)` on a kernel target; the host index state, nothing, a static buffer on the host; `host-launch` runs a kernel over a grid on the CPU (the one-language guard: cases 8545-8547, examples/gpu/gpu.fib: vadd and gemm match a CPU loop, 0 mismatches) |
| shared memory in lIR | spec/lir.md 6.9a `(global shared NAME T init)`; lir.parse, lir.print, native.lower `add-shared-global` | address space 3, `@NAME` the generic address; `gpu/shared` emits one per call site; the GEMM through it: 17.1 TFLOPS at 4096 (section 0.1) |
| the kernel table and module | emit.compile `kernel-table`, `compile-kernels`; native.kernel reads `fib.kernel.NAME` | the `gpu-kernel-` name convention of GPU-1 is gone; the extractor's refusal (runtime, libc) is the backstop below the checker; the stack entries of a scalar cell (`fib.stack-init`, `fib.stack-end`) get device versions (three stores, no trace) |
| the launch ABI | native.kernel `kernel-signatures`: `// fib.kernel-sig NAME: kind..` lines before the PTX; lib/fib/gpu/device.fib `parse-signatures`, `check-args`, `check-dims` | a launch with the wrong count or kind is `:arguments` naming the kernel and the position, before `cuLaunchKernel` (case 8548; the driver's `--wrong-args`; the contract) |
| `--kernel-target TRIPLE` (FIB_KERNEL_TARGET) | driver.commands `build-with-kernels` | the program is lowered for the kernel target (the PTX to `OUT.ptx`, beside the executable: the inspectable by-product), then for the host with the PTX embedded (`gpu/program-ptx`, through FIB_KERNEL_PTX); `fibc run` and `fibc cases` run a kernel's body on the host (the development loop), `fibc build` of an executable needs the target or refuses |
| the rejection on a platform with no kernel target | driver.commands `build-checked` | `fibc: the program has the kernels vadd, gemm (defkernel) and x86_64-pc-linux-gnu has no kernel target: build with --kernel-target nvptx64-nvidia-cuda (or FIB_KERNEL_TARGET), which embeds the kernels' PTX for a driver; there is no CPU fallback for a kernel`; `--kernel-target none` the same; wasm and JS hosts: the same message (no driver route; WebGPU is not designed, section 8) |
| the `Device` protocols | lib/fib/gpu/device.fib: `Platform Device Module Kernel Buffer Stream Event`, `GpuError` (a kind), `DeviceInfo`, `Arg` | a program takes a `(dyn Platform)` from a driver and never names CUDA; ADR 0011 rule 4 (the allowance): `fib.gpu` declares no extern |
| the contract | lib/fib/gpu/contract.fib (`GpuContract`: 6 scenarios; `GpuTrapContract`: the device assert, its own process), fault.fib (`:skip-lent-check`, `:swallow-assert`, `:ignore-signature`), contract-kernels.fib | fib-gpu-cuda v0.1.0 passes 7 of 7 and every fault is caught (`scripts/test.sh: 0 failed`) |
| the gate | compiler/tests/native/gpu-emit.sh (20 checks, no GPU) in scripts/tools.sh's full mode | the driver's GPU tests stay in its repository (CI there needs a GitLab runner on the GPU box: shell executor, `libcuda.so.1`, cuBLAS, a fibc of GPU-2) |

**Host semantics of the builtins** (documented, consistent): outside `host-launch` a kernel is the one thread (0,0,0) of a 1x1x1 grid
(ids 0, sizes 1); inside, the index `host-launch` is on; `gpu/barrier` is nothing (threads run one after another, so a kernel that needs
another thread's write before a barrier, the shared-memory GEMM, does not reproduce on the host: the device result is checked against the
CPU's `fib.tensor` instead); `gpu/shared` is one static buffer per call site.

### 0.1 Measured (GPU-2, fib-gpu-cuda `examples/gemm.fib`, median of 5; 3 at 4096; every result checked against the CPU)

| n | CPU `fib.tensor` mmul, one core | fibber `gemm` (register 4x4) | fibber `gemm-smem` (64x64 shared-memory tiles) | CUDA C `gemm_reg` | CUDA C `gemm_smem` | cuBLAS | fibber smem / cuBLAS |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1024 | 73 GFLOPS (29 ms) | 8095 (0.265 ms) | 12495 (0.172 ms) | 7582 | 12633 | 25575 (0.084 ms) | 49% |
| 2048 | 136 (126 ms) | 10645 (1.61 ms) | 16644 (1.03 ms) | 10619 | 16643 | 35099 (0.49 ms) | 47% |
| 4096 | 132 (1045 ms) | 10923 (12.6 ms) | 17099 (8.04 ms) | 11042 | 16962 | 36229 (3.79 ms) | 47% |

All four kernels are bit for bit the CPU's (the same fma order per element); fibber's shared-memory kernel (examples/gpu/kernels.fib
`gemm-smem`: 16 accumulators as scalar cells, a counted inner loop of 16 LLVM unrolls) is the CUDA C one's speed, 1.57x the register
kernel, the phase-2 target (16 TFLOPS) met in phase 1. Vector add on 2^22: 0 mismatches, 682 GB/s. A launch through the protocol with the
signature check, followed by a sync: 6.1 to 6.2 us (fib-gpu-cuda `examples/launch-cost.fib`, 20000 launches); the host-side check is a loop
over the arguments, below the noise of the sync. The one-language guard: `fibc run examples/gpu/gpu.fib -I examples/gpu` prints `host vadd
100000: 0 mismatches; host gemm 128: 0 mismatches`.

**Not in phase 1:** `with-launch`, the static lend of a device buffer (the run-time lent state in the driver is the backstop; the design
of 3.3 stands: a scoped `DeviceBuffer` over the `with-view` machinery, phase 3); a typed launch form checked at compile time against the
`defkernel`'s parameters (the run-time check covers every launch; the form needs the expander to see the kernel's types across modules);
the barrier-uniformity check; pinned and async transfers (GPU-3 did atomics and the reductions: section 12); f16/bf16; the SPIR-V row.

## 1. Recommendation

**Do it, as a kernel target plus drivers, in the order below; the prototype says the architecture is right and the cost is modest.** A
GPU is not a target of fibber in the sense of docs/adr/0008 (no runtime, no tail calls, no tasks): it is a **kernel target**, a third
class beside supported and portability targets, whose output is the PTX (later SPIR-V, Metal) of a program's kernels and never an
executable, and whose kernels are launched by a driver (ADR 0011) that the core sees through a `Device` protocol. The prototype takes a
kernel written in ordinary fibber (raw pointers, scalars, `loop`, `simd/fma`, `trap`) through the unchanged emitter and lair to PTX, and a
driver written in fibber launches it: vector add on 2^24 elements is bit for bit the CPU's; a register-tiled f32 GEMM written in fibber runs
at **10.9 TFLOPS at n = 4096, 30% of cuBLAS (36.1 TFLOPS) and within 5% of the same kernel written in CUDA C and compiled by nvcc**, 90x
the one-core CPU `fib.tensor` kernel on this box (122 GFLOPS); a device trap (a fibber `trap`, a checked `+`) is the `Err` of the next
`sync`; misuse of a device buffer (download while a kernel runs, release twice, use after release, one buffer as two arguments) is an `Err`.
The compiler change was 24 files and about 600 lines, no change to the front end; the driver is 330 lines of fibber over 32 externs. What is
missing (section 6) is a checker for the kernel subset (today the refusal is at the lIR level, with the path that reaches the runtime),
shared memory (address spaces in lIR, the one item the GEMM number waits on: the shared-memory CUDA C kernel is 1.55x the register one), the
`:kernel` marker as a core form (today a macro and a name), and the launch ABI (a typed launch). Phase 1 (section 10) is three to four
agent-weeks; cuBLAS from fibber through the driver, the quickest win for a numerics user, already works and is the baseline.

## 2. What was measured

`fib-gpu-cuda/examples/gemm.fib`, f32, square, row major, median of 5 launches (3 at 4096), GPU events around the launch; every result
compared with the CPU's `fib.tensor` `mmul` element by element (the same inputs, section 2.1 on tolerance):

| n | CPU `fib.tensor` mmul, one core | fibber kernel `gemm` (register-tiled 4x4, PTX by fibc) | CUDA C, the same tiling (nvcc) | CUDA C, 64x64 tiles in shared memory (nvcc) | cuBLAS SGEMM | fibber / cuBLAS |
|---:|---:|---:|---:|---:|---:|---:|
| 1024 | 126 GFLOPS (17.1 ms) | 8126 GFLOPS (0.264 ms) | 7632 | 12633 | 25891 (0.083 ms) | 31% |
| 2048 | 136 (126 ms) | 10641 (1.61 ms) | 10619 | 16643 | 35103 (0.49 ms) | 30% |
| 4096 | 122 (1123 ms) | 10930 (12.6 ms) | 11065 | 16947 | 36109 (3.81 ms) | 30% |

* **Vector add**, 2^24 f32: 0 mismatches of 16,777,216 (bit exact); 0.32 ms for 201 MB of traffic, 626 GB/s against the 4080 SUPER's
  736 GB/s peak (the first launch after module load is 0.80 ms).
* **Transfers**, pageable host memory over PCIe 4.0 x16: upload 13.6 to 15.7 GB/s; download 0.4 to 2.4 GB/s. The download lands in a
  freshly allocated host array (page faults included); pinned host memory (`cuMemHostAlloc`) is the fix and is on the list (section 6).
* **The emitter against nvcc:** fibber's `gemm` and the CUDA C `gemm_reg` are the same algorithm (a 4x4 register tile per thread, one
  fma chain per element in increasing k) and produce bit-identical results; their speeds are within 5% both ways. The PTX fibc writes is
  what a hand would write (section 4.1): `mad.lo`, `fma.rn.f32`, `ld.global.b32`, one `cvta.to.global` per pointer parameter.
* **Device trap:** `assert_positive` on an input with a zero: `sync` returns `Err 719 unspecified launch failure`; on a positive input,
  `Ok`. The CPU box lost nothing: the process goes on and reports.
* **CPU numbers to beat** (docs/shootout/tensor.md, another machine): f32 matmul 95 to 167 GFLOPS one core; this box's `fib.tensor` is 122
  to 136. The GPU kernel is 80 to 90x one core; cuBLAS 265 to 295x.

### 2.1 Tolerance, and why bit exact is possible

`fib.tensor`'s f32 GEMM is "one chain of fused multiply-adds over the inner index in increasing order" per output element
(lib/fib/tensor/gemm-fma-f32.fib). The fibber kernel computes each element the same way, so the two agree bit for bit, and the check demands
it (tolerance 0). cuBLAS and the shared-memory kernel sum in other orders: the f32 error bound of a dot product of length n is
n·2^-24·Σ|a_k b_k|; the inputs are positive, so the bound at n = 4096 is 2.4e-4 relative; the check allows 5e-4; the worst seen was 3.4e-6.

## 3. The kernel subset

A kernel is a fibber function with a restricted body. The restriction is what the GPU lacks, not what fibber lacks: no allocator, no
reference counts, no tasks, no libc, no strings, no closures over heap data, no recursion of unbounded depth, no tail-call guarantee.

### 3.1 The form

```clojure
(ns main (:require [gpu :as gpu]))
(gpu/defkernel vadd (a: ptr b: ptr c: ptr n: i64)
  (let ((i (gpu/global-id 0)))
    (when (< i n)
      (gpu/f32-set! c i (+ (gpu/f32-at a i) (gpu/f32-at b i))))))
```

`defkernel` names a kernel entry: `unit`-valued, parameters scalars (`i8` to `i64`, `f32`, `f64`, `bool`) and raw `ptr`s (device buffers),
its body in `unsafe`. In the design it is a core form that marks the `defun` `:kernel` (like `:private`, expand/private.fib) and the
emitter reads the mark; in the prototype it is a macro of `examples/gpu/gpu.fib` that makes a `defun` named `gpu-kernel-NAME`, which
`native.kernel` recognises (section 9, delta 1). The same file builds and runs for the host: nothing of the host calls a kernel.

What a kernel body may contain (the checker of section 6 enforces it; the prototype enforces it at the lIR level, section 4.2):

| allowed | what it lowers to |
|---|---|
| integer and float arithmetic, comparisons, `cond`/`if`/`when`, `let`, `loop`/`recur` (a loop is a branch, never a tail call) | PTX arithmetic and branches; `recur` is `br` in lIR already |
| `simd/fma` on scalars, `sqrt`, `fabs`, `floor`, `ceil`, `fmin`/`fmax` and the rest of the lIR fma family | `fma.rn`, `sqrt.rn`, libdevice-free intrinsics NVPTX has |
| `load-f32`/`store-f32`, `load-i32`, .. through `ptr+` on a parameter (the prototype's `gpu/f32-at`, `gpu/f32-set!`) | `ld.global`/`st.global` at natural alignment (3.3) |
| calls of other functions of the program or the library whose bodies are in the subset (inlined by LLVM; `gpu/global-id` is one) | device functions |
| checked `+`, `*`, an index check, `(trap "..")` | a device trap: `trap;` (3.4) |
| `(gpu/local-id d)`, `(gpu/group-id d)`, `(gpu/group-size d)`, `(gpu/grid-size d)`, `(gpu/global-id d)`, `(gpu/barrier)` | `(sreg tid.x)`, `(sreg ctaid.x)`, `(sreg ntid.x)`, `(sreg nctaid.x)`, `(barrier)` (spec/lir.md 6.9a) |
| not yet: `(gpu/shared f32 N)`, a block-local window of N elements; f16/bf16; atomics; a warp reduction | section 6 |

Refused: anything that reaches the runtime (`array`, `str`, a Vec, `println`, `spawn`, a closure that allocates, `(Simd T n)` values: the
SIMD lane model is the CPU's, 5.3), any `extern` but the index space, recursion (LLVM's NVPTX inlines; a recursive device function needs a
stack and is refused), a `tailcall`. The refusal names the path: `the kernel reaches the runtime function fib.array-alloc <- f.gpu-kernel-k`.

### 3.2 The index space

One invocation per index of a grid: the host launches `grid` blocks of `block` threads, each one to three dimensions; a thread's
`(gpu/global-id d)` is `group-id * group-size + local-id` along `d`. A kernel checks its own bound (`(when (< i n) ..)`): the grid is
rounded up by the host, the tail of the last block does nothing. `(gpu/barrier)` is the block's barrier (`bar.sync 0`); it must be reached
by every thread of the block or by none (a divergent barrier is undefined in PTX: the checker's rule, section 6, is "a barrier is not under
a condition that depends on the index"). Shared memory, when it lands, is a window of fixed size declared in the kernel and shared by the
block: `(gpu/shared f32 1024)` returns a `ptr` into address space 3 (section 6, delta 3).

Windows and index spaces: `with-tiles` (docs/design/exclusive-views.md) lends disjoint windows of one host buffer to a body; a GPU grid is the
same idea with the GPU as the body: each block owns a tile of C and reads tiles of A and B. The design's `launch-tiles` (section 5) makes
that correspondence a function: a `WinPair` per block becomes `(group-id, tile offset)` arguments, and the exclusivity the checker proves
for `with-tiles` (no two lends of one place) is what makes the kernel's writes race-free.

### 3.3 Memory and ownership

Device memory is explicit: a `(DeviceBuffer T)` is allocated, filled by `upload`, read by `download`, released; there is no implicit
transfer, no unified memory in the protocol (a driver may use it underneath). The rules, borrow first:

1. **A launch lends.** `(launch s k grid block args)` lends every buffer among `args` to the stream `s` until `(sync s)` returns. While
   lent, `download`, `upload`, `release` and a second lend of the same buffer are refused (a kernel that reads and writes one buffer through
   two parameters is a race; a kernel that wants in-place work takes one pointer).
2. **Release is final.** `(release b)` ends the buffer; a second release or any use after it is refused.
3. **No buffer outlives its device.** `close` of a `Device` with live buffers is refused (the prototype does not check this yet).

The prototype checks 1 and 2 at run time (`state` in the buffer, 13 cases in `fib-gpu-cuda/specs/ownership-spec.fib`; each an `Err`,
never a crash). The static form is the scoped lend of exclusive views (spec/types.md 1.10, 6.15): a `DeviceBuffer` is scoped
(`(impl Scoped (DeviceBuffer t) ..)`), so it is borrowed and never consumed; `with-launch` lends it for the body as `with-view` lends a
window, and the body cannot read the owner (case 339's rule) or lend it twice (case 338's); `sync` is the scope's exit. That gives "download
while a kernel runs" and "two lends" as compile-time rejections with no new checker rule; "release twice" and "use after release" become
impossible when `release` is the scope exit of `with-buffer`. Section 10 schedules it after the kernel checker.

Alignment: the kernel subset indexes elements, never bytes. The emitter writes `(align 1)` for every raw-pointer access (spec/syntax.md
3.15: unaligned addresses are allowed on the CPU); on NVPTX a byte-aligned f32 load is four `ld.global.b8`. On a kernel target lair reads
`(align 1)` on a scalar as the scalar's size (native.lower.memory `kernel-align`), and the PTX has `ld.global.b32`. The design's accessors
carry the alignment themselves (a `gpu/f32-at` builtin emits `(align 4)`), and the lair rule goes.

### 3.4 Errors

Every device call returns a `Result`. A device-side assert is a kernel's `trap` (fibber's `(trap "..")`, a checked `+` or `*` that
overflows, an index check): the runtime's trap entries (`fib.trap`, `fib.trap-c`, `fib.trap-index`) become `(trap)` in the kernel module,
`trap;` in PTX, and the host sees the `Err` of the `sync` that follows (CUDA: 719 "unspecified launch failure" on this driver, 715
"illegal instruction" on others). The message is lost on the device (no `write`), which is why a kernel's trap should be rare and its
inputs checked on the host; a `printf`-style device log is not planned. After a trap the CUDA context is unusable: the `Device` is closed
and reopened by the program, as CUDA demands.

## 4. The kernel target and the host API

### 4.1 The target table

A row with a new class (compiler/types/targets.fib `row-nvptx64-cuda`, `kernel-row?`):

| triple | ptr | threads | vector, CPU | fma | -O0 floor | tailcc / kind | reloc | object, linker | libc | status | support |
|---|---|---|---|---|---|---|---|---|---|---|---|
| nvptx64-nvidia-cuda | 64 | none | 128, sm_89 | yes | 1 | 0 (C) tail hint | static | ptx, none (a driver loads it) | none (all 50 lacking) | emits PTX | kernel target |

`fibc build --target nvptx64-nvidia-cuda --emit ptx FILE -o K.ptx` writes the PTX of FILE's kernels; `--emit llvm` their LLVM IR; `--emit
obj` and an executable are refused (PTX is text; nothing links). `FIB_TARGET_CPU=sm_120` picks another SM. ADR 0008's exception, stated: a
kernel target has **no tail calls** (NVPTX has no `tailcc`; a kernel's loops are branches, a kernel never tail-calls) and **FMA always**;
it is neither supported nor a portability target, it is a kernel target, built without `--allow-unsupported` because nothing of the rule's
concern (a tail call between functions of any signature) can occur in it. The NVPTX backend is initialised like the others (four externs:
it has no assembly parser). `scripts/llvm-static.sh` lists `nvptx`.

### 4.2 How a kernel module is made (the lIR route 2.3 of autodiff.md)

`compiler/native/kernel.fib`. The program is lowered as usual (every pass, every check: the kernels are the emitter's export roots on a
kernel target, `emit.compile/kernel-specs`), then the kernel module is extracted from the lIR: the `gpu-kernel-NAME` defines and everything
they reach through `@` references; the entry is renamed `NAME`, made external and given `kernelcc` (LLVM's `ptx_kernel`); the runtime's trap
entries become `(trap)`; the index-space externs `gpu.sreg.R` and `gpu.barrier` become `(sreg R)` and `(barrier)`; names become PTX names
(`f.main.gpu-kernel-vadd` reaches the driver as `vadd`, `-` as `_`); the host's `(target (cpu ..))` form is dropped; string constants come
along (a trap's message, unused). Anything else of the runtime (`fib.*`) or of libc reached is a refusal with the path. The module is parsed
and checked again, so lair sees a module like any other, and lowered with the NVPTX machine; the "assembly" of that machine is PTX.

The PTX of `vadd` (examples/gpu/kernels.fib), as fibc writes it:

```
.visible .entry vadd(.param .u64 .ptr .align 1 vadd_param_0, .., .param .u64 vadd_param_3)
  ld.param.b64 %rd4, [vadd_param_0];  cvta.to.global.u64 %rd9, %rd4;  ..
  mov.u32 %r1, %ctaid.x;  mov.u32 %r2, %ntid.x;  mul.wide.u32 %rd11, %r1, %r2;  mov.u32 %r3, %tid.x;  ..
  setp.ge.s64 %p1, %rd13, %rd10;  @%p1 bra $L__BB0_2;
  ld.global.b32 %r4, [%rd3];  ld.global.b32 %r5, [%rd2];  add.rn.f32 %r6, %r4, %r5;  st.global.b32 [%rd1], %r6;
```

### 4.3 The host API: the `Device` protocol

The core gets a protocol; a driver implements it (ADR 0011: the externs live in the driver). The prototype is `fib-gpu-cuda`'s `cuda`
module, whose shape is the protocol's:

| | prototype (`cuda`) | protocol (design) |
|---|---|---|
| enumerate, open | `(device-count)`, `(open ordinal)` -> `Device` (name, SM, context); an `Err` names what is missing | `Device` protocol: `open`, `close`, `name`, `capability` |
| compile | none: the PTX is text from `fibc build` (the driver assembles it) | `(compile d source)`: PTX, SPIR-V or MSL text to a `Module`; a driver that needs a compiler (Metal) runs it here |
| load, kernels | `(load-ptx d text)`, `(load-image d bytes)` (a cubin), `(kernel m "name")` | the same; `kernel` returns the signature the PTX carries (6, the launch ABI) |
| memory | `(buffer d bytes)`, `(upload-f32 b xs)`, `(download-f32 b n)`, `(fill-bytes b v)`, `(release b)` | `(DeviceBuffer T)` typed by element; `upload`/`download` over `(Array T)` and over windows (`with-view`); `map` for unified memory where a driver has it |
| launch | `(launch s k grid block args)`, args `ABuf`/`AI64`/`AI32`/`AF32` | `(launch s k grid block args)` with the args checked against the kernel's signature; `launch-tiles` over a `with-tiles` plan |
| streams, events | `(stream d)`, `(sync s)`, `(event d)`, `(record e s)`, `(elapsed-ms e0 e1)` | the same; `sync` returns the lent buffers |
| errors | `CudaError (call code message)`; `-1` the driver's own check | `DeviceError`, one per driver, under one protocol |

No global state: a `Device` holds its context, a `Stream` the buffers it has been lent, a `Buffer` its state. `cuInit` is called by
`device-count`/`open` (idempotent in the driver API); the primary context is retained per `open` and released per `close`.

### 4.4 Drivers

* **fib-gpu-cuda** (built, v0.0.1): the driver API (`libcuda`, `:lib "cuda"`, 32 externs: `cuInit`, `cuDeviceGet*`,
  `cuDevicePrimaryCtxRetain/Release`, `cuModuleLoadData`, `cuModuleGetFunction`, `cuMemAlloc/Free/cpyHtoD/DtoH` (`_v2`), `cuLaunchKernel`,
  `cuStream*`, `cuEvent*`, `cuGetErrorString`), not the runtime (no static initialisation, every handle explicit). `cublas` beside it (SGEMM,
  `:lib "cublas"`): the baseline, and the quickest win for a numerics user. A kernel's PTX is assembled by the driver at load; a cubin from
  nvcc loads through the same call (the driver on this box takes PTX up to ISA 8.x: nvcc 13.4's 9.4 is refused with 222, a cubin is not).
* **fib-gpu-vulkan** (next, after CUDA): SPIR-V from LLVM 21's `spirv64` backend (it is in `llc --version` here), kernels as compute
  shaders, `vkCreateComputePipelines`, buffers through `VkBuffer` and host-visible memory. Runs on NVIDIA, AMD, Intel and (through
  MoltenVK) Apple. The LLVM SPIR-V backend emits the OpenCL flavour (kernels, not shaders): a Vulkan driver needs the `clspv`-style
  translation or the OpenCL-on-Vulkan layer; **the direct route is OpenCL** (`libOpenCL`, `clBuildProgram` from SPIR-V through
  `cl_khr_il_program`), which the same SPIR-V feeds with no translation. Size: a driver of CUDA's size, plus the SPIR-V row (a second kernel
  row: `spirv64-unknown-unknown`, address spaces and `spir_kernel` instead of `ptx_kernel`).
* **fib-gpu-metal** (Apple): LLVM has no Metal backend. Two routes: (a) MSL source from fibber, a second emitter (the kernel subset is small:
  an MSL printer of the kernel's lIR is about the size of `lir.print`, 400 lines, plus types and the index-space names; estimate two
  agent-weeks), compiled by `MTLDevice.newLibrary(source:)` at run time (the driver has the compiler); (b) SPIR-V to MSL through
  SPIRV-Cross (a C++ library; a driver that links it: no fibber code, but a C++ build). Route (a) keeps the toolchain fibber's and gives
  the owner's Mac Studio (M1 Ultra, docs/design/aarch64.md) a target; (b) is quicker to try. Not started: the Mac was not used for this
  package.
* **Which first:** CUDA on this box (done), then OpenCL/SPIR-V (one driver covers three vendors and the Mac through its OpenCL, which Apple
  deprecated but ships), then Metal by route (a) if the Mac matters.

## 5. How autodiff and the tensor library plug in

docs/design/autodiff.md 2.3 names the lIR route: a tensor op is a function; a second backend takes the same lIR. The kernel target is that
backend for the ops that are kernels. The plan:

1. **Device tensors.** `(Tensor t)` gains a storage variant whose buffer is a `(DeviceBuffer t)` (storage.fib's `from-parts` with a device
   buffer instead of an `Array`), with the same `meta` (offset, dims, strides). `t/to-device` and `t/to-host` are the explicit transfers;
   every other function refuses a mixed call (`mmul of a host tensor and a device tensor`) until the graph layer moves them.
2. **Kernels of the ops.** `t/matmul`, `t/add`, `t/mul`, the reductions, the activations, the Adam step (optim.fib): each gets a
   `defkernel` beside its CPU kernel, in the kernel subset, selected by the storage variant. The GEMM kernel exists (`gemm`, 3.1); the
   elementwise ones are one-liners over `global-id`; the reductions need a block reduction (shared memory, section 6) or atomics.
3. **Mixed graphs.** autodiff's tape records ops over tensors; a device tensor's ops run on the device, a transfer is an op on the tape
   with its own adjoint (identity), so a graph with a host head (data loading) and a device body (the model) differentiates as one.
4. **cuBLAS/cuDNN where they win.** The `MatrixKernel` protocol of linalg.fib is the seam: an instance over device tensors calls `cublas`
   (36 TFLOPS) and keeps fibber's kernel for the shapes cuBLAS handles badly (small, odd strides) and for every other op.

What this buys: the numerics user gets the GPU through the tensor API they have; the kernel author gets a kernel language that is fibber.

## 6. What the language already gives, and what is missing

**Already there, used by the prototype:**

* **ownership** for transfers: a device buffer is a value with one owner; a launch lends, a sync returns; the scoped-type rule (1.10, 6.15)
  is the static form of the lend;
* **exclusive windows** (`with-view`, `with-tiles`): the model of a kernel index space over one buffer (3.2);
* **the lIR**: a second backend was the plan (autodiff.md 2.3) and it held: the kernel module is lIR, lair lowers it, nothing of the emitter
  changed to produce it; the whole change below the lIR is one calling convention, two forms and an alignment rule;
* **the target table**: a row is data; the kernel class is three predicates;
* **raw pointers and `unsafe`**: the kernel's memory model is theirs; `simd/fma` on scalars is the fused multiply-add; `loop`/`recur` is
  the loop; checked arithmetic is the device assert;
* **the export mechanism** (`--export`, FIB_EXPORTS): a kernel is an export whose wrapper is dropped;
* **drivers and `:lib`** (ADR 0011): the CUDA binding is a module in its own repository and the core has no extern.

**The SIMD lane model is not the warp.** `(Simd f32 8)` is one thread's vector register: eight lanes, one instruction, the compiler chooses
the width (`native-lanes`). A warp is 32 *threads* that happen to execute in lockstep: each has its own registers and program counter
(since Volta), divergence is legal, and the lanes are the index space, not a value. Mapping `(Simd T 32)` to a warp would make every scalar
a vector and every branch a mask, which is the GPU's job (the hardware masks divergent threads). So a kernel is scalar code and the GPU
supplies the parallelism; `Simd` values are refused in a kernel (NVPTX has `.v2`/`.v4` loads but no 256-bit arithmetic). Where SIMD
*does* map is the CPU side of the same program: the host code around a launch keeps its vectors.

**Missing, in the order the phases need them:**

1. **The `:kernel` marker as a core form** (section 9, delta 1): `defkernel` is a macro and a name convention today.
2. **The kernel-subset checker**: in `own/` or a new `kernel/` pass over the typed AST, before emission: refuse an allocation, a closure
   that captures heap data, a `str`, a `Simd` value, a protocol call whose instance is not in the subset, a `spawn`, a recursion, a
   `barrier` under an index-dependent condition. Today the refusal is at the lIR level (4.2) with a path; it catches everything but names
   lIR symbols, not source positions. Size: M (three to five days).
3. **Address spaces in lIR**: `(global shared NAME [N x float] ..)` lowered with `LLVMAddGlobalInAddressSpace(.., 3)` and an
   `addrspacecast` at each `@NAME` use (LLVM's `InferAddressSpaces` then makes `ld.shared`); a `(gpu/shared T N)` builtin that emits one
   per kernel. The measured gain: 1.55x on GEMM (16.9 against 10.9 TFLOPS). Size: S-M (two to three days: the lIR grammar, the lowering, the
   builtin with its spec row and case, ADR 0014).
4. **The launch ABI**: the PTX carries no signature the driver can read; `fibc` writes a sidecar (a JSON or lIR `declare` of each kernel's
   parameters beside the PTX, or a comment the driver parses) and `launch` checks the arguments against it. Today a wrong argument count
   is a kernel that reads what it was not given. Size: S.
5. **f16 / bf16**: lIR has no half type; PTX has `f16`, `bf16` and their `fma`; the tensor-core path (`mma.sync`, `wmma`) needs them and
   an intrinsic family. Size: M for the types, L for tensor cores.
6. **Atomics on device**: lIR's `atomicrmw` lowers on NVPTX as it is (global and shared); the kernel subset needs the `atom` library
   functions unlocked and a case. Size: S.
7. **Reductions**: a block reduction over shared memory with `barrier`, a warp shuffle (`shfl.sync`, an intrinsic family), `atomicAdd` for the
   final step. Size: M (after 3).
8. **Async transfers and pinned memory**: `cuMemHostAlloc`, `cuMemcpyHtoDAsync` on a stream (the externs are declared already), a host
   buffer type that is pinned; download at 0.4 GB/s is the cost of not having it. Size: S.
9. **The `Device` protocol in the core** (4.3) and `(DeviceBuffer T)`: a `fib.gpu` module with the protocol and no extern, a driver
   implementing it. Size: M.
10. **Device tensors** (section 5). Size: M-L.
11. **A second kernel row** (SPIR-V) and its driver; **Metal**. Size: L each. The WGSL row (docs/design/webgpu.md) is the second row now and
    reaches Metal through Dawn's own WGSL-to-MSL path (webgpu.md 9); SPIR-V stays the OpenCL route.
12. **The name rule**: a kernel's PTX name is its fibber name with `-` as `_`; two kernels whose names differ only in a `-` collide (the
    extractor refuses the second; a better rule mangles).

## 7. Risks

* **Debugging without a GPU debugger.** A kernel that is wrong prints nothing; the prototype's method (the same kernel on the CPU through
  `fib.tensor`, bit for bit) is the method: every kernel ships with its host reference and a tolerance it justifies. `cuda-gdb` works on
  fibc's PTX (it is PTX with no debug info: `-g` would need `.loc` lines from the emitter's positions, which lIR has as `pos`); not tried.
* **Driver API versions.** `_v2` names and PTX ISA versions move with the toolkit: nvcc 13.4's PTX 9.4 was refused by driver 610 (the
  cubin was the answer). fibc's PTX is LLVM 21's ISA 7.8, which every driver since 2022 takes; a row per PTX version is not needed yet.
  The externs name the versioned symbols explicitly, so a header change cannot move them.
* **Nondeterminism of float reductions.** A reduction across threads orders its sums by scheduling unless the kernel fixes the order (tree
  reductions do); cuBLAS's result differed from the CPU's by up to 3.4e-6 relative and by nothing at n = 4096. The rule for the library:
  a device reduction documents its order or its bound, as 2.1 does, and the tests compare with a tolerance derived from it, never with
  bit equality unless the order is the CPU's.
* **The two-languages trap.** A kernel subset that drifts from the language becomes CUDA with parentheses. Guard: the subset is defined
  by refusal (what is not there), never by new syntax; `defkernel` is a `defun` with a mark; a kernel body compiles and runs on the host
  as a function (the test harness does that: `fibc run examples/gpu/kernels.fib`). What a kernel needs that the language lacks (shared
  memory, half floats) enters as ordinary builtins and types the CPU side can use too.
* **Portability across CUDA, Vulkan/OpenCL and Metal.** The index space, the memory model and the trap are the same in all three; the
  address spaces are (global, shared, local in each); what differs is the launch API (the driver's job) and the kernel container (PTX,
  SPIR-V, MSL: the row's job). The risk is in the backends LLVM lacks (Metal) and the flavour it has (SPIR-V OpenCL, not Vulkan shaders).
* **The owner's machine count.** One GPU box; the gate cannot run the GPU tests. `gpu-emit.sh` needs no GPU and belongs in the tools stage;
  `fib-gpu-cuda/scripts/test.sh` runs where there is a GPU, as the Mac scripts do for aarch64.

## 8. Alternatives considered

* **OpenCL C strings from fibber.** Kernels as C source in strings, compiled by the OpenCL runtime. No compiler work, immediate
  portability, and the two-languages trap in full: the kernel is C, with none of fibber's checks. Rejected as the main route; worth a
  driver (`fib-gpu-opencl`) for the C kernels people already have.
* **A CUDA C extern driver only** (cuBLAS/cuDNN from fibber, no kernels). Measured: cuBLAS through the driver is 36 TFLOPS, 3.3x fibber's
  kernel, and it took one module of 30 lines. This is **phase 0** and it is done; it is not the recommendation because every op that is
  not a BLAS call (the activations, the optimiser step, anything custom) would be a CUDA C file and a build, and because the kernel route
  costs little more than it did (section 1).
* **SYCL-style** (single source, the kernel a lambda captured by a launch, the compiler splitting host and device). This *is* the design:
  one file, `defkernel` beside `defun`, `fibc` splits by target. What SYCL adds, kernels as closures launched inline, is the design's
  `with-launch` form (3.3) once the checker exists; the prototype launches by name.
* **WebGPU/WGSL for the wasm/JS targets.** A WGSL printer of the kernel's lIR (the same size as the MSL one: 4.4) gives the browser target a
  GPU through `navigator.gpu`; the driver is JavaScript glue (docs/design/wasm.md 5.3's `jsbind`). **Built** (WEBGPU-1, docs/design/webgpu.md):
  the second kernel row `wgsl-unknown-webgpu`, the printer `native.wgsl` over the same kernel module, the JS glue and the wasm host, and
  `fib-gpu-webgpu` over wgpu-native for the native host; the same `defkernel` gives the CPU's result bit for bit through CUDA and WebGPU.
* **A general NVPTX target** (the runtime on the GPU: allocation, tasks). Rejected: no libc, no threads in the runtime's sense, no tail
  calls; targets.md 5.5 said "a kernel-only subset" and the prototype confirms the subset is enough.

## 9. The exact deltas the prototype took, and what the design changes

What the emitter needed: **nothing above the lIR.** The compiler change (two commits, 24 files, about 600 lines):

| where | delta |
|---|---|
| compiler/types/targets.fib | the row `row-nvptx64-cuda`; `kernel-row?`, `kernel-triple?`, `KERNEL-FUN-PREFIX`; `os-of-triple` knows `-cuda`; `row-buildable?` admits a kernel row |
| compiler/llvm/target.fib, core.fib | the four NVPTX initialisers; `LLVMGetTypeKind`, `LLVMGetIntTypeWidth` |
| compiler/lir/{types,ast,parse/ty,parse/control,check/expr,check/phi,print,dump}.fib | `CcKernel` (`kernelcc`); `KSreg`, `KBarrier` (`(sreg R)`, `(barrier)`) |
| compiler/native/lower/{lx,arith,expr,memory}.fib, lower.fib | cc 71; the intrinsics; the `kernel` flag of `Lx`; `kernel-align`; `kernel-refusal` off a kernel target |
| compiler/native/kernel.fib (new, 215 lines) | the kernel module: reach, rewrite, refuse, PTX names |
| compiler/native/api.fib, target.fib | `--emit ptx`; the kernel module on a kernel target; `requested-triple-row` |
| compiler/emit/compile.fib, types/infer/mod.fib | the kernels are the export roots on a kernel target; no `main` needed there |
| compiler/driver/args.fib, native/jit.fib, scripts/llvm-static.sh | the `ptx` word; the JIT's `CcKernel` arm; the `nvptx` component |

The design replaces three stand-ins of the prototype (each a delta the front end must learn):

1. **`defkernel` as a core form with a `:kernel` mark.** Today: a macro in examples/gpu/gpu.fib makes `gpu-kernel-NAME`; `native.kernel`
   finds the name. Design: expand/private.fib's mechanism records `:kernel` in `Names` (as `:private` is), `TypedProgram` carries it, the
   emitter's `kernel-specs` reads it, and the lIR function gets `kernelcc` from the emitter (emit/ir.fib `FnBuilder` grows a cc). The name
   convention goes; the extractor keeps the reach, the rewrite of traps and the refusal until the checker (delta 2) makes the refusal
   impossible.
2. **The index space as builtins.** Today: externs `gpu.sreg.R`, `gpu.barrier` in gpu.fib, rewritten by `native.kernel` to the lIR forms.
   Design: builtins `gpu/local-id`, `gpu/group-id`, `gpu/group-size`, `gpu/grid-size`, `gpu/barrier` (ADR 0014: a spec row and a case
   each) in a platform-layer module `fib.gpu` (ADR 0011: it declares no extern, it emits forms), lowered by emit/lower/builtins.fib to
   `(sreg ..)` and `(barrier)`. ADR 0003 checked: no function of `compiler/` or `lib/` is named `defkernel`, `global-id`, `local-id`,
   `group-size`, `grid-size` or `barrier`; `lib/fib/tls/hello.fib` has a `group-id` (TLS groups), which the qualified builtin `gpu/group-id`
   does not capture, but the plain name `group-id` must not become a core form.
3. **Alignment from the accessor, not from lair.** Today: `kernel-align`. Design: `gpu/f32-at` and friends are typed element accessors
   that emit `(align 4)`/`(align 8)`; the lair rule is removed with them.

## 10. Phases, sizes, the first milestone

Sizes in agent-weeks (an agent-week is five working days of one agent with the gate; the prototype took one agent-day for the compiler
and driver together, which calibrates these):

| phase | what | size | done when |
|---|---|---|---|
| **0** | cuBLAS/cuDNN from fibber through `fib-gpu-cuda` (the numerics quick win) | done (SGEMM); cuDNN conv/activation externs 0.3 | `examples/gemm.fib` prints 36 TFLOPS from cuBLAS and the checks hold |
| **1** | the kernel target as a product: deltas 1 and 2 of section 9 (the `:kernel` core form, the `fib.gpu` builtins), the kernel-subset checker (6.2), the launch ABI (6.4), `gpu-emit.sh` in the tools stage, the `Device` protocol in `fib.gpu` with the CUDA driver as its instance, spec rows (types.md, lir.md 6.9a decided) | 3 to 4 weeks | a kernel with a `str` in it is rejected at its source position; a launch with a wrong argument is an `Err` naming the parameter; `scripts/gate.sh --full` runs `gpu-emit.sh`; fib-gpu-cuda v0.1.0 implements the protocol |
| **2** | shared memory (6.3), atomics (6.6), reductions (6.7), pinned and async transfers (6.8); the GEMM reaches the shared-memory number | 2 weeks | fibber's GEMM at or above 16 TFLOPS at 4096 (45% of cuBLAS), a `sum` kernel within its documented bound, download above 10 GB/s |
| **3** | device tensors and the tensor ops as kernels (section 5), the `MatrixKernel` instance over cuBLAS, autodiff's tape over device tensors; `with-launch`, the static lend (3.3) | 4 weeks | a two-layer model of docs/design/autodiff.md trains on the device and matches the CPU within tolerance; cases/ownership has the accept/reject pairs of `with-launch` |
| **4** | the SPIR-V kernel row and `fib-gpu-opencl`; f16/bf16; then Metal by the MSL printer if the Mac matters | 3 + 2 + 2 weeks | the GEMM runs on a second vendor from the same source |

**The first milestone** is phase 1's first item: `defkernel` as a core form and the kernel checker, with examples/gpu/kernels.fib
unchanged in meaning and `gpu-emit.sh` extended with ten source-level rejections (a `str`, an `array`, a closure over a Vec, a `Simd`
value, a `spawn`, a recursion, a barrier under an index condition, a protocol call off the subset, a `tailcall`, a kernel with a non-scalar
parameter), each at its source position; plus the launch ABI so that the driver's `launch` is typed. That is two agent-weeks and it
removes the only part of the prototype that is a convention rather than a rule.

## 11. Checks run

* `compiler/tests/native/gpu-emit.sh` with the branch's F: 17 checks hold, 2 planted faults caught ("gpu-emit: every check holds").
* `compiler/tests/types/unit-targets.fib`, `compiler/tests/lir/unit-types.fib`, `compiler/tests/native/j-unit.fib` (the kernel-forms
  tests among them): each prints 0.
* `fib-gpu-cuda/scripts/test.sh --plant` (the GPU): vadd 2^22 bit exact; the short grid and the download before sync caught; no device is
  `Err 100`; the assert's two paths; the 13 ownership checks; GEMM at 1024 with every tolerance; a driver without the lent check fails the
  spec ("test.sh: 0 failed").
* `examples/gemm.fib` at 1024, 2048, 4096: the table of section 2 (exit 0 each).
* `scripts/gate.sh --quick`: section 12 of the report.

## 12. Atomics and reductions as built (GPU-3)

Phase 2's items 6 and 7 of section 9 (atomics on the device; block and grid reductions). Verified on this machine's RTX 4080 SUPER (driver 610.57.04) through
libcuda for PTX and through Dawn for WGSL (the adapter is the same GPU: vendor nvidia, architecture lovelace); `compiler/tests/native/gpu-device.sh` is the
executable form and skips the device part, with a note, where there is none. Pinned memory and async transfers (item 8) are NOT done.

| what | where | verdict |
|---|---|---|
| atomics as builtins | `gpu/atomic-{add,min,max,umin,umax,xchg,cas}-i32`, `gpu/atomic-add-i64`, `gpu/atomic-add-f32` (compiler/types/builtins.fib, emit.lower.gpu): one lIR `atomicrmw` or `cmpxchg`, monotonic, answering the OLD value; unsafe like `load-i32`; in the kernel subset (own.kernel); on the host the same instruction | PTX `atom.global.add.u32`, `atom.shared.max.s32`, `atom.global.min.s32`, `atom.global.max.u32`, `atom.relaxed.global.cas.b32`, `atom.global.add.f32`; the address space is inferred from the pointer (a `gpu/shared` word gives `atom.shared`) |
| no new lIR form | `atomicrmw` and `cmpxchg` (lir.md 6.6) lower on the kernel target as they are; 6.9a says so | no spec and code disagreement found |
| the library | `fib.gpu.atomic` (element-indexed, i32 index; `i32-add!` `i32-max!` .. discard the old value, `i32-fetch-add` `i32-cas` .. answer it; `f32-max!` is a compare-and-swap loop, exact) | exported by `fib.gpu` |
| a branch-free select | `gpu/select c a b` (lIR `select`) | the reductions' tool for WebGPU (below) |
| block reductions | `fib.gpu.reduce`: `block-sum-i32 -max-i32 -min-i32 block-sum-f32 -max-f32` over one shared window with `gpu/barrier`; a tree, stride block/2 down to 1, thread t takes thread t+stride; the block is a power of two of at most 256 threads (else a device trap); the answer is for every thread | the tree order is fixed, so the f32 sum is the same on every run and every IEEE device |
| grid reductions | `fib.gpu.reduce-kernels` (`reduce-sum-i32 -max-i32 -min-i32 -max-f32 -partials-f32`, arguments `(data out n blocks)`), `fib.gpu.reduce-atomic-f32` (`reduce-sum-f32`, PTX only) | integer and f32-max results equal the CPU's exactly; `reduce-partials-f32` equals `fib.gpu.reduce-ref`'s `partials-f32` BIT FOR BIT; `reduce-sum-f32` (atomic, order of finishing blocks) is within n * 2^-24 * sum |x| of the left-to-right sum (measured difference 5e-3 at n = 10^6, the bound 3e4) |
| the CPU reference | `fib.gpu.reduce-ref` (deterministic data from a generator a harness in another language repeats, the integer results, the left-to-right f32 sum and its bound, the per-block partials in the device order); `examples/gpu/reduce.fib` prints the reference and runs the kernels over one-thread blocks on the host | cases 8550-8555 (host), 8578-8579 (WGSL) |

**WebGPU.** WGSL has atomics on `atomic<i32>` and `atomic<u32>` only, so a module that contains an atomic prints every storage buffer and workgroup array as
`array<atomic<u32>>` and every plain access as `atomicLoad`/`atomicStore` (webgpu.md 3.7); a module with none prints what it always printed (the golden
kernels.wgsl is unchanged). Signed min and max and `cas` are loops of `atomicCompareExchangeWeak`; an f32 or 64-bit atomic is refused by name. Dawn's
uniformity analysis shaped the reductions: the printer makes any function with a branch a `loop { switch }` state machine, and Tint then takes a barrier in it
as non-uniform when anything after the barrier branches on the thread. So the reductions have no thread-dependent branch in a function that holds a barrier
(masks through `gpu/select`, folds in functions of their own, every thread doing the final atomic with the identity for those that must not add), and the
number of blocks is a kernel argument checked against `gpu/num-groups` (a read of a builtin through a private variable is non-uniform to Tint). A device trap
in the WGSL target returns zeros and goes on (webgpu.md 3.4), so a loop that steps by a trapped call's answer must not: the folds step past n if the total is 0.

**Not done:** pinned host memory and async launch (item 8: `cuMemHostAlloc`, `cuMemcpyHtoDAsync` belong to the driver repository fib-gpu-cuda, which the
core does not contain; the download at 0.4 GB/s is still the cost of not having it); a warp shuffle reduction (an intrinsic family); f32 atomic add and
min/max through WGSL (no form); a relooper in the WGSL printer (webgpu.md 9.3), which would lift the branch rules above.
