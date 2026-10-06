# A JavaScript backend for lIR

**Status: first working slice** (2026-10-06). `lir2js` translates a checked lIR module (spec/lir.md) directly to one ES module that runs
under node; no LLVM is involved. Its value beyond the fun of it: node is on every development machine, so the output runs with no
downloads; and it is a second code generator, independent of LLVM, so every lIR program can be run both ways and the results compared, a
differential check at the lIR level (`compiler/tests/js/diff.sh`). Readable output, and source maps later, are the third.

What is where:

| Path | What |
|---|---|
| `compiler/js/jx.fib` | the translator's state, JS names, the x86-64 layout of lIR types |
| `compiler/js/ty.fib` | the type of every expression; the type descriptors the runtime reads |
| `compiler/js/scalar.fib`, `cast.fib` | scalar arithmetic, comparisons and conversions as JS text |
| `compiler/js/mem.fib`, `call.fib`, `expr.fib` | memory forms; calls and intrinsics; the expression dispatcher |
| `compiler/js/func.fib`, `module.fib` | one function; the whole module |
| `compiler/js/rt/*.js` | the runtime, concatenated ahead of the module: `core` (heap, stack, function table, process end), `int`, `float`, `mem`, `fmt` (printf, strtod), `libc` (shims), `stdio` |
| `compiler/lir2js.fib` | the tool: `lir2js FILE.lir -o OUT.mjs`, `lir2js emit FILE.lir`, `lir2js run FILE.lir [ARGS..]` |
| `compiler/tests/js/diff.sh` | the differential harness; `expected.txt` its known non-passing programs |
| `compiler/tests/js/faults.sh` | the planted faults the harness must catch |
| `compiler/tests/js/rt-test.mjs` | unit tests of the runtime (fma, printf, strtof) against exact references |
| `compiler/tests/js/speed.sh`, `bench/*.mjs` | speed against the native build; the micro-measurements below |

The input is the AST of `compiler/lir` after `lir.check` and `check-main` have accepted it: the same reader and checker as `lairf`, so a
module that lair rejects, lir2js rejects with the same diagnostic. The translator does not fail on a checked module: anything the runtime
cannot do becomes a call that stops the program with `lir2js: unsupported: WHAT` (exit status 70) when it is reached.

Build and use (the seed builds it; nothing links LLVM):

```
fibc build compiler/lir2js.fib -I compiler -I lib -o lir2js
lir2js prog.lir -o prog.mjs && node --stack-size=7000 prog.mjs ARGS
lir2js run prog.lir ARGS          # the same, through a temporary file, node exec'd in place of lir2js
```

`--rt DIR` (or `FIB_JS_RT`) names the runtime's directory; the default is `compiler/js/rt` from the working directory.

No tool was downloaded and no npm package installed: everything is plain node (v26.10.0) and its standard library.

## Decisions, with their measurements

The micro-measurements are `compiler/tests/js/bench/micro.mjs` and `bench/mem.mjs` (`node FILE`; node v26.10.0, this machine, one run each
after a warm-up run). The numbers are for choosing between designs, not claims about programs.

### 1. Integers

| Variant (20 M iterations of a 64-bit LCG step and a sum) | ms |
|---|---|
| i64 as BigInt, `BigInt.asIntN(64, ..)` after each op | 226 |
| i64 as a pair of i32 (hi, lo) with 16-bit limbs for the product | 160 |
| the same loop at i32 (`|0`, `Math.imul`), for scale | 17 |
| BigInt counter loop, add only | 628 |
| f64 counter with a 2^53 range check (the "f64 when provably small" variant) | 79 |

**Decision: i64 is a BigInt; i8, i16 and i32 are JS numbers holding the signed value; i1 is 0 or 1.** The pair is only 1.4 times faster
on the multiply-heavy loop and costs two values per i64 everywhere (phis, parameters, struct fields, memory), multi-value returns, and a
hand-written carry for every operation; f64-when-small needs a range analysis lIR does not give (fibber's `+` traps on overflow through
`sadd-overflow`, so the range is not bounded by the program's types). BigInt is exact by construction: wrapping is `BigInt.asIntN(64, x)`
after `+ - * <<`, `& | ^` and `>>` need none on canonical signed values, and every checked-overflow instruction compares the exact result
with the wrapped one (`$sov`, rt/int.js). i32 is `(a+b|0)`, `Math.imul`, `>>>` for logical shifts; i8 and i16 are computed as i32 and cut
back with `<<24>>24`. Unsigned operations and predicates convert at the operation (`>>>0`, `& 255`, `BigInt.asUintN(64, ..)`). Division by
zero, and the minimum divided by -1, raise SIGFPE as x86-64's `idiv` does (lIR leaves both undefined; the native build of a checked fibber
program never reaches them, since fibber checks first). Shift amounts are masked as x86-64 masks them (an amount at or above the width is
poison in lIR).

This is the main cost of the slice (see Speed): fibber's integers are i64, so its loops are BigInt loops. The way out, measured above at
roughly 13 times, is a range analysis that proves an i64 small enough to live in a double, a later step.

**f32**: a float is a JS number that `Math.fround` has rounded after every operation. For `+ - * / sqrt` and `frem` this is exact IEEE
single arithmetic (the double result of two floats, rounded once to float, is the correctly rounded float result, since 53 >= 2*24+2).
Conversions that would round twice are done once: i64 to float folds the low bits into a sticky bit first (`$i2f32`), strtof rounds the
decimal once through BigInt (`$decToF32`). A float NaN keeps its payload and quiet bit (a load widens it by hand, since DataView's
`getFloat32` goes through the hardware conversion, which quiets a signalling NaN): `cases/ownership/194-float-bit-casts.fib` failed on
its four f32 NaN probes until this was done.

### 2. Memory

| Variant (write and read 4 M i32 cells, 20 times) | fixed ArrayBuffer | resizable ArrayBuffer |
|---|---|---|
| `DataView.getInt32/setInt32` | 9.4 ms | 325 ms |
| `Int32Array[p >> 2]` | 10.9 ms | 53 ms |

A 4 GiB `ArrayBuffer` is reserved in 5 ms and costs 45 MB of resident memory (V8 commits pages lazily), under the 16 GB address-space cap.

**Decision: one fixed ArrayBuffer (`FIB_JS_HEAP_MB`, default 4096), read and written through one DataView `$D`.** A resizable buffer
makes every access 6 to 35 times slower, and a buffer that grows by copying invalidates every view, so code holding `$D` across a call that
allocates would read the old buffer: a buffer that never moves avoids both. DataView over typed arrays because lIR allows any alignment
(`(align 1)`), it reads i64 as BigInt directly (`getBigInt64`), and it is as fast on a fixed buffer. A pointer is a JS number, the address;
the byte offset is `address - 4096`, so the first page (null and its small offsets) is a negative offset, which DataView refuses with a
RangeError: the SIGSEGV of a null dereference. The layout is `[4096, +8 MiB)` the lIR stack (allocas: a bump pointer `$SP` saved at entry
and restored at return), then static data (globals, string constants), then the heap. Aggregates and vectors are read and written by the
runtime from type descriptors that carry LLVM's x86-64 layout (`js.jx/size-of`, `field-offsets`; a vector aligned to its size rounded up to a
power of two; `<N x i1>` bit-packed as LLVM stores it), so the byte offsets a fibber program computes (`(i64 16)` passed to `malloc` for a
header) mean the same in both builds. `malloc` is the runtime's own: power-of-two size classes with a 16-byte header and a free list per
class (16-byte aligned, as glibc's); the runtime's `rt/*.lir` allocator is fibber's own counting on top of `malloc`, translated like any
other lIR. Function pointers are numbers from 2^44 up, 16 apart, indexing a table `$FT`; a call through any other number is SIGSEGV.

### 3. Control flow

| Variant (20 M iterations of a loop with an if) | ms |
|---|---|
| structured `for` with `if` | 7.5 |
| `for (;;) switch (L)`, one case per block | 66 |

**Decision, for this slice: the loop-and-switch form** (every function of more than one block is `let L = 0; for (;;) switch (L) {..}`, a
branch being `L = k; continue;`), because it is correct for every CFG lIR allows (irreducible ones included), and simple enough that its
correctness is evident. It is 9 times slower than structured code on the measurement above; a stackifier or relooper is the first
speed step after BigInt (the CFG of fibber's code is reducible). A phi is a variable `ph<pos>` that every edge into its block assigns before
it jumps; edges read only the block's other names, never the phi variables, so the copies of one edge are parallel by construction (a swap
of two phis is right with no temporaries). A function of one block without a self tail call has no loop.

### 4. Calls and tail calls

| Variant | ms |
|---|---|
| 10 M direct recursive calls (depth 5000, 2000 times) | 99 |
| 10 M tail calls through a trampoline (`return $TAIL` and a loop in the caller) | 47 |
| 10 M self tail calls as a loop | 3 |

JS has no guaranteed tail calls (only Safari implements them). **Decision: a tail call to the function itself sets the parameters and
jumps to the entry block** (a loop, free); **any other tail call returns `$TAIL`**, with the callee in `$tf` and the arguments in `$ta`, and
the caller runs the trampoline `$tl`. Only the callers that need it pay: a direct call goes through `$tl` only when the callee has a tail
call that is not to itself or to a C function, and every indirect call does. A non-tail call is a JS call: node's default stack holds
about 18 000 frames of a small function, `--stack-size=7000` (below the 8 MiB the main thread has) about 82 000, so `lir2js run` and the
harness use it; a JS stack overflow (RangeError) ends the program with SIGSEGV, as the native stack's guard page does.

### 5. The C library

Every `declare`d function is a shim in `$X` (rt/libc.js, rt/stdio.js): the malloc family, `memcpy memmove memset memcmp strlen`, `printf
fprintf snprintf sprintf puts fputs putchar fputc fwrite fflush fopen fclose fread ferror`, `write read openat close lseek isatty unlink
mkdir rmdir strerror __errno_location getenv clock_gettime nanosleep sysconf(84) exit abort`, `strtod strtof`, libm (`sqrt floor ceil trunc
fabs fmod` exact; `sin cos exp log pow ..` are JS's, which may differ from glibc's in the last place), `mmap` (anonymous only), and the
single-threaded pthread subset: keys, and mutexes and condition variables that never wait. A declared function with no shim, and
`pthread_create`, `pthread_cond_wait` (it would wait forever), `mprotect(PROT_NONE)` (a guard page would need a test on every access) and
`declare-global` of a name other than `stdin stdout stderr environ`, stop the program with `lir2js: unsupported: ..` when reached. The first
slice is single-threaded: a fibber program that spawns a task is unsupported.

printf is glibc's, byte for byte, including the floating conversions: the digits of `%f %e %g` come from the exact binary value through
BigInt with ties to even (`Number.prototype.toFixed` rounds ties away from zero and prints `0.5` with `%.0f` as `1` where glibc prints `0`),
and the exponent of `%e`/`%g` is decided on the exact value before rounding (the first version took it from `Math.log10` and printed
`9.9999999999999998e+149` as `1e+150`; `cases/lir/simd/float-fns.lir` caught it).

stdio buffers as glibc does, because what a dying program loses is observable: stdout is line-buffered on a terminal and fully buffered
(4096 bytes) otherwise, stderr unbuffered; `exit` and a return from `main` flush, `abort` and `(trap)` flush nothing.

### 6. Vectors and fma

A vector is a JS array of its lanes, each held as a scalar of its element type; every operation maps the scalar one over the lanes
(`$vm1`, `$vm2`, `$vm3`), masks are arrays of 0 and 1, reductions fold in lane order (`reduce-fadd` without `reassoc` adds lane by lane,
as spec/lir.md requires; with it, the same order, which `reassoc` permits). Masked loads and stores and gathers touch only the lanes whose
bit is set. Correct first; no SIMD.js exists any more, and WebAssembly SIMD is a different backend.

`fma` is exact: `Math` has no fma, so `$fma64`/`$fma32` take the exact product and sum in BigInt and round once to nearest, ties to even, at
53 or 24 bits with the subnormal floor (rt/float.js `$roundTo`). This matches LLVM's `llvm.fma`, which is exactly rounded on every target,
bit for bit (rt-test.mjs checks it against a rational reference over random and edge operands, and `cases/lir/simd/fma.lir` passes). It is
slow (a few microseconds), which n-body does not notice because fibber's code does not call it. `fmuladd` is fused here (x86-64 with FMA
fuses it); a module that depends on the difference is target-dependent by spec/lir.md 6.1.

### 7. Traps and exit codes

The native process ends in one of three ways, and the JS one ends the same way: `main`'s status (`process.exit(status & 255)` after
flushing); `exit(n)`; or a signal. `(trap)` is llvm.trap, SIGILL on x86-64; fibber's own traps write their message with `write(2)` and call
`abort`, SIGABRT; a null dereference or a stack overflow is SIGSEGV; an integer division by zero SIGFPE. The runtime raises the same
signal on node itself (`process.kill(process.pid, 'SIGILL')`), so the shell sees the same `128 + N` status and nothing that native code
would not have flushed is written. A JS exception that is none of these is reported as `lir2js: internal error` with status 70, which the
harness counts as a difference.

### 8. Output shape

One ES module per lIR module: the runtime (about 900 lines of JS) and then the module: the shims of the declared functions, the addresses
of the globals, the hoisted constants (type descriptors `T<n>`, strings `S<n>`, function addresses `A_<name>`, float literals `K<n>`), the
global initialisers, one JS function per lIR function (`f_<name>`, locals `v_<name>`, a name's bytes outside `[A-Za-z0-9_]` as `$xx`), and
`$start(f_main, nparams)`. Source maps (lIR line and column are in every node's `pos`) are a later step.

## The differential harness

`compiler/tests/js/diff.sh` runs each program natively (`lairf run`: the JIT of lair, compiler/native) and as JS, and compares standard
output, standard error and the exit status. Inputs: every `;; expect: accept` case under `cases/lir`, and, with `--emit DIR`, the lIR that
`fibc emit` makes for each accepted or trapping fibber program of DIR. Rows are `pass`, `differ` (a bug, in lir2js or in lair: each is
looked into), `unsupported` (the JS run reached `lir2js: unsupported`) and `skip` (the native run timed out, or fibc could not emit).
`--expect compiler/tests/js/expected.txt` makes the run fail when the set of non-passing programs is not exactly the listed one.

Results (RESULTS)

## Speed

(SPEED)

## Not done in this slice

(NOTDONE)
