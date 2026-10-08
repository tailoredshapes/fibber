# Targets as data: one row per LLVM target

Status: design and first implementation, 2026-10-06; **wasm32-wasip1 runs since WASM-2, 2026-10-07**: it is linked with wasm-ld against wasi-libc and run under node and wasmtime, and
the three case directories run on it (section 5.1 as built, section 7, and docs/design/wasm.md, which is the design of the wasm32 target). Code generation was checked on this x86-64
Linux machine for every other triple below; nothing was linked or run for wasm32-unknown-unknown or riscv64 (riscv64 is parked, ADR 0008). The aarch64 rows are unchanged from
docs/design/aarch64.md and were run on the Mac there.

The owner's goal is to target every LLVM backend. WebAssembly (WASI first, then the browser) and RISC-V come next. This package makes a target one row
of data. It shows what each new row emits today and lists what stands between emitting and running.

## 1. Summary

* `compiler/types/targets.fib` holds the table: one `TargetRow` per target, plus a CPU table. Four readers now get their answers from it:
  `types.target` (lane counts and fma from the CPU table), `llvm.target` (the cross CPU, features, the -O 0 floor, the relocation model and
  the backends it initialises), `native.target` (the checker's Target for `--target`) and `expand.targetos` (the OS backend of the library and the
  runtime). The lowering also gets lIR's `tailcc` and the tail-call kind from the row. `fibc targets` prints the table.
* The host and the x86 and aarch64 rows behave as before. Main's F and this branch's F emit byte-identical lIR, and main's lairf and this
  branch's lairf emit identical assembly, in all 33 comparisons: host, x86_64, three aarch64 triples, three `FIB_TARGET_CPU` values, -O 0 and -O 2
  (`same.sh`, section 7). `a64-emit.sh` passes. In the full gate everything held except one tools check, the LSP fuzz harness's self-test,
  which uses no compiler (section 7).
* `fibc build --target T --emit obj|asm|llvm` works for wasm32-wasip1, wasm32-unknown-unknown and riscv64-unknown-linux-gnu.
  `compiler/tests/native/targets-emit.sh` checks the following for each triple:
  * the accepted lIR cases of simd, instr and mapping lower: 51 of 51 for wasm32, and 49 of 51 for riscv64, where the 2 that do not are expected;
  * 103 fibber programs build: 40 ownership cases, 60 SIMD cases, two tensor cases and a kernel;
  * the object format and the data layout match the row;
  * the pointer width matches the row;
  * node validates the wasm object;
  * the assembly has the expected instructions: `f64x2.mul` and `f32x4.add` for wasm, `return_call` for wasm tail calls, `fmadd.d` and the
    rv64imafdc attribute for riscv64 RV64GC, and `vfmul.vv`, `vfadd.vv` and `vfmacc` for riscv64 with V;
  * nine planted faults are each caught.
* Lowering does not catch the main wasm32 blockers. The objects are valid, but code built from them would be wrong at run time. Section 3 lists
  these blockers, and section 3.1 says what lowering did catch.

## 2. The target record

```
(defstruct TargetRow
  (triple arch os runtime-os data-layout pointer-bits little-endian threads
   vector-bits default-cpu features has-fma min-level tail-cc tail-kind reloc
   unwind object-format linker sysroot libc libc-lacks status support))
```

| triple | ptr | threads | vector, CPU, features | fma | -O0 floor | tailcc / kind | reloc | object, linker | libc (lacks) | runtime-os | status | support (ADR 0008) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| x86_64-unknown-linux-gnu | 64 | pthreads | 256, x86-64-v3 (cross, release) / host | yes | 0 | 18 musttail | PIC | ELF, cc | glibc | Linux | runs | supported |
| x86_64-unknown-linux-musl | 64 | pthreads | 256, x86-64-v3 | yes | 0 | 18 musttail | PIC | ELF, ld (static) | musl (lacks mallopt) | Linux | runs; fully static | supported |
| aarch64-unknown-linux-musl | 64 | pthreads | 128, generic | yes | 1 | 18 musttail | PIC | ELF, ld (static, aarch64) | musl (lacks mallopt) | Linux | emits and runs (qemu-user); fully static | supported |
| aarch64-apple-darwin | 64 | pthreads | 128, apple-m1 | yes | 1 | 18 musttail | PIC | Mach-O, cc (ld64) | libSystem | Darwin | runs (Mac) | supported |
| aarch64-unknown-linux-gnu | 64 | pthreads | 128, generic | yes | 1 | 18 musttail | PIC | ELF, cc | glibc | Linux | emits | supported |
| arm64-apple-ios | 64 | pthreads | 128, apple-m1 | yes | 1 | 18 musttail | PIC | Mach-O, xcrun clang, static library | libSystem (pipe) | Darwin | emits | supported |
| wasm32-wasip1 | 32 | none | 128, generic, `+simd128,+tail-call` | no | 1 | 0 (C) tail | static | wasm, wasm-ld, `$WASI_SYSROOT` | wasi-libc (madvise mallopt pthread_exit) | Wasi | runs (node, wasmtime) | portability target |
| wasm32-unknown-unknown | 32 | none | 128, generic, `+simd128,+tail-call` | no | 0 | 0 (C) tail | static | wasm, `wasm-ld --no-entry --export-dynamic` | none (all 50) | Linux (stand-in) | emits | portability target |
| riscv64-unknown-linux-gnu | 64 | pthreads | 128, generic-rv64, `+m,+a,+f,+d,+c,+zicsr,+zifencei` | yes (fmadd.d) | 0 | 8 (fastcc) tail | PIC | ELF lp64d, cc (riscv64 cross), `$RISCV_SYSROOT` | glibc | Linux | emits | parked: LLVM has no tailcc for RISC-V |
| nvptx64-nvidia-cuda | 64 | none | 128, sm_89 | yes | 1 | 0 (C) tail | static | ptx, none (a driver loads it) | none (all 50) | Linux (stand-in; no runtime is emitted) | emits PTX, run by fib-gpu-cuda (docs/design/gpu.md) | kernel target |
| wgsl-unknown-webgpu | 64 | none | 128, (none: no LLVM machine) | yes | 1 | 0 (C) tail | static | wgsl, none (a driver compiles it) | none (all 50) | Linux (stand-in; no runtime is emitted) | emits WGSL (the printer native.wgsl, not LLVM), run by fib-gpu-webgpu, the JS glue and Dawn (docs/design/webgpu.md) | kernel target |

Data layouts (LLVM 21's, from the machine; checked by `targets-emit.sh` and `a64-emit.sh`):

* x86_64: `e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128`
* aarch64 ELF: `e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32`
* aarch64 Mach-O: `e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32`
* wasm32: `e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-i128:128-n32:64-S128-ni:1:10:20`
* riscv64: `e-m:e-p:64:64-i64:64-i128:128-n32:64-S128`

How the readers use a row:

* **`target-row-of-triple`.** It matches on architecture (`arm64` reads as `aarch64`) and OS:
  * `-ios` is ios;
  * `-apple-`, `-darwin` and `-macos` are darwin;
  * a triple containing `wasi` is wasi;
  * any other `wasm*` triple is none;
  * `-linux` is linux;
  * anything else is other.

  A triple whose architecture has a row but whose OS does not gets that architecture's row. Its runtime OS is the triple's (Apple is Darwin,
  anything else Linux) and its status is `unlisted`. An unknown architecture gets a generic row: Linux, 128 bits, no fma. Both fallbacks are what
  the compiler did before the table (`compiler/tests/types/unit-targets.fib`).
* **`target-of-row`.** This is the checker's Target for a cross build. If `FIB_TARGET_CPU` names a CPU that the CPU table knows, that CPU's
  width and fma apply, with no features. Otherwise the row's width and fma apply, and the row's features too when no CPU is named. The module's
  `(target (cpu ..) (features ..))` form and the cross machine get the same CPU and features (`llvm.target` `cross-features`).
* **`min-level`.** The AArch64 -O 0 FastISel miscompile workaround (aarch64.md 2.3) is now the row value 1. `machine-level` is
  `max(opt, min-level)`.
* **`tail-cc` and `tail-kind`.** lIR's `tailcc` maps to the following:
  * on x86 and aarch64: LLVM's tailcc (18) with `musttail`;
  * on WebAssembly: the C convention (0) with a `tail` hint, because the backend rejects every other convention ("WebAssembly doesn't support
    non-C calling conventions");
  * on RISC-V: fastcc (8) with a `tail` hint, because the backend has no tailcc ("LLVM ERROR: Unsupported calling convention", an abort).

  LLVM's verifier allows `musttail` between different prototypes only under tailcc, so both new rows must use the hint (section 3.2).
* **`reloc`.** The relocation model is PIC, except on WebAssembly. A PIC wasm object imports `__memory_base`, which is the dynamic-linking
  convention and not what wasm-ld builds an executable from.
* **`libc-lacks`** lists the functions of `runtime-c-functions` that the target's C library does not provide. `runtime-c-functions` is exactly
  the 50 `declare`s of `rt/*.lir`; `targets-emit.sh` checks this and plants a missing one. The wasm32-wasip1 list comes from knowledge of
  wasi-libc and has not been checked against a sysroot (section 5.1, step W1).

* **`support`** (docs/adr/0008, the owner's rule of 2026-10-06): `supported` only where LLVM guarantees a tail call between functions of any
  signature (tailcc 18 with `musttail`) and the instruction set has FMA; `row-supported?` checks the facts as well as the mark, so a row
  marked supported that breaks the rule prints `unsupported: marked supported but breaks the rule`. riscv64 is `parked` (no tailcc in
  LLVM 21): `fibc build --target riscv64-unknown-linux-gnu` refuses it unless `--allow-unsupported` (or `FIB_ALLOW_UNSUPPORTED=1`) is
  given, which `targets-emit.sh` does to keep its emit check. The wasm rows are `portability target`s: built without the flag, `simd/fma`
  a compile-time warning and a run-time trap there, `simd/muladd` a multiply and an add. x86-64 is supported at x86-64-v3: the row's CPU
  (cross builds, releases) is `x86-64-v3`; a host build keeps the host's CPU, and every x86-64 executable checks at start that the CPU
  has what it was built for (`emit.cpucheck`).

* **The start-up CPU check** (`compiler/emit/cpucheck.fib`, ADR 0008). An executable for x86-64 Linux calls `fib.cpu-check` before anything
  else in `main`. It asks glibc (`__x86_get_cpuid_feature_leaf`, glibc 2.33 and later; the `active` bits fold in the OS's register-state support) for each
  feature the binary was built for: the level of a named CPU (x86-64-v2, v3, v4 and the CPU table's names) or the host's `+` features, among
  SSE3 to SSE4.2, POPCNT, CX16, AVX, AVX2, BMI1, BMI2, F16C, FMA, LZCNT, MOVBE and the five AVX-512 features of v4. A missing one ends the
  program with `trap: this program needs x86-64-v3 (AVX2, FMA); this CPU lacks: FMA` (status 134) instead of SIGILL. It costs one call at start.
  The check is scalar code and `write` calls, so it runs on the CPU it refuses (`compiler/tests/driver/cpu-check.sh` runs a v3 binary under
  `qemu-x86_64 -cpu Westmere`). There is no check for baseline x86-64, for an unknown CPU name, for a non-Linux OS, or for aarch64, whose baseline
  has FMA. An aarch64 build for a CPU with more features (`FIB_TARGET_CPU=apple-m2`) is not checked; that is a known gap.

**Adding a row.** For an architecture that already has one, add a `row-...` function and put it in `target-rows`. For a new architecture,
also add the five `LLVMInitialize<Arch>{TargetInfo,Target,TargetMC,AsmPrinter,AsmParser}` externs and calls to `llvm.target`, and add the
component to `scripts/llvm-static.sh`. LLVM-C's `LLVMInitializeAllTargets` is a `static inline` and has no symbol to bind.

**The rest of LLVM 21's backends.** `llc-21 --version` lists AArch64, AMDGPU, ARM, AVR, BPF, Hexagon, Lanai, LoongArch, Mips, MSP430,
NVPTX, PowerPC, RISC-V, Sparc, SystemZ, VE, WebAssembly, X86 and XCore. None of these has a row beyond the seven above. Section 5.5 groups them by
effort.

## 3. Census

A read of rt/, compiler/emit, compiler/native, compiler/llvm, compiler/expand, lib/ and scripts/ classified each finding as one of four kinds:

* **A**: fine everywhere;
* **B**: needs a target-row value;
* **C**: needs a runtime alternative;
* **D**: blocks a target.

A row that groups several sites (for example a run of `i64` length fields in one file) counts once:

| where | A | B | C | D |
|---|---|---|---|---|
| rt/*.lir | 34 | 47 | 13 | |
| compiler/emit | 17 | 27 | 1 | |
| compiler/native, compiler/llvm | 9 | 5 | | 1 (target init, fixed here) |
| compiler/expand | | 1 | 1 | |
| lib/, scripts/ | 6 | 23 | 13 | |
| **total** | **~66** | **~103** | **~28** | **1 (fixed)** |

The **B** rows are "blockers for running" on wasm32 until their value comes from the row. On riscv64 almost every B already has the right
(Linux LP64) value.

**Pointers.** Objects store pointers as lIR `ptr` and never as `i64`. Struct GEPs follow the data layout, so field access is right on wasm32. The
fixed header parts do not depend on pointer width: `fib.hdr` is `{i64 i32 i32}` (16 bytes), and str and array have an `i64` length, so data
starts at 24 on every target.

### Blockers for wasm32 (both rows), largest first

1. **Pointer size computed by hand.** `compiler/emit/layout.fib:111-112` `size-align` says `ptr` and `raw` are 8 bytes and `dyn` is 16. That
   size becomes the runtime's element size (`esize`), and the runtime indexes arrays and vectors by `i*esize`. The sites are:
   * `rt/array.lir:38-40,54-57`;
   * `rt/vec.lir:42,100,123,133` (`(i64 8)` literal);
   * `rt/vecbuild.lir:56,78,86`;
   * `rt/core.lir:359,362` (worklist `cap*8`);
   * `compiler/emit/lower/builtins.fib:165,338,429,431`;
   * `compiler/emit/lower/pattern.fib:183,264`;
   * `compiler/emit/lower/quote.fib:112,117`;
   * `compiler/emit/objects.fib:56-98,138,154`, where obj-size and obj-offsets are summed from `size-align`.

   The compiler's own element access uses a GEP on `[0 x ptr]`, whose stride is 4 on wasm32. **An Array or Vec of objects or `dyn`s is
   therefore read at two different strides.** The vector trie's child arrays are among them. Most fibber programs touch one.
2. **`size_t` and `long` declared as `i64`.** These include:
   * `malloc`, `calloc`, `memcpy`, `memcmp`, `write`, `strlen` and `snprintf` in `rt/core.lir:7-17`;
   * `fread`, `fwrite` and `realloc` in `rt/io.lir:6-9`;
   * `read` in `rt/sys.lir:9`, `memset` in `rt/str.lir:100`, `sysconf` in `rt/task.lir:11` and `pthread_detach` in `rt/thread.lir:84`;
   * about 20 `extern`s of `lib/fib/os/**`;
   * `prelude.fib:687`.

   On wasm32 these are `i32`. In the objects that `targets-emit.sh` builds, `env.malloc` is imported with an `i64` parameter. wasm-ld treats
   such a signature mismatch by replacing the import with a trapping stub. This was not seen here: no wasm-ld.
3. **Threads.** The following assume threads:
   * `pthread_create`, `pthread_detach` and `pthread_exit` (`rt/thread.lir:83-132`, `rt/core.lir:44,570`);
   * the worker pool (`rt/task.lir:155-163`);
   * the task key (`rt/core.lir:40-50,580-583`);
   * `spawn`, `pmap` and `async` (`lib/prelude.fib:665-676`).

   wasm32-wasip1 has no threads. The kernel's object imports `pthread_create` and `pthread_exit`, which are in the row's `libc-lacks`.
4. **libc constants and gaps.** These are Linux values or calls that WASI lacks:
   * `madvise(..14)` (Linux `MADV_HUGEPAGE`) and 4 KiB page rounding (`rt/core.lir:125-129`);
   * `pipe` and `dup` (`rt/sys.lir:11-12,92-103`);
   * `clock_gettime` with an integer clock id (`rt/sys.lir:141-151`): in wasi-libc, `clockid_t` is a pointer to a struct;
   * a `timespec` read as `i64` at +8 (`rt/sys.lir:143-160`): `long` is 4 bytes on wasm32;
   * Linux errno numbers `-22` and `-12` (`rt/sys.lir:37..135`, `lib/fib/os/errors.fib:10-12`);
   * `AT_FDCWD -100` (`rt/sys.lir:7,39`): -2 in wasi-libc;
   * `__errno_location` (`rt/sys.lir:20`).
5. **Library OS backend.** `expand/roots.fib:138-141` has backends for Linux and Darwin only. The wasm rows use Linux as a stand-in, which is
   their `runtime-os` and is wrong for wasi-libc (flags, errno and dirent offsets). `lib/fib/os` also uses `/proc/self/exe` and
   `/proc/self/status`, `fork`, `waitpid`, sockets, `poll` and signals (`process.fib`, `net/*.fib`), none of which WASI preview 1 has.
6. **Entry point.** `compiler/emit/compile.fib:23-25` writes a C `main(argc, argv)` that calls `printf`. That suits wasip1 (wasi-libc's
   `_start` calls `main`). wasm32-unknown-unknown has no `main` or argv, so it needs exported entry points.
7. **No libc (wasm32-unknown-unknown).** All 50 runtime C functions are imports that the JavaScript host would have to provide. Float printing
   (`%.800e` and its `strtod` read-back, `rt/str.lir:288`) needs its own code.

### Blockers for riscv64-linux-gnu

riscv64-linux-gnu is LP64 glibc on Linux with pthreads and little-endian byte order. Every Linux constant above (`sysconf` 84, errno, dirent
offsets, `timespec`, 8-byte pointers, `AT_FDCWD -100`) is right for it. Two things remain:

* the tail-call guarantee (section 3.2);
* linking. `native/link.fib:77,163` runs `cc` with no `--target` or `--sysroot`, so linking another target is refused. The plan is to link with
  a riscv64 cross `cc` that is given the sysroot.

### Found on the way, for the existing Darwin row

`rt/core.lir:40` declares `fib.task-key` as an `i32` global, and `pthread_key_create` writes a `pthread_key_t` into it. On Darwin that type
is `unsigned long`, 8 bytes, so the call writes 4 bytes past the global. On little-endian hardware the high half is zero for a small key, so the
overwrite writes zeros into the neighbouring global. This is harmless only while that neighbour is zero or unused. It is not fixed here: it changes
code that only the Mac can test. The fix is a row value, an `i64` key on Darwin through `emit/os.fib` (plan, D-1).

### 3.1 The census as lowering results

`targets-emit.sh` (full run: 51 lIR cases and 103 fibber programs per triple) found nothing in the language or the library that fails to
lower on wasm32 or riscv64 once the calling convention, the tail-call kind and the relocation model come from the row. Two lIR cases fail on
riscv64 only, and are listed in `targets-emit.expected`. `cases/lir/simd/target.lir` and `target-cpu-only.lir` name an x86 CPU in their
`(target (cpu ..))` form. The RISC-V backend aborts the process ("LLVM ERROR: RV64 target requires an RV64 CPU"), where wasm and aarch64 only
warn and ignore the CPU. lairf should refuse a module whose CPU belongs to another architecture with a diagnostic (plan R3). Before those three row values, lowering failed as follows:

* **wasm32.** Every fibber program was refused: `in function l.f.fib.seq.consumers.first..: WebAssembly doesn't support non-C calling
  conventions`.
* **riscv64.** Code generation aborted with `LLVM ERROR: Unsupported calling convention`, exit 134.
* **Both, with C or fastcc and `musttail`.** The verifier rejected 8 of the 10 quick programs: `cannot guarantee tail call due to mismatched
  parameter counts` (or `types`).

The 64-bit assumptions in the list above make no lowering error, because LLVM accepts an `i64` `malloc` argument and an `(i64 8)` element
size. They are run-time faults and need run-time tests (section 5).

**Small, safe fixes made here.** These are row values instead of constants:

* the calling convention for lIR `tailcc`;
* the tail-call kind;
* the relocation model;
* the cross machine's features;
* the -O 0 floor;
* the OS backend choice.

The pointer-size fix touches every `size-align` caller. It also has to stay on the host's layout for `emit/defs/jit.fib`, which reads JIT
memory. It is not small: plan W2.

### 3.2 Tail calls are best-effort on WebAssembly and RISC-V

**WASM-2 settled this for wasm** (docs/design/wasm.md 6): with `+tail-call` and the code generated at level 1 or more (the row's `min-level` is now 1: at level 0 LLVM's FastISel makes no `return_call`),
ten million tail calls between functions of different signatures run in node's default stack (case 8250), and overflow without the feature (`FIB_TARGET_FEATURES=-tail-call`, the negative control of
`compiler/tests/wasm/wasm.sh`). The text below is what was known before that run; RISC-V is as it was.

spec/lir.md 7.3 promises that a tail call is a `musttail`. On the two new rows that promise does not hold today:

* WebAssembly makes `return_call` where LLVM can. `02-structural-sharing` gets 6, checked by `targets-emit.sh`; with the row's
  `+tail-call` turned off, it gets 0, the planted fault.
* RISC-V makes a sibling call where the arguments fit.

Anything else becomes an ordinary call, so deep mutual tail recursion could overflow the stack. This is a spec-versus-target question for
the owner. The options are:

* keep the hint and document the rows as weaker;
* make every lIR `tailcc` function take one shared prototype, for example an argument block, so that `musttail` with C or fastcc verifies;
* use the WebAssembly backend's own `musttail` lowering once the prototypes agree.

The x86 and aarch64 rows still use `musttail`.

## 4. What emits

Results of `targets-emit.sh`, section 7:

| | lIR cases (simd, instr, mapping) | fibber programs | object | data layout, pointer width | instructions |
|---|---|---|---|---|---|
| wasm32-wasip1 | 51 of 51 | 103 of 103 | wasm, `\0asm`, node validates | row's, 32 | f64x2.mul/add, f32x4.mul/add; one f64x2.mul per `<2 x double>`; return_call |
| wasm32-unknown-unknown | 51 of 51 | 103 of 103 | wasm, node validates | row's, 32 | (as wasip1) |
| riscv64-unknown-linux-gnu | 49 of 51 (2 expected: x86 CPU in the module, 3.1) | 103 of 103 | ELF 64-bit RISC-V, RVC, double-float ABI | row's, 64 | fmadd.d, rv64imafdc attribute; with `FIB_TARGET_CPU=spacemit-x60`: vfmul.vv, vfadd.vv, vfmacc |

What it means that these emit: a valid object exists for the target, and that object would have the run-time faults of section 3 once it was
linked and run.

## 5. Plan

Sizes: S is under a day, M one to three days, L about a week.

### 5.1 wasm32-wasip1, single-threaded first

**As built (WASM-2).** W1 to W7 are done, with these decisions (docs/design/wasm.md has the detail and the numbers):

| package | built as |
|---|---|
| **W1** | `scripts/fetch-wasm-tools.sh` (wasi-sdk 34 and wasmtime 49, sha256 pinned, ADR 0020); `fibc build --target wasm32-wasi` links with `wasm-ld`, wasi-libc and compiler-rt's builtins (`native/linkwasm.fib`: `WASI_SDK`, or `WASM_LD`, `WASI_SYSROOT`, `WASI_BUILTINS`); runs under node:wasi (`compiler/tests/wasm/run.mjs`) and wasmtime. `libc-lacks` checked against `libc.a`: it is `madvise mallopt pthread_exit` (the guess of the table was wrong: wasi-libc has stubs of `pipe`, `dup`, `pthread_create`, `pthread_detach`) |
| **W2** | `size-align-pb`; the pointer width is `Program.pb` (the target's, 8 while the JIT makes constant `def`s and in macro modules); the constant `def`s of a 32-bit target are made on the host and the rest lowered afresh (`make-defs-at`, `program-rebuilt`); the runtime's pointer-sized literals are `emit.wasi/rewrites` |
| **W3** | not as planned: the runtime keeps `i64`; the lowering declares the libc functions of `libc-narrow-names` with `i32` and narrows at each call (saturating), so the library's `extern`s need no change |
| **W4** | `emit/wasi.fib` (the runtime) and `lib/platform/wasi/fib/os/backend.fib`; `AT_FDCWD` -2, WASI open flags, the clock, errno numbers mapped, no `pipe`/`dup` (ENOSYS) |
| **W5** | decided: tasks run on the thread that joins them, `spawn` runs the task at the spawn (inline), the pool has no worker; a wait that needs another thread is the trap `deadlock` (wasm.md 3) |
| **W6** | `compiler/tests/wasm/suite.py` runs a case directory as wasm modules against `compiler/tests/wasm/expected.txt`; the gate has a wasm stage (skipped, with the reason, without node or the toolchain) |
| **W7** | decided in 3.2 above |

The plan as it was written:

| package | what | size |
|---|---|---|
| **W1** | Tools (5.6). Link the hello world with wasm-ld and wasi-libc, run it under wasmtime and under node's `node:wasi`, then check `libc-lacks` against wasi-libc's `libc.a` (`llvm-nm`) | S |
| **W2** | Pointer size from the row. `size-align` takes the pointer width (`LirPtr`/`LirRaw` = pointer-bits/8, `LirDyn` = 2x). Every caller in compiler/emit gets the module's row through `Globals` (where `Target` already lives). `defs/jit.fib` keeps the host row. rt's `(i64 8)` literals become a `fib.ptr-size` constant that emit.rt substitutes per row. Check: `layout-dump` for wasm32 and a case of a Vec of objects run under wasmtime | M |
| **W3** | `size_t` per row. The runtime's `declare`s use a `size` type that emit.rt substitutes per row (`i64` or `i32`), with `trunc` and `zext` at the call sites in rt. The library's `extern`s get the same through a `usize` alias in `fib.os.abi` | M |
| **W4** | A WASI runtime adaptation in `emit/os.fib` and a `lib/platform/wasi` backend: `AT_FDCWD -2`, the clock through `clock_time_get` or the `_CLOCK_MONOTONIC` address, WASI errno numbers, `madvise` dropped, no `pipe` or `dup` (refused with an error value), executable-path refused | M |
| **W5** | Single-threaded runtime: `threads = none` makes `spawn` run inline at `deref` (or be refused with a type error the checker gives from the row; the owner chooses), the pool is one worker, `pmap` is `map`, and a task trap is not isolated (exceptions.md mechanism (b), the hidden result, needs no `pthread_exit` and no unwind tables) | M |
| **W6** | `ownership` cases differentially: the same stdout, stderr and status under wasmtime and the x86 build. Then stdlib. Join the gate as a run row (5.7) | M |
| **W7** | Tail calls: decide 3.2 | S-M |

### 5.2 wasm threads (wasm32-wasip1-threads)

A row with `threads = wasm-threads`, `+atomics,+bulk-memory` and shared memory, plus wasi-libc's threads build and `wasi_thread_spawn`.
pthreads come from wasi-libc. `pthread_exit` works there, but mechanism (b) is preferable. Runs under wasmtime with `-S threads`. The browser needs
cross-origin isolation for shared memory. Size: M, after W1-W6.

### 5.3 Browser and JavaScript host (wasm32-unknown-unknown)

* A `jsbind` layer, the counterpart of `wasm-bindgen`: `(export-js name)` on a defun emits an exported wrapper. Strings cross as a pointer
  and length into linear memory, and a generated JS glue module encodes and decodes UTF-8.
* Host imports for `write` (console), `getenv` (none), time (`performance.now`) and `abort`.
* An allocator in fibber or lIR over `memory.grow` instead of libc `malloc`.
* Float formatting without `snprintf`.
* A `--no-entry` build with exported entry points.

Size: L. The libc-free runtime is most of it.

### 5.4 riscv64-linux-gnu

| package | what | size |
|---|---|---|
| **R1** | Link with a riscv64 cross gcc or clang and a sysroot (`native/link.fib` grows `--target` and `--sysroot` from the row's `linker` and `sysroot`). Run the hello world under `qemu-riscv64 -L $RISCV_SYSROOT` | S |
| **R2** | Ownership and stdlib cases differentially under qemu-user, with and without V (`FIB_TARGET_CPU=spacemit-x60`, `qemu-riscv64 -cpu rv64,v=true,vlen=256`). Tail calls (3.2) | M |
| **R3** | CPU table rows for RISC-V (sifive-*, spacemit-*, the rva23 profiles: 128-bit preference, fma) and a VLEN-aware `vector-bits`. lairf refuses a module whose `(target (cpu ..))` belongs to another architecture, instead of the RISC-V backend's abort | S |
| **R4** | A riscv64 seed (the cross route of aarch64.md) and CI under qemu | M |

### 5.5 The rest of LLVM's backends, by effort

* **Little-endian 64-bit Linux with glibc** (LoongArch64, ppc64le, aarch64_be excluded): a row, the initialisers, qemu-user. Each is S-M,
  and like riscv64 the Linux constants hold. Check tailcc support per backend: LoongArch and PowerPC lack it.
* **Big-endian** (s390x, ppc64, mips64, sparc64): the same plus the little-endian assumptions of `lib/fib/os/native.fib:6-9`, the `timespec`
  stores and the pipe fd packing. M each.
* **32-bit hosted** (armv7 linux-gnueabihf, i686, riscv32): W2 and W3 (pointer and `size_t` width) first, then as above. M.
* **Embedded and no OS** (AVR, MSP430, ARM Cortex-M, RISC-V bare metal, Xtensa, which is not in LLVM 21 here): the libc-free runtime of 5.3,
  no threads, a 16-bit `int` on AVR and MSP430. L or more. AVR's 16-bit pointers break the 24-byte header.
* **Accelerators and VMs** (NVPTX, AMDGPU, BPF, SPIR-V, Lanai, VE, Hexagon): no general host runtime. A kernel-only subset of the language is
  docs/design/gpu.md (GPU-1, 2026-10-07): NVPTX is a **kernel target** row that emits the PTX of a program's kernels; SPIR-V and AMDGPU are the same class.

### 5.6 What running needs installed

None of this is on this machine except qemu and node.

* **wasm32-wasip1:**
  * `wasm-ld` from LLVM 21 (Debian/Ubuntu `lld-21`, or in wasi-sdk);
  * wasi-libc built for wasm32-wasip1. wasi-sdk 25 or later brings clang, wasm-ld, the sysroot and `libclang_rt.builtins-wasm32.a`. Point
    `WASI_SYSROOT` at `wasi-sdk/share/wasi-sysroot`;
  * a runtime: wasmtime (any 2024 or later release; `-W tail-call=y` is the default since v22), or node.

  node v26.10.0 is installed here and has `node:wasi`, an experimental feature that prints a warning. It runs a linked wasip1 module with
  `new WASI({version: 'preview1', args, env})`. It was not tried, because no module could be linked. V8 validates the objects that
  `targets-emit.sh` builds.
* **wasm32-unknown-unknown:** wasm-ld; node or a browser for the JS host.
* **riscv64-linux-gnu:**
  * `qemu-riscv64` user mode. Debian's qemu-user 10.2.1 is installed here (`/usr/bin/qemu-riscv64`).
  * a riscv64 sysroot: Debian or Ubuntu `libc6-dev-riscv64-cross`, `libgcc-14-dev-riscv64-cross` and `linux-libc-dev-riscv64-cross`, or a
    `riscv64-linux-gnu` cross toolchain;
  * a linker: `gcc-riscv64-linux-gnu`, or clang 21 with `--target=riscv64-linux-gnu --sysroot` and `lld-21`.

### 5.7 The gate

* `targets-emit.sh --quick` (12 lIR cases and 10 programs per triple, three triples) should join the tools stage beside `a64-emit.sh
  --quick`. It needs only F and lairf, and no network or target tools. It takes `F` and `L` like `a64-emit.sh`.
* The full run has 154 builds per triple and took 165 s here. It belongs in the full gate.
* When W1 and R1 land, each run row adds a `--run` mode that skips with a note (status 0, a `skip` line) when the tools are absent. A machine
  that has the tools then runs the ownership cases differentially.

This package does not edit scripts/gate.sh, scripts/tools.sh or compiler/tests/units.sh. `compiler/tests/types/unit-targets.fib` is listed in
`compiler/tests/units.run`, so units.sh runs it.

## 6. Tools fetched, and not fetched

WASM-2 (2026-10-07) had network access: `scripts/fetch-wasm-tools.sh` fetches wasi-sdk 34.0 (x86-64 Linux: clang, `wasm-ld`, the wasi-libc sysroot, compiler-rt's builtins) and wasmtime 49.0.2 into
`~/.cache/fibber-scratch/tools/wasm`, each checked against the sha256 that GitHub publishes for the release asset and that the script holds:

* `wasi-sdk-34.0-x86_64-linux.tar.gz` b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4
* `wasmtime-v49.0.2-x86_64-linux.tar.xz` a4d6e9e3a5a60f527cf7793d674c48930c80c2e8977995b8a275cad3254b9322

Nothing is installed system-wide. node's `node:wasi` is the first runtime and was already here. A static qemu-riscv64 and a riscv64 sysroot were not fetched (riscv64 stays parked, ADR 0008);
`wasm-opt` (binaryen) was not fetched and is not used.

## 7. Checks run

* **WASM-2 (2026-10-07), wasm32-wasip1 run, not only emitted.** Linked with wasm-ld 23 (wasi-sdk 34) against wasi-libc, run under node v26.10.0 and wasmtime 49.0.2 (docs/design/wasm.md 9):
  * `compiler/tests/wasm/suite.py`: cases/ownership 351 of 354 pass, cases/modules 32 of 32, cases/stdlib 1274 of 1337 (16 more are `open:` cases the header excuses); the 3 + 47 that do not
    pass are in `compiler/tests/wasm/expected.txt` with their reasons (no second thread, no sockets, no FMA, libm differences, the unused `calloc`); the SIMD, tensor and movemask cases: 182 of 191 with simd128
    and 182 with `FIB_TARGET_FEATURES=-simd128`, the nine that do not are FMA or libm;
  * `compiler/tests/wasm/wasm.sh`: 12 of 12 (hello, memory.grow, oversize request, a 200 KB write, ten million tail calls and the negative control, movemask, vectors of objects, simd128 on and off, export), 6 s;
  * `scripts/mutant-wasm.sh`: six planted faults (memory limit, `write` length, tail-call feature, lane order of a wide mask, pointer width, `size_t` saturation), each killed;
  * `compiler/tests/emit/unit-wasi.fib` (every rewrite of the runtime is in rt/*.lir) and `compiler/tests/types/unit-targets.fib`; cases 8250-8252 pass natively and on wasm;
  * the nine quick benchmarks print the same bytes on wasm and native, 1.2 to 3.6 times slower on node (wasm.md 8); a hello world is 43 KB.

* `compiler/tests/native/targets-emit.sh` (full and `--quick`), `compiler/tests/types/unit-targets.fib` (17 checks) and
  `compiler/tests/driver/unit-args.fib` (22 checks). The output is in the commit messages and the report.
* `same.sh` (scratch): main's F and lairf against this branch's, 0 of 33 comparisons differ.
* `compiler/tests/native/a64-emit.sh` (full, with this branch's F and lairf): every check holds and every planted fault is caught. It lowered
  51 lIR cases and 101 programs for each of the three aarch64 triples, in 107 s.
* The full gate: `GATE_FRESH=1 FIBC=<seed v0.1.7> scripts/gate.sh --full -j 6`, 2476 s, mostly spent waiting for the shared lock.
  * The stage 2 build took 66 s, and the fixed point holds: F and F3 emit the same lIR.
  * cases/ownership: 354 of 354 pass. cases/modules: 27 of 27 pass. cases/stdlib: 1281 pass, 20 open and 1 fail
    (`1707-a-def-atom-..`, the one entry in `ci-stage2.expected`, so `ci-stage2: ok`).
  * Tools: every unit part passed (units-emit, -own, -types, -rest, -pending). The gate failed on one check: `lsp-hardening`'s last step,
    `node compiler/tests/lsp/fuzz.js --selftest`, printed "selftest crash: hang (no answer in 1500 ms)" and "selftest: FAILED". That self-test
    runs the fuzzer against a fake server written in JavaScript and starts no fibc. Run alone on this branch, it fails the same way (node
    v26.10.0, load average about 7), and this branch changes nothing under `compiler/tests/lsp`. It is not caused by this package, but the gate
    did fail.
