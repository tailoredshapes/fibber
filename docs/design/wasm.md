# WebAssembly: `fibc build --target wasm32-wasi`

Status: built and measured, WASM-2, 2026-10-07. The wasm32-wasip1 row runs fibber programs; the numbers below are from this machine (x86-64 Linux, node v26.10.0, wasmtime 49.0.2,
wasi-sdk 34). The design this carries out is docs/design/targets.md (the target table, the plan W1-W7); ADR 0008 puts wasm outside the supported ISAs as a *portability target*.
What is not done is in section 9.

The owner's goal: "Wasm and js feel like sensible next targets"; "being able to target all of the backends provided by llvm is the end goal".

## 1. What it does

```
fibc build prog.fib --target wasm32-wasi -o prog.wasm        # links with wasm-ld against wasi-libc; runs under node:wasi, wasmtime, or any WASI preview 1 host
node compiler/tests/wasm/run.mjs prog.wasm                    # node's built-in WASI (prints no experimental warning with --no-warnings)
fibc build lib.fib --target wasm32-wasi -o lib.wasm --export NAME[=SYMBOL] ..   # a library a JavaScript host calls (section 5)
```

`--target wasm32-wasi` (and `wasm32-wasip1`) is the row of `fibc targets` named `wasm32-wasip1`. The code is `wasm32` with `+simd128,+tail-call`; the executable has a WASI
`_start` that calls fibber's `main`. `wasm32-unknown-unknown` still only emits (`--emit obj|asm|llvm`): it needs a libc-free runtime (section 9).

## 2. The platform interface under WASI, and what a program may use

The runtime (`rt/*.lir`) and `fib.os` need the services of docs/design/platform-boundary.md 1. Under WASI preview 1 through wasi-libc:

| service | under wasm32-wasi |
|---|---|
| memory | wasi-libc's `malloc` (dlmalloc), which grows the linear memory with `memory.grow`: **that is the page provider**; the runtime's free lists and the large-block cache sit on it as everywhere. No `madvise`, no `mallopt` |
| fd I/O, files, directories | `fd_read`, `fd_write`, `path_open`, ... through wasi-libc. The library's Linux open flags and `AT_FDCWD` are mapped to WASI's (`fib.w.oflags`); the host must preopen the directories the program uses (`node run.mjs --dir /tmp`, `wasmtime --dir`) |
| clock | `clock_time_get`: the runtime's `clock_gettime(id, ts)` is a shim that maps the numeric ids to wasi-libc's clock objects and widens `tv_nsec` |
| entropy | `getentropy` (`random_get`) |
| arguments, environment | `args_get`, `environ_get` (`main(argc, argv)`, `getenv`) |
| exit, trap | `proc_exit`; a trap writes `trap: MESSAGE` to standard error and `abort`s, which is the wasm `unreachable`: node's runner ends with status 134 as a native `abort` does |
| threads | none. See 3 |
| sockets, `fork`/`exec`, pipes, `dup`, host names, `mkdtemp`, the path of the executable | not in preview 1. `fib.os` answers ENOSYS where the library asks (`lib/platform/wasi/fib/os/backend.fib`); a program that calls `fork`, `getaddrinfo` or `mkdtemp` does not link, and the link error says what to do |
| errno | wasi-libc's numbers are WASI's (ENOENT is 44); the runtime and `fib.os` answer the Linux numbers of the same errors, so the library's error kinds are one table |

Why wasi-libc and not "no libc": the runtime needs about 50 C functions (`compiler/types/targets.fib` `runtime-c-functions`); wasi-libc has 47 of them and `snprintf`/`strtod` (the float
printer and reader, `%.800e`, are the hard part of a libc-free runtime). The measured price is size: a hello world is **43 KB** stripped, 231 KB with the name section (of which `printf`'s core
is 8.7 KB, `dlmalloc` 5.4 KB): the libc is what the module is, not a dependency to apologise for. The libc-free design of targets.md 5.3 stays right for `wasm32-unknown-unknown` (the browser
without WASI), where it is most of the work; it is not done.

`libc-lacks` of the row (checked against `libc.a` of wasi-sdk 34 with `llvm-nm`): `madvise`, `mallopt`, `pthread_exit`.

## 3. Threads: tasks run on the thread that asks

WASI preview 1 has no threads (the threads proposal and `wasi-threads` are a separate build, `wasm32-wasip1-threads`, section 9). ADR 0008 names no rule for wasm threads, so this is decided here:
**a program builds and runs, and what needs a second thread of execution is refused at run time with a message, not at compile time**, because the same source is a valid program on every other target.

- `async` and `plet`/`pmap` tasks run on the thread that joins them: the pool starts no worker (a joiner already drives the queue itself when it waits), and the exit drains what is queued (`fib.pool-quiesce`).
- `spawn` runs the task **at the spawn**, to its end, on the caller's stack (`fib.w.run-inline` in place of `pthread_create`). Its result is the same; the order is not (the spawner does not run
  while the task does). A program that relies on the two overlapping (a spin wait on an atom another thread sets) does not finish: the wait it makes is `trap: deadlock: this wait needs another thread, and a WASI program has one`, and a pure spin loop is ended by the host's time limit.
- A task that traps ends the program (no thread to end): `fib.fail-task`'s "the task failed with MESSAGE" cannot be told apart from the trap.
- Atoms work (they are `cmpxchg` and a spinlock on a single thread). `fib.parallel`'s worker count is 1.

The cases this changes are listed in compiler/tests/wasm/expected.txt with the reason.

## 4. The 32-bit target

- **Pointers are 4 bytes** (`emit.layout/size-align-pb`, `emit.program/program-pb`). The sizes of objects, element sizes of arrays and vectors, and the text of the runtime that says `8` for a pointer
  (`emit.wasi/rewrites`) come from the target row. `layout-dump` for the target shows them; cases 8251 (containers of objects at 4-byte pointers) and 8252 (constant `def`s of objects) hold them.
- **The JIT runs on the host.** Constant `def`s and macros are evaluated by running the program's code in an in-process JIT, so for a target whose pointers are not the host's they are laid out
  and lowered for the host; the constants come back as text (LLVM lays the struct constants out for the target), and the rest of the module is lowered afresh at the target's width
  (`emit.compile/make-defs-at`, `emit.program/program-rebuilt`). A macro's module always uses the host layout (`macro-module`).
- **`size_t` and `long` are `i32`** in wasi-libc. The runtime and the library write them `i64`; the lowering declares the libc functions of `types.targets/libc-narrow-names` with the target's
  types and narrows each argument and widens each result (`native.lower.calls`). A size that does not fit becomes the largest `i32` (so that `malloc` fails, not that its low 32 bits succeed).
  `lseek`'s `off_t` is 64 bits and stays.
- **`main`** is wasi-libc's `__main_argc_argv`.
- The generated code is the same lIR; `emit.wasi/adapt` is the whole of the runtime's differences (a list of exact text replacements, each checked to be in the runtime by
  `compiler/tests/emit/unit-wasi.fib`, so a change of `rt/*.lir` that moves one fails a unit instead of quietly leaving a 64-bit assumption).

## 5. Calling fibber from JavaScript

`fibc build lib.fib --target wasm32-wasi -o lib.wasm --export NAME[=SYMBOL]` (any number of `--export`; FIB_EXPORTS carries them) makes the function `NAME` of the program a symbol the module
exports (`SYMBOL`, default `NAME`), with the C calling convention. There is no `main`: the module is a library (wasi-libc's reactor start, `_initialize`; the fibber runtime starts on the
first call: `fib.export-ready`). An export takes integers, floats and raw `ptr`s and returns one of them or nothing; a generic function, a function that returns two words, or a program that catches is refused with a message.
The module also exports `malloc`, `free` and `memory`, so the host makes the memory a `ptr` argument points to and fills it through a typed array.

```
$ fibc build examples/wasm/kernels.fib --target wasm32-wasi -o kernels.wasm --export add --export dot --export sum-squares=sumSquares --export fib-vec-length=vecLength
$ node examples/wasm/kernels.mjs kernels.wasm
add(40, 2) = 42n          # an i64 is a BigInt in JavaScript
dot = 300                 # two Float64Arrays copied into the module's memory
sumSquares(1000) = 332833500n     # allocates vectors inside the module
vecLength(5) = 5n
```

No build step beyond `fibc`: the host needs `WebAssembly` and a WASI shim (`node:wasi`; in a browser a ~50-line one, or `@bjorn3/browser_wasi_shim`), because the module imports `wasi_snapshot_preview1`.
Strings and vectors do not cross: a host passes bytes and a length and the module builds the value inside (a `jsbind` layer like `wasm-bindgen` is the next step, targets.md 5.3). `--export` also works with `--emit obj` for another target (the symbol is external with the C convention); linking such a library for the host is not done.

## 6. Tail calls

`+tail-call` is in the row's features and `tailcc` is the C convention with a `tail` hint (the WebAssembly backend takes `return_call` for any callee with the caller's result type; LLVM's
verifier lets `musttail` pass only between equal prototypes). ADR 0008's rule, that a tail call does not grow the stack, holds for wasm:

- case 8250: ten million tail calls between three functions of different signatures (two integers; two integers and a float; one integer) run in node's default stack (about 1 MB) at `-O 0`, `-O 1` and `-O 2`: **passes**;
- the negative control, the same program built with `FIB_TARGET_FEATURES=+simd128,-tail-call` at `-O 0`: node ends with `RangeError: Maximum call stack size exceeded`;
- the row's `min-level` is 1: at code-generation level 0 LLVM's FastISel makes no `return_call` (the assembly of case 8250 has none at `-O 0` with level 0; three with level 1), so `-O 0` is generated at level 1,
  as AArch64's is for another reason (targets.md 2). `compiler/tests/wasm/wasm.sh` `tail` and `tail-control` hold both.

The guarantee is as good as the backend's: a call LLVM does not make a `return_call` (a callee with a different result type does not occur in lIR, where a tail call returns what it calls) would grow the stack.
The host must have the tail-call proposal (node 20+, wasmtime 22+ by default, Chrome 112+, Firefox 121+, Safari 18.2+).

## 7. SIMD

`+simd128` is on and `native-lanes` is 4 for f32 and 2 for f64 (the row says 128 bits, as aarch64's NEON): the fixed vector types lower to `v128` (`f64x2.mul`, `f32x4.add`, `i8x16.eq`, ..), wider types are split by LLVM.
Without FMA in the instruction set (ADR 0008): `simd/fma` is a compile-time warning and a run-time trap, `simd/muladd` a multiply and an add, `(has-fma)` is false.
`FIB_TARGET_FEATURES=+tail-call,-simd128` builds without v128 (any target: it replaces the row's features), which `wasm.sh` checks for `v128` instructions in the assembly.

Results (suite.py over the 191 cases of the vector, tensor and movemask blocks `62xx`, `70xx`, `774x`, `777x`, `796x`, `798x`): **182 pass with simd128 and the same 182 with it off**; the 9 that do not are the same
in both: the seven that need an exact `simd/fma` (62xx `muladd`, `255`, 7077, 7078, 7770, 7960, 7961), 7771 (it asserts the fused branch of `has-fma`, which is the point of the target having none) and 7080 (the
tensor `exp`/`log`/`tanh` ULP bounds are against the C library's libm, wasi-libc's differs by more than 1 ULP somewhere in the sweep).

A found LLVM 21 fault: `bitcast <32 x i1>` to `i32` on the WebAssembly backend gives the wrong bits (case 7980, `simd/movemask` of 32 and 64 lanes). The lowering reads such a mask 16 lanes at a time on wasm32
(`emit.lower.simdfn/lcx-movemask-halves`), which is also what `bitmask` is.

`fib.tensor` (GEMM, vector math) and `fib.json` build and run on wasm through the same lowering; JSON-3 owns `lib/fib/json`, so only the result is reported: no json case is in the expected-failure list (the whole stdlib block runs, section 9), and the tensor cases fail only where they need an exact `simd/fma` or compare with libm.

## 8. Size and speed

Hello world (`(println "hello wasm")`): **43,495 bytes** (stripped; `FIB_WASM_NAMES=1` keeps the name section: 230,871), 17,479 gzipped. The native dynamic executable is 28 KB plus the C library it loads.
`wasm-opt` (binaryen) was **not used** and not downloaded.

Speed: `compiler/tests/wasm/bench.sh --fibc F` over scripts/bench, median of 3, node v26.10.0 (V8's Liftoff then TurboFan), the whole process including node's start and the module's compile:

| program | native s | wasm s | wasm / native | same output |
|---|---|---|---|---|
| num-f64 | 1.94 | 2.36 | 1.22 | yes |
| num-nbody | 0.74 | 2.25 | 3.03 | yes |
| vec-sort | 0.56 | 1.15 | 2.06 | yes |
| strings | 0.84 | 1.66 | 1.98 | yes |
| vec-index | 0.50 | 1.78 | 3.59 | yes |
| map-assoc-get | 0.37 | 0.47 | 1.26 | yes |
| vec-conj-pop | 1.06 | 2.62 | 2.47 | yes |
| set-conj | 0.34 | 0.64 | 1.85 | yes |
| lazy-fused | 1.09 | 2.14 | 1.97 | yes |

With `-simd128` the same run gives 1.17-4.25 (num-f64 1.32, vec-index 4.25): these programs do not use vector types, so simd128 does not move them. All nine print byte-identical output
to the native build (the checksum, not only the time). ADR 0019 holds.

## 9. Running the tests

```
scripts/fetch-wasm-tools.sh                       # wasi-sdk 34 and wasmtime 49 into ~/.cache/fibber-scratch/tools/wasm, sha256 checked (ADR 0020)
export WASI_SDK=$HOME/.cache/fibber-scratch/tools/wasm/wasi-sdk-34.0-x86_64-linux
python3 compiler/tests/wasm/suite.py --fibc F cases/ownership -j 4 --expected compiler/tests/wasm/expected.txt     # also cases/modules, cases/stdlib
bash compiler/tests/wasm/wasm.sh F                # the checks of the 32-bit target: hello, memory.grow, oversize, write, tail calls, movemask, vectors of objects, simd128, export (6 s)
bash scripts/mutant-wasm.sh F                     # planted faults, each must be killed by wasm.sh (about 8 minutes)
compiler/tests/wasm/bench.sh --fibc F             # native against wasm on node
GATE_WASM=1 scripts/gate.sh                       # the gate's wasm stage (the full gate has it); skipped, with the reason, when node or the toolchain is not there
```

The suite judges each case by its header as `F cases` does (`result`, `trap`, `reject`; `open:` cases that fail are excused), with the program built by `fibc build --target wasm32-wasi` and
`FIB_RESULT_STDOUT=1` (the executable prints `main`'s result as `fibc run` does, instead of returning it as an 8-bit status). `--runtime wasmtime` runs under wasmtime (`WASMTIME` names it).

Results (this tree, x86-64 Linux, `-j 4`), against compiler/tests/wasm/expected.txt (50 lines, each with its reason):

| directory | cases | pass on node | expected failures | other | seconds |
|---|---|---|---|---|---|
| cases/ownership | 354 | 351 | 3 (166, 167: a spin wait for a second thread; 195: an unused huge `calloc` is removed by LLVM) | 0 | 54 |
| cases/modules | 32 | 32 | 0 | 0 | 4 |
| cases/stdlib | 1337 | 1274 | 47 | 16 `open:` cases excused by their headers, as the native suite excuses them | 310 |

The 47 stdlib failures by reason: 12 tasks whose trap is isolated by the joiner (a trap ends a wasm program: 7601, 7608, 7875, 911, 915, 918-920, 923, 925, 926, 922); 4 spin waits or lock hand-offs between tasks that need two
threads of execution (7622-7624, 913; and 166, 167 of ownership); 3 that count the pool's workers or processors (7600, 7606, 912); 8 that need an exact `simd/fma` or assert the fused branch (6255, 6262, 7077, 7078, 7770, 7771,
7960, 7961); 1 libm ULP bound (7080); 6 `getaddrinfo`, 2 `fork`, 1 each `mkdtemp` and `getppid` that do not link; 6 pipes and sockets (ENOSYS); 7400 (asserts the Linux backend); 3607 (the errno of reading a directory); 7881
(the unused `calloc`). The same suites under wasmtime 49.0.2: ownership 351 and modules 32, the same expected failures (wasmtime's default stack is 1 MB: the runner gives it `max-wasm-stack=256 MiB`, as node gets `--stack-size`).

Every one of the 1274 passes is a case whose header the native run also holds: the result, the trap message or the rejection. The audit of the native run (`audit: clean`, the leak trace) is not run on wasm.

## 10. Not done

- `wasm32-unknown-unknown` (the browser without WASI): it emits; it has no libc, so it needs the libc-free runtime of targets.md 5.3 (an allocator over `memory.grow` in lIR, float formatting without `snprintf`, host imports for write and time).
- Threads (`wasm32-wasip1-threads`): shared memory, `+atomics`, `wasi_thread_spawn`; the inline `spawn` is the stand-in.
- A `jsbind` layer: strings and vectors across the boundary.
- wasm64 and the component model (`wasm32-wasip2`).
- `wasm-opt`.
- Sockets, `fork`, pipes: not in WASI preview 1.
- The browser: no browser was run; the module needs a WASI shim there.
