# fibber on aarch64 (Apple Silicon macOS, iOS, Linux): package A64-0

Status: design and spike, 2026-10-05. Everything marked **ran on the Mac** was run on an Apple M1 Ultra (macOS 27.0.1, arm64, Apple clang 21,
Homebrew `llvm@21` 21.1.8) over ssh, under `~/fibber-a64-scratch`. Everything marked **emitted only** was produced on x86-64 Linux and
inspected, not run on aarch64. Nothing here ran on iOS, on aarch64 Linux or on an M4 (SME).

## 1. Summary

fibber's code generator is LLVM 21 through LLVM-C, so aarch64 is mostly a matter of telling it the target. The spike adds that
(`--target TRIPLE`, `FIB_TARGET_TRIPLE`) and shows, on real hardware:

* a cross-compiled hello world (x86-64 Linux, `fibc emit` + `lairf build --emit obj`, linked by the Mac's `cc`) runs natively;
* 272 of 272 accept/trap programs of `cases/ownership` run natively and agree with their headers, except two that are not aarch64 findings (7.2);
* the compiler itself, cross-compiled, links on the Mac against Homebrew's `libLLVM-21.dylib` and **runs there**: `fibc cases cases/ownership`
  passes 310 of 311 natively (the one failure reads `spec/method.md`, which was not copied), `cases/stdlib` passes 1196 of 1225 (21 are
  labelled open on x86 too; 7.3 lists the rest);
* the compiler builds itself on the Mac: the native `fibc` builds `F2`, `F2` builds `F3`, and `fibc`, `F2` and `F3` emit byte-identical lIR
  for `compiler/fibc.fib` (a 24,299,790-byte module): the self-host fixed point holds on aarch64 macOS;
* the lIR that the x86 box emits with `--target arm64-apple-macosx13.0.0` for `compiler/fibc.fib` is byte-identical to what the native Mac `fibc` emits.

The ORC JIT works on macOS arm64 with no entitlement and no signing beyond the linker's ad-hoc signature (the macro runner and `fibc run` use it).
Open items are in 7 and the package plan in 8.

## 2. Census of x86 and Linux assumptions

References are to the tree before this package (commit 771b782); `*` marks a line this package changed.

### 2.1 Target initialisation, triple, data layout

| What | Where | Today | aarch64 |
|---|---|---|---|
| Target initialisers bound | `compiler/llvm/target.fib:12-17` | only `LLVMInitializeX86{TargetInfo,Target,TargetMC,AsmPrinter,AsmParser}`; LLVM-C's `LLVMInitializeNativeTarget` is a `static inline` and has no symbol | * adds the five `LLVMInitializeAArch64*` (they exist in `libLLVM-21.so` and the dylib). Both backends must be in the linked LLVM: Homebrew's and Debian's are; a one-target static LLVM is not (6.2) |
| Where they are called | `compiler/llvm/target.fib:45-50` (`init`), called by `machine-host`, `target-info`, `host-feature-set` | x86 only | * both |
| Triple | `target.fib:33,110` `LLVMGetDefaultTargetTriple` | the host's | * `cross-machine` takes `FIB_TARGET_TRIPLE`; the host default is unchanged |
| Data layout | `target.fib` `layout-of` from the machine; `lower.fib:10` `lower m name triple data-layout` | derived from the machine, so correct for whatever triple the machine has | nothing to change: Mach-O `e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32`, ELF `e-m:e-...-i8:8:32-i16:16:32-...` (checked by `a64-emit.sh`) |
| Machine users | `native/aot.fib:55`, `native/pipeline.fib:26`, `native/jit.fib:40` | all `machine-host` | * AOT and `emit-llvm` use `machine-target` (the cross machine when the triple is set); the JIT stays on the host, because it runs what it compiles |
| CPU of the machine | `target.fib:105-110` | host CPU and features, or `FIB_TARGET_CPU` | * a cross machine uses `FIB_TARGET_CPU` if set, else `apple-m1` for Apple triples and `generic` otherwise |

Per-function `"target-cpu"`/`"target-features"` attributes come from the module's `(target ..)` form (`native/lower.fib:96,109`); the
front end writes the form from the `Target` it was given (`emit/compile.fib:70-75`), so the form and the checker's lane counts agree.

### 2.2 The `Target` record, lanes, fma

`compiler/types/target.fib`: `cpu-avx2?` (lines 20-25) lists x86 CPU names; `target-of-cpu` (28) gives 256 bits and `has-fma` only for those and 128/false
for every other name; `target-of-features` (34) reads `+avx2` and `+fma` from LLVM's host feature list; `target-baseline` is `x86-64`, `target-default`
`x86-64-v3` (40, 44). On an aarch64 host `target-of-features` looks for `+avx2` and `+fma` in LLVM's host feature list; AArch64 FMA is base floating point and (to be confirmed on the Mac) not a listed `+fma` feature, so it would report **128 bits and no fma**, wrong for `has-fma`.
* The spike does not touch this file (SC1 owns it). `native/target.fib` `cross-target-info` returns `(Target cpu "" 128 true)` for an aarch64 triple.
* A native Mac build (no triple) still goes through `target-of-features`, so SC1's `has-fma` must be true on aarch64: see A64-2. (The native Mac `fibc` emitted the same lIR as the cross
  one, including the `(target ..)` line, so today both agree on `cpu`; `has-fma` is not consumed by the library yet: `grep has-fma lib` finds nothing.)
* Vector width: Apple cores have four 128-bit NEON pipes; 128 bits is right. SVE does not exist on M1-M4 (M4 has SME streaming mode only); do not add it.
  Linux server cores (Neoverse V1/V2, Graviton 3/4) have SVE: not assumed.

### 2.3 JIT (ORC)

`compiler/llvm/orc.fib:13,46-62`: `LLVMOrcCreateLLJIT` with a `JITTargetMachineBuilder` made from the host machine, and
`LLVMOrcCreateDynamicLibrarySearchGeneratorForProcess` for libc symbols. LLVM 21's LLJIT picks JITLink for arm64 Mach-O and its default in-process memory manager.
Measured, **ran on the Mac**: the cross-built `fibc` (signature: ad-hoc, linker-signed, no hardened runtime, no entitlements, from `codesign -dvv`) runs
`fibc run` (JIT at `-O0` through the fast code generator, `-O1`, `-O2`) and the macro runner (every case that expands a user macro passed), with no
`MAP_JIT` code of ours. So a developer-run `fibc` needs nothing. What is *not* known: a hardened-runtime or notarised `fibc` (needs
`com.apple.security.cs.allow-jit`, and `allow-unsigned-executable-memory` is not needed because LLVM uses `MAP_JIT`); the distribution decision is A64-5.
One failure is JIT-specific: `cases/stdlib/6223-vectors-cross-calls-and-loops-at-odd-widths.fib` crashes (exit 139) under `fibc run -O 0`
(`jit-new-fast-codegen`, FastISel) and passes under `-O 1`, `-O 2`, and as an AOT executable at `-O 0` and `-O 2`. A64-4.

### 2.4 The call shim and the mailbox

`compiler/native/support.fib:16-61`, `call.fib:1-20,88,107`: lIR text; `lair_shim_run` calls a C address with six integers, eight doubles and two
integer stack words, "as the System V ABI places each class". It is reached through libc's `bsearch` with one element (calls its comparison once) and `pthread_create`.
AAPCS64 differs: eight integer registers (x0-x7), eight vector registers (v0-v7), stack for the rest, and **Apple's variant puts variadic arguments on the stack**.
The shim is lIR compiled by LLVM for the target, so it follows the target's convention by construction; whether its "six integers in registers, two on the stack" layout is what an arm64 callee reads for a callee with 7 or 8 integer arguments is *analysis only, not tested* (AAPCS64 passes all 8 in x0-x7). **Ran on the Mac**: every JIT call (`fibc run`, macros, `lairf`'s `execute`) goes through it and works. Not exercised: a macro or hook with more than 8
integer arguments, or a call to a *variadic* C function through the shim (it declares none).
The mailbox lays out a `pthread_mutex_t` as 5 words and a `pthread_cond_t` as 6 (`support.fib:68`, offsets 32-43, glibc x86-64 sizes 40 and 48). Darwin's are 64 and 48 bytes
(`_opaque[56]` plus a signature word; `__sig` + `_opaque[40]`), so the mutex would overflow into the first condition variable. This is analysis from the header sizes, not a measured fault (the macro cases pass natively); A64-4 should enlarge the record and re-run them.
The runtime's own mutex (`crates/fibc/rt/task.lir:26-34`) allocates 64 bytes each: large enough for Darwin.

### 2.5 `extern`, varargs

Externs lower to LLVM declarations with a variadic function type when `:varargs` (`native/lower/types.fib:39`, `emit/program.fib:144`); the call
instruction uses that function type (`native/lower/calls.fib:50`), so LLVM applies the Darwin rule (variadic arguments on the stack) at every call site.
Uses: `fcntl` (`lib/platform/darwin/fib/os/backend.fib:11`), `openat` with a mode (`crates/fibc/rt/sys.lir:7`, `declare openat i32 (i32 ptr i32 ...)`),
`printf`-style externs written by users. **Ran on the Mac**: the `openat` path (every `read-file`, `write-file`) and the Darwin `fcntl` (pipes, sockets: 7402 and 7404
time out, 7.3, so the `fcntl` path itself is *not* yet shown to work). The rule for authors: a C function that is variadic must be declared `:varargs`
or the call is wrong on Apple arm64 and right on Linux x86-64 and aarch64: the opposite of how it fails on x86, where a missing `...` is only a missing `%al`.

### 2.6 The runtime `rt/*.lir` and its libc

| What | Where | Linux | Darwin | Status |
|---|---|---|---|---|
| errno location | `rt/sys.lir:20,26`; `lib/fib/os/backend.fib:3` | `__errno_location` | `__error` | adapted by `emit/os.fib` (text replace) |
| `openat` cwd | `rt/sys.lir:39` | `AT_FDCWD = -100` | `-2` | adapted |
| monotonic clock | `rt/sys.lir:148` `fib.sys-clock (i32 1)` | `CLOCK_MONOTONIC = 1` | `6` | adapted |
| processor count | `rt/task.lir:155` `sysconf (i32 84)` | `_SC_NPROCESSORS_ONLN = 84` | `58` | **not adapted**: `sysconf(84)` is `-1` on macOS, so the pool starts one worker (`select slt n0 1`): correct but single-core. A64-3 |
| `strfromd` | `lib/fib/fmt/libc.fib:5`, `fmt.fib:26` | glibc | absent | **fails to link/JIT** (case 4205). A64-3: use `snprintf` declared `:varargs` |
| errno numbers | tests (`3607`: `-39` for ENOTEMPTY) and `lib/fib/os/abi-data.fib` | Linux | ENOTEMPTY is 66 | the library maps through `abi-data`; the *test* hard-codes Linux numbers |
| `sched_yield`, `pthread_*`, `getenv`, `malloc` family | rt | POSIX | POSIX | work (ran) |

The adaptation picks the OS from the *host* (`fib.os.platform.detect/current`, `uname`) in `emit/rt.fib:10` and `expand/roots.fib:126`. Cross-compiling needs the
target's: this package adds `compiler/expand/targetos.fib` (`FIB_TARGET_TRIPLE` decides: an Apple triple is Darwin, anything else Linux) and makes the JIT's runtime
(macro modules, `def` evaluation) stay on the host's OS (`emit/rt.fib` `runtime-text-host`, `emit/assemble.fib` `module-parts-with`); without that, a cross
compile of a program with a macro asks the *host* JIT to run a `__error` call (17 cases failed that way before the change).

### 2.7 `lib/fib/os` ABI data, struct layouts, syscalls

`lib/fib/os/abi-data.fib` has `linux-abi` and `darwin-abi` as data (flags, errno values, `sockaddr` shape, `uname` field width 65 vs 256, dirent offsets 19/21);
Darwin is the same on x86-64 and arm64 (LP64, the same BSD headers) *except*: `readdir$INODE64` exists only on Intel (the symbol does not exist on arm64: link error
`Undefined symbols: _readdir$INODE64`, hit and fixed in this package: `lib/platform/darwin/fib/os/backend.fib` now binds plain `readdir`; Intel macOS is not a target).
There are **no raw syscall numbers** in `rt/*.lir` or `lib/`: everything goes through libc, which is what makes macOS (which forbids raw syscalls) feasible. `size_t` and `long` are 64 bits on
LP64 Linux and Darwin alike and are written `i64` throughout; `int` is `i32`. Structs the compiler lays out itself (`emit/layout.fib:107-127`) assume
pointer 8/8, i64 8/8, `LirVec` (n lanes, any width) **alignment 8** and size = lane bytes; that is independent of the CPU. The compiler's own dirent walk
(`native/cases/walk.fib:14`) hard-coded glibc's `d_name` offset 19: now 21 on Darwin (`native/cases/walk.fib`, `*`). `/proc/self/exe` is used by
`lairf cases` (`native/cli.fib:217`) and `lairf fuzz` (`native/fuzz.fib:17`) and has no macOS equivalent: use `fib.os/executable-path` (which has a Darwin `_NSGetExecutablePath`
backend, and which `fibc cases` already uses): A64-3.

### 2.8 Vectors in objects (SIMD3)

Vectors are stored inside objects as `[k x i64]` (`emit/value.fib:36-49`) and loaded/stored with `(align 8)` (`value.fib:49`), so object layout does not depend on the target's vector
alignment (AArch64's ABI alignment for `<2 x double>` is 16; x86's is 16 for 128 bits and 32 for 256). Explicit-alignment vector loads (`emit/lower/simdmem.fib:69`) use the
alignment the program states; AArch64 has no aligned-only vector loads, so a misstated alignment does not fault. **Ran on the Mac**: cases 6219 (vector in a struct, enum, option), 6221, 6244, 6245 pass natively.

### 2.9 Linker

`native/link.fib:75-80`: `cc OBJ -o OUT -lm -lpthread [-LDIR -Xlinker -rpath -Xlinker DIR].. [-lNAME]..`; on macOS this is Apple's `ld` through `cc`: `-lm` and `-lpthread` are
accepted (both are in libSystem), `-Xlinker -rpath` is accepted. **Ran on the Mac**: `fibc build` produced working executables, and linked the
compiler itself against `-L/opt/homebrew/opt/llvm@21/lib -lLLVM-21` with an rpath. Warning seen for a triple with no OS version (`aarch64-apple-darwin`:
"no platform load command found ... assuming: macOS"); use `arm64-apple-macosx13.0.0` (the scripts do). `scripts/llvm-static.sh` writes a **GNU ld script** with a `GROUP (...)`;
Apple's ld has neither (it rescans archives itself and takes `-lLLVM...` archives directly); and `AS_NEEDED(-lstdc++ ..)` is `-lc++` on macOS. A static-LLVM release on macOS is
A64-6. The dynamic route (Homebrew `llvm@21`, rpath) works today and is what `mac-check.sh` uses.

### 2.10 Release packaging, install, seed, CI

* `scripts/package.sh:2,28,32,91-92,99`: tarball name `fibc-VERSION-linux-x86_64`, `FIB_TARGET_CPU=x86-64-v2` baseline, `ldd`-based dependency check, `objdump` AVX check: all Linux x86.
  macOS needs `otool -L`, `FIB_TARGET_CPU=apple-m1` (no baseline problem: every Apple Silicon Mac has it), `codesign`, and a naming rule `fibc-VERSION-macos-arm64`.
* `SEED` (one `url=` and `sha256=`, `scripts/fetch-seed.sh`): a single x86-64 Linux tarball. Needs a per-platform table (A64-6): `SEED` gets `url.linux-x86_64=`, `url.macos-arm64=` lines (or
  `SEED.macos-arm64`), and `fetch-seed.sh` picks by `uname -s`/`uname -m`. Until a release has an arm64 seed, the **cross seed** of this package (built on x86 by
  `a64-fibc-on-mac.sh`) is the bootstrap.
* `.github/workflows/release.yml:26,110`: `ubuntu-latest` only; a macOS arm64 job (`macos-14` or later runners are arm64) needs Homebrew `llvm@21` and the seed above.
* The install: `compiler/expand/libdir.fib:8` finds the library by the executable's location through `fib.os/executable-path`, which has a Darwin backend: no change.
* The stage-2 loop on x86 is `fibc build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21`; on the Mac the same with `-L /opt/homebrew/opt/llvm@21/lib`.
  **Ran on the Mac** (6.3).

### 2.11 Other

`FIB_TARGET_CPU` (`llvm/target.fib:62`) is read by `requested-cpu`; `scripts/package.sh` exports `x86-64-v2` for the whole build, which is wrong for any aarch64 build (the CPU name
`x86-64-v2` is not a CPU of the AArch64 backend: LLVM warns and falls back to generic). With a triple set, `FIB_TARGET_CPU` names an AArch64 CPU (`apple-m1`, `apple-m4`, `neoverse-n1`, ...).
`crates/` (Rust seed, `fibref`, legacy `lair`) is frozen and x86-Linux only; nothing in this package touches it.

## 3. What the spike adds

* `--target TRIPLE` (and `FIB_TARGET_TRIPLE`) for `lairf check|build|emit-llvm` (`native/cli.fib` `take-target`) and for `fibc` (`fibc.fib` `take-target`, any command that reads a program).
  The default (neither set) is the host, unchanged: the x86 gate passes (6.4).
* `llvm.target`: AArch64 initialisers, `requested-triple`, `aarch64-triple?`, `apple-triple?`, `cross-cpu`, `cross-machine`. `native.target`: `machine-target`, `cross-target-info`, `cross-target?`.
* `native.aot`: the object, assembly and IR come from the cross machine; **linking an executable for another target is refused** with a message that says to use `--emit obj|asm` (we do not
  have the target's `cc`/libraries, and a host link of foreign code would be wrong).
* `expand.targetos`: the target's OS name (the library's OS backend and the runtime adaptation follow it); `emit.rt` `runtime-text-host` for the JIT.
* `fibc emit --exe`: the lIR of an *executable* (what `build` compiles; `emit` alone prints the `run` shape whose `main` prints its result). The cross flow is
  `fibc emit --exe --target T prog.fib > prog.lir; lairf build prog.lir --target T --emit obj -o prog.o`, then link where the target runs.
* Darwin: `readdir` without `$INODE64`; the dirent offset by platform in the compiler's own walker.
* Scripts: `compiler/tests/native/a64-emit.sh` (6.1), `a64-cross-mac.sh`, `a64-fibc-on-mac.sh`, `scripts/mac-run-objects.sh`, `scripts/mac-check.sh`.

Decisions: (1) one environment variable and one flag, no new tool: the target is global process configuration read at the three places that need it, as `FIB_TARGET_CPU` already is. No global
mutable state is added in fibber code; the "state" is the process environment, as before. (2) The JIT never takes the target: it executes. (3) `cross-target-info` lives in `native/target.fib`, not `types/target.fib`,
only because SC1 is editing the latter; the rows belong in `target-of-cpu` once it is free (A64-2).

## 4. Triples

| Triple | Object | Notes |
|---|---|---|
| `arm64-apple-macosx13.0.0` (also `aarch64-apple-darwin`) | Mach-O arm64 | **ran on the Mac**; give an OS version or the linker warns |
| `arm64-apple-ios` (e.g. `arm64-apple-ios16.0`) | Mach-O arm64, platform iOS | **emitted only** (object type checked with `file`; no iOS SDK/linker here) |
| `aarch64-unknown-linux-gnu` | ELF aarch64 | **emitted only** |

## 5. iOS

What is different, and the story (none of it run):

* **No JIT outside entitlements.** An App Store app cannot map writable-then-executable memory; `MAP_JIT` needs the `dynamic-codesigning` entitlement that only some apps get.
  So `fibc run`, the macro runner and the REPL-shaped paths do not exist on device. The AOT path is the iOS path: **fibber code is compiled ahead of time to an object**.
  Macros run in the *compiler*, on the Mac, never in the app; that is already how `fibc build` works.
* **Cross-compile on the Mac** (or on x86 Linux, which has no Xcode SDK to link with): `fibc emit --exe --target arm64-apple-ios16.0 app.fib` then
  `lairf build ... --target arm64-apple-ios16.0 --emit obj`. The object is Mach-O with the iOS platform load command (for device: `arm64-apple-ios`; Simulator on Apple Silicon is
  `arm64-apple-ios16.0-simulator`, a different platform tag: both must be built to run in both).
* **Library, not executable.** An Xcode app owns `main`. The deliverable is a static library (`libfibapp.a`: the object, and `libtool -static`/`ar`) plus a C header of the entry points. That needs
  an **exported-function mode**: `emit.compile` today makes one `main` module; a `(defun ^export ..)` marker (or an `extern`-visible attribute) lowered to external linkage with the C ABI is
  A64-7. Until then the app calls `main` as the one entry (it is an ordinary C `main(argc, argv)` in the object: rename with `-Dmain=fib_main` at the link, or `objcopy`-style `ld -alias`).
* **No `fork`/`exec`, no `/bin/sh`.** `fib.os` process spawning (`lib/fib/os/process.fib`) and anything using `posix_spawn` of arbitrary programs are unavailable; the library should compile a
  `Unsupported` backend for iOS (`platform/ios/fib/os/backend.fib`) whose process functions return `Err`. `uname` returns `Darwin`; the platform probe cannot tell iOS from macOS
  (A64-7: `TARGET_OS_IPHONE` is a compile-time fact, so `targetos` should return `Darwin-ios` from the triple, and `fib.os.platform` get a variant).
* **Sandbox**: filesystem access is the app container; `/tmp`, `/proc` absent (the compiler is not on device). `getenv("HOME")` is the container.
* **Code signing**: every Mach-O in the bundle is signed by Xcode at build; nothing for fibber to do beyond producing a valid object. Bitcode is gone (Xcode 14).
* **Threads, atomics, SIMD**: pthreads work; NEON is baseline; the runtime's `pthread_*` layouts (2.4) are Darwin's on iOS too.
* The Target for iOS devices is `apple-a14`/`apple-m1`-class: `cross-cpu` returns `apple-m1` for every Apple triple, which is a superset of all current devices that run iOS 16 (older devices may lack extensions `apple-m1` enables: use an older Apple CPU name in `FIB_TARGET_CPU` for an app that must run there; not checked). **Emitted only.**
* An example app is out of scope.

## 6. Verification

### 6.1 No-hardware checks (this machine): `compiler/tests/native/a64-emit.sh`

Quoted from `a64-emit.sh` (full mode; `F` is the stage 2 built from this tree, `L` its lairf):

```
ok   aarch64-unknown-linux-gnu: lowering and code generation of 51 lIR cases
ok   aarch64-unknown-linux-gnu: 101 fibber programs compile through fibc emit --target and lairf
ok   aarch64-unknown-linux-gnu: object is ELF 64-bit LSB relocatable, ARM aarch64
ok   aarch64-unknown-linux-gnu: data layout e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32
ok   aarch64-apple-darwin: lowering and code generation of 51 lIR cases
ok   aarch64-apple-darwin: 101 fibber programs compile through fibc emit --target and lairf
ok   aarch64-apple-darwin: object is Mach-O 64-bit arm64 object
ok   aarch64-apple-darwin: data layout e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32
(the same four for arm64-apple-ios)
ok   aarch64-unknown-linux-gnu: kernel assembly has fmla and fadd on .2d and .4s     (and the two Apple triples)
ok   a 2 x double fma with +neon,+fp-armv8 is one fmla
ok   planted wrong triple (x86-64): the NEON check fails, as it must
ok   planted wrong data layout (x86-64's): the layout check fails, as it must
ok   planted wrong vector width (<4 x double>): 2 fmla, the width check fails, as it must
ok   planted nonsense triple refused: ... error: no target for the triple notanarch-unknown-linux: No available target ...
a64-emit: every check holds and every planted fault is caught
```

The 101 programs are 40 of `cases/ownership` and up to 60 of `cases/stdlib/62[0-5]*` (f64x2, f32x4, `<<..>>` literals, masks, shuffles, conversions, float functions) plus `compiler/tests/native/a64/kernel.fib`.
The kernel's assembly for Darwin has `fmla.2d v3, v2, v0` and `fadd.4s`; for ELF `fmla v0.2d, v2.2d, v5.2d` and `fmla v4.4s, v3.4s, v1.4s`. Planted faults were each seen to fail first
(a wrong data layout string, an x86 triple, a 256-bit vector) while the unplanted checks passed.

### 6.2 What `a64-emit.sh` cannot show

It does not run AArch64 code; 6.3 does, on the Mac. The iOS and Linux triples are emitted only.

### 6.3 On the Mac (ran)

1. `compiler/tests/native/a64-cross-mac.sh --max 400`: after the macro-runtime fix, "cross-compiled 272, failed to compile 0 ... run-objects: 270 pass, 2 fail". The two: `188` reads `spec/method.md` by a relative
   path from a directory that has none (an artefact of where the program ran) and `195-trap-alloc-out-of-memory` returns 7 instead of trapping: at `-O 2` LLVM deletes the `calloc`/`free` pair
   (the same binary built at `-O 2` on x86 Linux also exits with 7, so this is the case's `-O 2` behaviour, not aarch64).
2. Stdlib SIMD, tensor and constraint cases (62xx, 70xx, 72xx; 169 programs): "run-objects: 166 pass, 3 fail": `7077`, `7078` (fma GEMM, below) and `724` (labelled open).
3. The compiler on the Mac: `a64-fibc-on-mac.sh` cross-compiles `fibc` (24.3 MB of lIR, 54 s for the object) and `lairf`, links them with
   `cc ... -L/opt/homebrew/opt/llvm@21/lib -lLLVM-21 -Wl,-rpath,...`, and `./fibc --version` prints `fibc 0.1.5`.
   `fibc build hello.fib -o hello` and `fibc run hello.fib` both print `hello from fibber on aarch64`.
   `fibc cases cases/ownership -j 8`: "311 cases: 310 pass, 1 fail" in 39 s (the failure is case 188 with `spec/method.md` absent).
   `fibc cases cases/stdlib -j 10`: "1225 cases: 1196 pass, 8 fail, 0 pending, 0 header error, 21 open" in 8 min 7 s. The 8: `1707` (also expected to fail on x86: `scripts/ci-stage2.expected`),
   `3607` (test hard-codes Linux's ENOTEMPTY, 39), `4205` (`strfromd`, 2.6), `6223` (JIT `-O 0`, 2.3), `7077` and `7078` (below), `7402` and `7404` (OS pipes and TCP loopback: timed out after 300 s).
4. The fixed point: the cross-built `fibc` builds `F2` natively in 63 s (`fibc build compiler/fibc.fib ... -o F2`); `F2` builds `F3`; then
   `cmp e1.lir e2.lir` (fibc against F2) and `cmp e2.lir e3.lir` (F2 against F3) print `fibc==F2` and `F2==F3`.
5. `scripts/mac-check.sh --quick` was run on the Mac: see 6.5.

`7077` and `7078` ("fma GEMM equals the ordered fma reference") fail identically on x86 Linux with `FIB_TARGET_CPU=x86-64` (128 bits, no fma) and pass with `x86-64-v3`: they depend on the target having fused multiply-add, which the library
selects through `Target.has-fma`, and nothing in `lib/` consumes `has-fma` yet. They are SC1's: with `has-fma` true for aarch64 (`cross-target-info`), they should pass on the Mac once the library reads it.

### 6.4 The x86 gate

`GATE_FRESH=1 FIBC=<v0.1.5 seed fibc> scripts/gate.sh --full -j 8` on the tree of the second commit of this package: `fixed point: F and F3 emit the same lIR (78.2 s)`,
`cases: ownership 18s (exit 0), modules 1s (exit 0), stdlib 205s (exit 1)` (the stdlib exit is the expected-failure set compared by the gate), `GATE PASS (full)`. Later commits touch only `compiler/tests/`, `scripts/`, `docs/`.

### 6.5 `scripts/mac-check.sh`

Written for the owner; run on the Mac in `--quick` mode with FIBC = the cross seed. Output (ran on the Mac):

```
ok   step 1      (arm64, cc, libLLVM-21.dylib, seed runs: fibc 0.1.5)
ok   step 2      (stage 2 built by the seed)
ok   step 3      (F builds F3; same lIR)
ok   step 4      (hello: AOT and JIT)
311 cases: 311 pass, 0 fail, 0 pending, 0 header error
ok   step 5
209 cases: 202 pass, 7 fail, 0 pending, 0 header error     (--only 62 70 74 42 36; all 7 are in KNOWN)
ok   step 6
  JIT -O 0: exit 139   JIT -O 1: exit 0   JIT -O 2: exit 0   AOT: exit 0      (case 6223)
mac-check: 0 step(s) failed
```

Step 6 passes by listing the known failures; a failure outside `KNOWN` is printed and fails the step.

## 7. Open findings (the Mac and the census)

1. `sysconf(84)` (2.6): one worker thread on macOS. 2. `strfromd` (2.6). 3. Mailbox mutex size (2.4). 4. JIT `-O 0` crash on 6223 (2.3). 5. `lairf cases`/`fuzz` use `/proc/self/exe` (2.7).
6. 7402/7404 OS pipe/TCP timeouts on Darwin: the Darwin backend has been type-checked but never run before (`docs/design/os.md`); `fcntl` variadic calls and the `socket`/`accept` fallbacks need a look.
7. `has-fma`/`target-of-features` on a native aarch64 host (2.2). 8. `package.sh`, `llvm-static.sh`, `SEED`, CI are Linux-x86 only (2.9, 2.10).
9. iOS has no backend variant, no exported-function mode (5).
10. The macro runner under `FIB_TARGET_CPU`/a different OS: a macro that itself uses `fib.os` is compiled with the *target's* OS backend by `roots.fib` while it runs on the host; no case does today.

## 8. Package plan

Sizes: S under a day, M one to three days, L about a week. "Linux" is what can be verified on this x86 box (object and assembly emission, or the cross-run to the Mac that is currently available); "Mac" needs hardware.

| Package | What | Depends | Verifiable on Linux | Needs a Mac |
|---|---|---|---|---|
| **A64-0** (this) | `--target`, AArch64 init, cross machine, `targetos`, tests, scripts, this design, cross seed | | `a64-emit.sh`, the x86 gate | `a64-cross-mac.sh`, `mac-check.sh` (done once: 6.3) |
| **A64-1** (S) | land A64-0; add `a64-emit.sh --quick` to `scripts/gate.sh` (or CI) so the aarch64 emit path cannot rot; `mac-check.sh` into the release checklist | A64-0 | yes | no |
| **A64-2** (S) | Target rows for aarch64 in `types/target.fib`: `cpu-aarch64?` table (apple-m1..m4, neoverse-*), `has-fma` true, 128 bits; `target-of-features` knows an aarch64 host (`+neon`, no `+fma` feature); remove `cross-target-info` | SC1's `has-fma` | unit tests; `a64-emit.sh` kernel | one native run of 7077/7078 |
| **A64-3** (M) | Runtime and library portability: `sysconf` constant by OS (`rt/task.lir`, adapted in `emit/os.fib`), `strfromd` to `snprintf :varargs`, `lairf cases/fuzz` through `executable-path`, mailbox mutex size, `abi-data` errno in the tests (`3607`); 7402/7404 Darwin pipes/TCP | A64-0 | emit, x86 gate (changes are shared code) | yes: `mac-check.sh` must list fewer KNOWN |
| **A64-4** (M) | JIT on arm64 macOS: root-cause 6223 at `-O 0` (FastISel/ORC), measure JIT startup and macro-heavy builds vs x86, mailbox/shim review against AAPCS64 for more than 8 integer arguments and variadic callees | A64-0 | no | yes (lldb on the Mac) |
| **A64-5** (S) | Distribution decision: hardened runtime and `com.apple.security.cs.allow-jit` entitlement for a notarised `fibc`; ad-hoc is enough for a developer-built `fibc` (measured) | A64-4 | no | yes (`codesign`, `spctl`) |
| **A64-6** (L) | Release: per-platform `SEED`/`fetch-seed.sh`, `package.sh` for macOS (name, `otool -L`, `codesign`, `apple-m1`), static or dynamic LLVM (the `llvm-static.sh` ld script has no macOS form; Homebrew-dynamic is the fallback), a macOS arm64 CI job, a first `fibc-0.1.x-macos-arm64` seed made by the cross route and then by itself | A64-1, A64-3 | partially (the script logic) | yes |
| **A64-7** (L) | iOS: exported-function / static-library mode; `platform/ios` OS backend (no process spawn); `Darwin-ios` platform variant; simulator triple; a hello-world static library that an Xcode project links (the example app stays out of scope) | A64-6 | object emission for `arm64-apple-ios` | yes, plus Xcode and a device or simulator |
| **A64-8** (M) | Linux aarch64: a Linux arm64 seed (the cross route), `neoverse`/Graviton CPU table and baseline (`FIB_TARGET_CPU=generic`/`neoverse-n1`), CI on an arm64 runner | A64-6 | emit (done) | no, but an aarch64 Linux box |
| **A64-9** (M, later) | Performance: NEON tensor kernels (tile sizes for 128-bit, 32 registers), bench on M1/M4; SME is *not* planned until LLVM exposes streaming mode usefully | A64-2 | no | yes |

**The order that got a hello world running on a Mac soonest**, and the order to keep: A64-0 (cross-compile on x86, link on the Mac: done, hello ran) → the cross seed (native `fibc` on the Mac: done) → self-host on the Mac (done) → A64-1/A64-3
(the cleanups that make `mac-check.sh` green) → A64-2 → A64-6 → A64-4/5 in parallel → A64-7 → A64-8, A64-9.
The reason the cross route is short: `fibc emit` is pure fibber and needs no LLVM backend for the target; only `lairf build --emit obj` needs the AArch64 backend, and the stock LLVM 21 has it.

## 9. What was not done

Nothing ran on iOS, the iOS Simulator, aarch64 Linux or an M4. The Mac's `mac-check.sh` was run in `--quick` mode only. A static-LLVM macOS build was not attempted. A hardened-runtime or notarised `fibc` was not tried.
`has-fma` is set but unconsumed; the SIMD library was not changed. No performance measurement was taken. The Rust seed and `crates/` were not touched.
