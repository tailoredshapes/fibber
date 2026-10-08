# A C backend for lIR

**Status: LIR2C-1, the CPU printer** (2026-10-07). `fibc build --emit c` writes a fibber program (the runtime included: a program is one lIR
module) as one C11 translation unit with GNU extensions; `fibc build --via c --cc gcc` compiles it with the C compiler instead of LLVM;
`--via c --static` compiles it to an object and links it against the musl pieces, so no LLVM is anywhere on that path. The owner asked
"We might be able to do lir -> c?" and "we can emit a warning if we can't translate the lir": every lIR construct the printer cannot
translate for the chosen C compiler is reported by name and position, a **warning** where the program keeps its meaning and an **error**
where it would not. Nothing is silently miscompiled, and no output depends on undefined behaviour in C.

The printer is a second code generator of lIR beside `lir2js` (docs/design/js-backend.md), built the same way: the same reader and
checker as `lairf` (`compiler/lir`), one module per lIR section, a differential harness against the native run, planted faults.

| Path | What |
|---|---|
| `compiler/c/cx.fib` | the printer's state, C names (`f_` `x_` `g_` `v_` `L_` `p_` and a hex escape), interned C types, diagnostics |
| `compiler/c/prelude.fib` | the C text every unit starts with: the helpers that give lIR's scalar semantics (section 3) |
| `compiler/c/scalar.fib`, `vec.fib`, `mem.fib`, `call.fib` | scalar operations and literals; vectors; memory and atomics; calls and intrinsics |
| `compiler/c/expr.fib`, `func.fib`, `module.fib` | the statement-oriented expression printer; one function; the translation unit |
| `compiler/c/policy.fib` | the per-compiler facts and the tail-call verdict (section 2) |
| `compiler/lir2c.fib` | the tool: `lir2c FILE.lir -o OUT.c [--cc gcc\|clang] [--allow-plain-tail-calls] [--no-lines]`, `lir2c emit FILE.lir` |
| `compiler/driver/viac.fib` | `--emit c`, `--via c`, the compiler probe, the flags, `FIB_VIA=c fibc run` |
| `compiler/tests/c-backend/diff.sh`, `expected.txt`, `cases/` | the differential harness (section 4), its listed non-passing programs, the backend's own lIR cases |
| `compiler/tests/c-backend/suite.sh` | the case suites through the C route (`FIB_VIA=c fibc cases`), with their expected-failure files |
| `compiler/tests/c-backend/fixed-point.sh`, `ubsan.sh`, `speed.sh` | the fixed point through C; the UB hunt; the speed against LLVM |
| `scripts/mutant-lir2c.sh` | the planted faults (section 4) |
| `scripts/bootstrap-c.sh` | fibber on a machine with only a C compiler (section 6) |

Build and use (the seed builds the tool; nothing of it links LLVM):

```
fibc build compiler/lir2c.fib -I compiler -I lib -o lir2c
lir2c prog.lir -o prog.c --cc gcc && gcc -std=gnu11 -O2 -march=native -ffp-contract=off -fno-math-errno -o prog prog.c -lm -lpthread
fibc build --emit c prog.fib -o prog.c            # the same from a fibber program
fibc build --via c --cc gcc prog.fib -o prog      # written to a temporary file and compiled
FIB_VIA=c FIB_CC=gcc fibc cases cases/ownership   # the case suite judged against the C route
```

## 1. Decisions

- **`--emit c` is an output kind of a target row, like `obj`, `asm` and `llvm`**, not a target of its own: the C is for the row's
  architecture and OS (the `#line`-free text is the same for every row; what differs is the C compiler and its `-march`), so
  `--target aarch64-unknown-linux-gnu --via c --cc aarch64-linux-gnu-gcc` cross-compiles by construction. `fibc targets` is unchanged;
  docs/design/targets.md gains the note.
- **One translation unit.** A fibber program is one lIR module (the runtime's `rt/*.lir` is in it), and lIR's items refer to each
  other in any order, so the C declares everything first (prototypes, tentative definitions of the globals) and defines afterwards.
  Splitting into units would need a header per module and would lose nothing but compile time; the compiler itself (30 MB of lIR,
  21 500 functions) compiles as one unit in the time of section 5.
- **The C compiler is the judge of what it can do, and the printer asks it first.** `fibc build --via c` compiles two lines with the
  chosen compiler before the first use (driver.viac `probe`, cached under `$TMPDIR` by the compiler's name): is `__attribute__((musttail))`
  honoured, and between two prototypes that differ? The table of c.policy (gcc 15: both; clang: only identical prototypes; anything
  else: neither) is the fallback when the compiler cannot be run.
- **A tail call C cannot guarantee is an error** (ADR 0008: a supported target guarantees tail calls), or with
  `--allow-plain-tail-calls` a warning at the call site and a plain call: the program keeps its meaning and risks its stack. The three
  cases are in the table of section 2. A tail call of a function to itself is never in question: it is a `goto` to the entry after a
  parallel assignment of the parameters, on every compiler.
- **Every result is a C local, every operand a name or a literal** (c.expr). lIR fixes the order of evaluation and C does not (function
  arguments, operands of `+`), and `select` evaluates both arms; hoisting every instruction into a local in lIR's order makes the C order
  the lIR order by construction. The C compiler undoes the locals.
- **Integer semantics by helpers, not by flags.** `-fwrapv` would make C's `+` wrap, but lIR's own traps are the overflow intrinsics,
  and `-fwrapv` hides nothing of them; still, the printer does not depend on it: `add sub mul` are computed on the unsigned twin and
  converted back (modulo 2^N on gcc and clang, documented behaviour), shifts mask their amount, division by zero raises SIGFPE. The
  generated C is correct under default flags; the flags of section 2 are for floating point and for warnings.
- **Memory through `may_alias` typedefs**, not `-fno-strict-aliasing`: an lIR pointer is untyped and a block is a string now and doubles
  later, so every typed load and store goes through `typedef T __attribute__((may_alias)) ma_T` (and `aligned(1)` where `(align N)` is
  below the ABI's). Aggregates are moved with `__builtin_memcpy`. The flag is not passed, and `scripts/mutant-lir2c.sh strict-alias`
  removes the attribute to show the harness notices.
- **Pointer arithmetic in `uintptr_t`.** A `getelementptr` may step outside any object (lIR says only `inbounds` may not, and fibber does
  not emit `inbounds`), which C's pointer arithmetic may not; integer arithmetic has no such rule. A struct field is
  `__builtin_offsetof` and an array element `sizeof`: the C compiler computes LLVM's layout (lIR has no packed structs), so no layout
  table is duplicated in the printer.
- **Vectors are GNU vector types** (`vector_size`), so `+ - * & | ^` and the comparisons are the compiler's and vectorise; a `<N x i1>` is
  a vector of bytes holding 0 or 1 (bit-packed in memory, as LLVM stores it), a `<N x ptr>` a vector of 64-bit addresses. What GNU vectors
  lack (division, saturating conversions, select by mask, masked loads and stores, gathers, scatters, reductions in lane order) is a loop
  over the lanes. A lane count that is not a power of two is held in the next power of two (`phys-lanes`), the padding lanes never read.
- **Exported symbols keep their lIR names through asm labels** (`__asm__("name")`, with a `_` prefix on Darwin by `FIB_SYM`), and every
  declared C function is a prototype from the lIR `declare` bound to its symbol the same way (ADR 0011: no header is read; the
  freestanding `<stdint.h>` and `<stddef.h>` are the only includes). A defined, exported function whose name holds a character the
  assembler does not take bare (`-`) keeps its C name instead, with a warning: gcc derives the names of its clones (`.part.0`,
  `.constprop.0`) from the label, and a quoted label with a suffix is not a symbol. A fibber program exports only `main`.
- **`#line` directives** name the lIR line in every diagnostic the C compiler gives, so a "cannot tail-call" from gcc, should the
  printer's estimate miss one, points at the lIR call site (`--no-lines` / `FIB_C_LINES=0` leave them out).
- **`(unreachable)` is `__builtin_trap()`**, as is the end of every function: reaching either is undefined in lIR, and a trap is a valid
  translation of undefined behaviour that is also defined. The same choice everywhere lIR says poison: a shift amount at or above the
  width is masked, an out-of-range `fptosi` saturates, an out-of-range lane index is masked. A program that is defined in lIR is defined
  in C, and a program that is not gets a defined answer rather than whatever the optimiser makes of it.
- **Exceptions and task traps need nothing of C**: a trap is a message and `abort` (or, on a task's thread, the task's failure and
  `pthread_exit`, ADR 0001); `try`/`catch` is the hidden-result convention of docs/design/exceptions.md 9 (ADR 0009), ordinary values
  and branches. There are no landing pads in lIR and no `setjmp` anywhere. The stack-overflow handler (`sigaltstack`, `sigaction`, the
  `siginfo` read by offset) is lIR in `rt/thread.lir` and goes through the printer like everything else.
- **Every plain access is `aligned(1)`.** lIR promises the ABI alignment of a plain `load`/`store` and calls a misaligned one undefined
  (6.12); LLVM executes it on x86-64 and AArch64 and the runtime has such sites (section 4, UBSan), so the printer writes every plain access
  through an `aligned(1)` typedef: the same instruction there, byte accesses where the hardware needs them. Atomics keep their alignment.
- **A function with a `musttail` call is `noinline`.** gcc 15 inlines such a function and then cannot honour the attribute in the inlined
  copy ("cannot tail-call: other reasons"); the call is a jump either way.
- **Allocas are C locals**, one slot per call, except an `alloca` in a block that can reach itself (a loop), which is
  `__builtin_alloca_with_align`, one slot per execution as LLVM gives. Neither fibber's emitter nor the runtime allocas in a loop; the
  rule keeps the printer exact if one day they do.

## 2. The policy: construct, translation, warning or error

The exact messages. `FILE:L:C:` is the lIR position; the C compiler's own diagnostics carry it too (`#line`).

| lIR construct | gcc 15+ | clang 13+ | any other cc |
|---|---|---|---|
| `tailcall`/`indirect-tailcall` to the function itself | `goto` to the entry (a loop) | the same | the same |
| `tailcall` to another function, identical prototype | `__attribute__((musttail)) return g(..)` | the same | **error** `tail call to @g in @f cannot be guaranteed: CC has no __attribute__((musttail)); --allow-plain-tail-calls lowers it to a plain call` |
| `tailcall`, prototypes differ, the callee's stack-passed arguments fit the caller's (the SysV estimate: 6 integer and 8 vector registers, aggregates over 16 bytes in memory) | `musttail` | **error** `..: clang honours musttail only between identical prototypes, and (fn ..) differs from (fn ..); --allow-plain-tail-calls ..` | error as above |
| `tailcall`, the callee needs more stack-passed argument bytes than the caller has | **error** `tail call to @g in @f cannot be guaranteed: the callee needs N bytes of stack-passed arguments and the caller has M (lIR's tailcc has no C equivalent); --allow-plain-tail-calls lowers it to a plain call` | error as the row above | error |
| any of the three errors with `--allow-plain-tail-calls` | **warning** `..; lowered to a plain call (--allow-plain-tail-calls): the stack may grow`, and `return g(..)` | the same | the same |
| `tailcall` between `tailcc` functions needing different stack areas | none: every `tailcc` prototype (definitions, declares, the function types of indirect calls) is padded with `int64_t padK` parameters, passed as 0, to the largest stack-passed-argument area any `tailcc` prototype of the module has (`tail-area`), so no callee needs more than its caller has | the same | (not applicable) |
| `tailcall` whose result the C convention returns through a hidden pointer (a vector over 32 bytes, an aggregate over 16) | **error** `tail call to @g in @f cannot be guaranteed: a result of type <8 x double> is returned through a hidden pointer by the C convention (gcc: "memory reference after call"); --allow-plain-tail-calls ..` | the same | error |
| `<N x T>` with N not a power of two | none: held in the next power of two, the padding lanes never read, loaded and stored lane by lane | the same | the same |
| `define kernelcc`, `(sreg R)`, `(barrier)` (spec/lir.md 6.9a) | **error** `@k is kernelcc: kernel target forms have no C translation: use --kernel-target` (`sreg: ..`, `barrier: ..`) | the same | the same |
| an exported definition whose name is not a plain symbol | **warning** `exported symbol checked-add is f_checked_2dadd in the C: the assembler takes no '-' or other such character in a symbol gcc may clone` | the same | the same |
| every other construct | translated as section 3 says | the same | the same |

The flags `fibc build --via c` passes (driver.viac `cc-flags`), each with its reason:

| Flag | Why |
|---|---|
| `-std=gnu11` | C11 with the GNU extensions the output uses (section 3, "GNU-only") |
| `-O2` (`-O N` of the build) | |
| `-march=native` for the host, `-march=x86-64-v3` for a cross x86-64 target (ADR 0008), nothing for another architecture | the same CPU choice as the LLVM path |
| `-ffp-contract=off` | only an explicit `fma` fuses; `fmuladd` chooses by `__FMA__` |
| `-fno-math-errno` | `sqrt` is an instruction and sets no errno, as `llvm.sqrt` |
| `-fno-delete-null-pointer-checks` | belt and braces: the printer never dereferences a pointer it compares, but a null check after a load is kept either way |
| `-Wall -Wextra` | |
| `-Wno-unused` | lIR binds names it never reads; the runtime defines functions a program never calls |
| `-Wno-array-bounds -Wno-stringop-overflow` | gcc's object-size tracking sees through the `uintptr_t` casts, and lIR addresses the bytes of a global past its declared type (a header followed by its elements): false positives on valid code |
| `-Wno-psabi` | 256-bit vector parameters "change the ABI" since gcc 4.6: every function of the unit is compiled with the same flags |
| `-lm -lpthread`, then the `-L`/rpath and `-l` words of the build (native.link `link-args`) | as the LLVM path links |

No `-fwrapv` (the semantics are in the helpers, section 1), no `-fno-strict-aliasing` (may_alias), no `-ffast-math` ever. With
these flags the output compiles with `-Wall -Wextra` clean on gcc 15.2.0 over the three case suites (the differential harness counts a
warning as a difference).

### `--c-std` (a note, not a flag)

The output is C11 (`-std=gnu11`). GNU-only, and why each is needed: `__attribute__((vector_size))` and `__builtin_shufflevector`,
`__builtin_convertvector` (vectors); `__attribute__((may_alias, aligned(1)))` (untyped memory); `__attribute__((musttail))` (guaranteed
tail calls); `__asm__("symbol")` labels (lIR names); `__builtin_*_overflow`, `__builtin_popcountll`, `__builtin_ctzll`, `__builtin_clzll`,
`__builtin_fma`, `__builtin_isnan`, `__builtin_trap`, `__builtin_alloca_with_align`, `__builtin_offsetof`, `__builtin_memcpy` (no header
for any of them); `__atomic_*` (lIR's orderings one to one); a zero-length array as a struct field (`[0 x T]`); an empty initialiser
`{}` for `{ }`. Computed goto is not used: blocks are labels and `goto` suffices. Thread-locals are not used: the runtime keeps its
per-thread state in pthread keys.

## 3. The translation, and why each construct keeps its meaning

| lIR | C | Why it is the same |
|---|---|---|
| `i1 i8 i16 i32 i64 float double ptr` | `uint8_t` (0 or 1), `intN_t`, `float`, `double`, `char *` | fixed widths; x86-64 and AArch64 are LP64 |
| `<N x T>` | `typedef T __attribute__((vector_size(N*sizeof T))) vN_T` (i1 lanes as `int8_t`, ptr lanes as `int64_t`), with an unsigned twin | the same bytes and alignment (a vector is aligned to its size) |
| `%struct.S`, `{ .. }`, `[N x T]` | `struct S_..` with fields `f0 f1 ..`, interned `struct A<k>`, `struct R<k> { T e[N]; }` | C's natural layout is LLVM's for unpacked structs; `offsetof`/`sizeof` computed by the C compiler |
| `(iK n)`, `(double x)`, `(float x)` | `nLL`, `(int8_t)n`, a hexadecimal float literal (`0x1.8p+1`), `(float)` of it | exact: no decimal conversion between the compilers; the minimum written as `(-9223372036854775807LL - 1)` |
| `(string "..")`, `(ptr null)`, `@g`, `@f` | `static const unsigned char s<k>[] = {..,0}`, `(char *)0`, `(char *)&g_x`, `(char *)f_x` | |
| `add sub mul` | `fib_addN(a, b)`: `(T)((U)a + (U)b)` (i8/i16 via `uint32_t`) | wraps; no signed overflow in C |
| `sdiv srem udiv urem` | `fib_sdivN`: SIGFPE on a zero divisor or the minimum by -1, else `/` `%` | undefined in lIR (6.12); the native x86-64 behaviour |
| `shl lshr ashr` | amount masked to the width; `lshr` on the unsigned twin, `ashr` on the signed value | an amount at or above the width is poison; the mask is x86-64's answer; a signed right shift is arithmetic on gcc and clang (documented) |
| `and or xor` | `&` `\|` `^` | |
| i1 arithmetic | `^` for add/sub/xor, `&` for mul/and, `fib_div1`/`fib_rem1` | arithmetic modulo 2; a shift amount of 1 is poison, so `a` |
| `ctpop cttz ctlz abs smin smax umin umax` | helpers over `__builtin_popcountll` etc.; `cttz(0)` is the width, `abs(min)` is min | never poison, as the spec says |
| `sadd/ssub/smul-overflow` | `__builtin_add_overflow(a, b, &r.f0)`, the flag in `r.f1`; a lane loop on vectors; the i1 form by hand | exact |
| `icmp` | `==` etc. on the signed values, on the unsigned twin for `u*`, on `uintptr_t` for pointers, on `-(int)a` for signed i1 | |
| `fcmp` | the ordered predicates are C's (false on NaN), the unordered ones negate the ordered opposite, `ord`/`uno` by `__builtin_isnan` | IEEE comparisons without fast-math |
| `fadd fsub fmul fdiv` | `+ - * /` | IEEE single and double on SSE (`FLT_EVAL_METHOD` 0); `-ffp-contract=off` keeps them separate |
| `frem` | `__builtin_fmod` | `frem` is `fmod` |
| `fneg` | `-a` | flips the sign bit, NaN included |
| `fma` | `__builtin_fma` | one rounding on every target: an instruction with `-mfma`, else libm's exact `fma` (ADR 0008) |
| `fmuladd` | `__builtin_fma` under `__FMA__`, else `a * b + c` | either is allowed (6.1) |
| `fsqrt fabs ffloor fceil ftrunc fround froundeven fcopysign` | `__builtin_sqrt fabs floor ceil trunc round rint copysign` | `rint` rounds to even in the default mode, which nothing changes; `round` ties away |
| `fmin fmax` / `fminnum fmaxnum` | `fib_fmin64` (NaN propagates, -0.0 < +0.0) / `fib_fminnum64` (the other operand) | the spec's two pairs |
| `trunc zext sext fptrunc fpext sitofp uitofp ptrtoint inttoptr` | C casts, through the unsigned twin for `zext uitofp inttoptr`, `-(int)` for an i1 source | modulo on narrowing (documented), zero-extension where lIR zero-extends |
| `fptosi fptoui fptosi-sat fptoui-sat` | `fib_fptosi_sat..` (NaN to 0, clamped) for all four | out of range is poison for the first two: saturation is a valid, defined answer |
| `bitcast` | a same-size vector cast; `__builtin_memcpy` between scalars and vectors; `<N x i1>` to `iN` packs lane i into bit i | the bytes, as `llvm.bitcast` |
| `select` | `c ? a : b` on names (both arms already evaluated); a lane loop with a mask | |
| `extractelement insertelement shufflevector` | `v[i]` (a constant, or a run-time index masked to the lanes), `r = v; r[i] = x`, `__builtin_shufflevector(a, b, k..)` | |
| `extractvalue insertvalue` | `.fK` / `.e[K]` paths on struct values | |
| `reduce-*` | a loop in lane order (`reduce-fadd` starts at the start value; the integer folds at lane 0) | lane order is what the spec promises without `reassoc`; with it, the same |
| `alloca` | a C local (`T slot[N] __attribute__((aligned(A)))`), or `__builtin_alloca_with_align` in a loop | one slot per call / per execution |
| `load store` | `*(ma_T *)p`, `*(ua_T *)p` under `(align N)` below the ABI's, `volatile` kept, `__builtin_memcpy` for aggregates, bit packing for `<N x i1>` | may_alias; unaligned where lIR allows it |
| `getelementptr` | `(char *)((uintptr_t)p + (uint64_t)(int64_t)i0 * sizeof(T) + __builtin_offsetof(..) + ..)`; a vector index gives `vN_i64` | integer arithmetic is never undefined; indices sign-extend as LLVM's |
| `masked-load masked-store gather scatter` | lane loops that touch only the lanes whose mask is set, through `aligned(1)` lane pointers | a masked-off lane is never accessed |
| `atomic-load atomic-store atomicrmw cmpxchg fence` | `__atomic_load/store/exchange/fetch_OP/compare_exchange`, `__atomic_thread_fence`; `max min umax umin fadd fsub fmax fmin` by a compare-exchange loop; orderings `unordered`→relaxed, `monotonic`→relaxed, `acquire release acq_rel seq_cst` the same; `singlethread` fences are `__atomic_signal_fence` | the C11 model is LLVM's (spec 8) |
| `phi` | a variable `p_<pos>` assigned on every edge into the block, several at once through temporaries | a parallel copy: the lost-copy and swap problems cannot occur |
| `br`, `switch`, `ret`, `unreachable` | `goto`, `switch` on the literal cases with a `goto` each, `return`, `__builtin_trap()` | |
| `call`, `indirect-call` | `f_g(..)`, `((R (*)(T..))p)(..)`; `tailcc` and `ccc` are both the C convention here (one unit, one convention) | arguments already evaluated in lIR's order |
| `tailcall`, `indirect-tailcall` | section 2 | |
| `(trap)` | `__builtin_trap()` | `ud2` on x86-64, SIGILL, as `llvm.trap` |
| `declare`, `declare-global` | `extern R x_f(T..) __asm__("f");`, `extern T g_x __asm__("x");` | the lIR prototype is the C prototype (ADR 0011); variadic with `...` |
| `global`, `constant` | `static T g_x = init;` (`const` for a constant: a store faults as it does from LLVM's rodata); exported ones with their asm label | static data, initialised as lIR writes it |
| `main` | `int32_t main(int32_t argc, char **argv)` with `argv` as a `char *` | C's `main` |
| linkage `private`/`internal`, `hidden` | `static`, `__attribute__((visibility("hidden")))` | |
| `(target ..)` | nothing in the text; the driver's `-march` | |

## 4. Tests

### The differential harness

`compiler/tests/c-backend/diff.sh` runs each program natively (`lairf run`, LLVM's JIT) and through C (`lir2c`, the C compiler with
the flags of section 2, the executable) and compares standard output, standard error and the exit status; a warning from the C compiler
is a difference too. Inputs: every `;; expect: accept` case under `cases/lir`, the JS backend's and this backend's own lIR cases, and with
`--emit DIR` the lIR stage 2 makes of each fibber case of DIR. `--reject` adds the reject cases: lir2c must refuse each with lairf's text.
`--expect compiler/tests/c-backend/expected.txt` fails the run when the non-passing set is not exactly the listed one.

### Results (2026-10-08; gcc 15.2.0, x86-64, this machine)

| Corpus | pass | differ | refused | cc-error | skip |
|---|---|---|---|---|---|
| `cases/lir`, `compiler/tests/js/cases`, `compiler/tests/c-backend/cases`, accept and reject (`--reject`) | all but the listed | 1 (listed) | 0 | 0 | 0 |
| `cases/ownership` and `cases/stdlib` through `fibc emit` (1,584 programs) | 1,561 | 0 | 12 (listed) | 0 | 11 |

Commands: `compiler/tests/c-backend/diff.sh -j 4 --reject --expect compiler/tests/c-backend/expected.txt` ("OK: every ending other than pass is
listed"), and `diff.sh -j 4 --emit cases/ownership --emit cases/stdlib`. The listed differences: `cases/lir/simd/target-host-lacks.lir`
(`lairf run` refuses a module whose `(target ..)` asks for AVX-512 on a host without it; `lair build` and lir2c accept it, as in the JS
backend). The 12 refusals are the tensor cases whose `fib.tensor.vmath.apply-lanes.8` tail-calls functions returning `<8 x double>`: gcc cannot
jump to a callee that returns through a hidden pointer, and the printer says so (row 4 of section 2); `--allow-plain-tail-calls` makes each
a warning and a plain call, and the four tried (7003, 7019, 7078, 7961) pass that way. The skips are the `open-` cases stage 2 does not
compile. **No difference was a bug found in lair or the native path.**

Bugs of the printer found on the way, each fixed and now pinned by a case: `lshr` of the saturating maximum computed on the signed value
(`cases/lir/simd/convert.lir`); a `let` in operand position dropped its statements (`cases/lir/audit/cmp-aot.lir`); a symbol with a `-`
in an asm label (`compiler/tests/js/cases/overflow-trap.lir`); pointer-vector lanes; C's rule that a tail call needs a callee whose
stack-passed arguments fit the caller's, which sent every `tailcc` prototype to a common padded stack area; a result returned in memory;
gcc inlining a function whose tail call is `musttail`; a lane count that is not a power of two (padding, `phys-lanes`).

### The case suites through the C route

`FIB_VIA=c FIB_CC=gcc F cases DIR` judges every case of a directory by its header against the C route: `run` builds the program through
C (driver.viac) and runs it with `FIB_TRACE=1`, so the audit traces (`;; audit:`, `leaks:`) are the executable's own, as the static
stage's are. `compiler/tests/c-backend/suite.sh` runs the three directories and compares the non-passing set with
`compiler/tests/c-backend/expected-gcc.txt` (and `expected-clang.txt`).

Through gcc 15.2.0 (`compiler/tests/c-backend/suite.sh --fibc F --cc gcc -j 4`):

| Directory | Cases | pass | fail | open | time |
|---|---|---|---|---|---|
| `cases/ownership` | 372 | 372 | 0 | 0 | 53 s |
| `cases/modules` | 32 | 32 | 0 | 0 | 4 s |
| `cases/stdlib` | 1,426 | 1,395 | 13 | 18 | 434 s |

"the non-passing set is exactly `expected-gcc.txt`": the 13 are `1707` (fails natively too, `scripts/ci-stage2.expected`) and the 12 tensor
cases above, each with its reason in the file. The audit cases pass: the traces are the executable's own. **Not run through clang**: clang
is not installed on this machine and ADR 0020 forbids a quiet download; the clang rows of section 2 (musttail only between identical
prototypes) are from its documentation, `expected-clang.txt` does not exist, and the printer's padding of `tailcc` prototypes equalises
only the stack area, not the parameter lists, so on clang every tail call between differing prototypes is an error until
`--allow-plain-tail-calls` (or a later universal-prototype step). Run `suite.sh --cc clang` where clang exists and read what it refuses. The `cases/stdlib` count of the merged tree is 1,454 (the floor); the table is the count
at the time of the run, before the merge.

### The fixed point through C

`compiler/tests/c-backend/fixed-point.sh`: `F build --via c --cc gcc compiler/fibc.fib -o F_c`, then `F_c emit compiler/fibc.fib` must
equal `F emit compiler/fibc.fib`, the LLVM-built F's. The strongest test of the printer: the compiler is 30 MB of lIR, every construct
fibber emits, compiled by gcc and then compiling itself.

```
F_c built through gcc -O2 in 113 s: 77996252 bytes of C, 1676108 lines; F_c is 7947968 bytes (F: 10252024)
F emit: 22 s, 31462597 bytes; F_c emit: 27 s, 31462597 bytes
OK: F_c emit compiler/fibc.fib == F emit compiler/fibc.fib (a58cf90099d52536)
```

The compiler built through gcc emits the same 31,462,597 bytes of lIR as the LLVM-built one. (F_c is smaller than F: it is built by the C
compiler at `-O2` from C with `-march=native`; F is LLVM's.) Not done: the same with clang (not installed).

### The UB hunt

`compiler/tests/c-backend/ubsan.sh`: the C of each stdlib case compiled with `-fsanitize=undefined,address` (gcc's sanitizers; clang is
not on this machine) and run; the bar is zero reports.

`ubsan.sh` over every `cases/ownership` case and every sixth `cases/stdlib` case (526 programs): **clean 523, report 0, differ 0, skip 3**
(two `open-` cases fibc cannot emit, and `8354`, which faults on purpose to reach the runtime's own SIGSEGV handler). The first run reported 13
programs, all one cause: **misaligned plain accesses**, which lIR (6.12) calls undefined and LLVM executes on x86-64 and AArch64:
`rt/str.lir`'s UTF-8 scan loads an `i64` at any byte offset (`(load i64 (getelementptr i8 p i))`, no `(align 1)`), and the emitter stores an
`i64` into a `[8 x i8]` field of a task at offset 4 mod 8 (`store (i64 0) t3`, cases 32, 176, 269). These are findings for the owners of
`rt/str.lir` and `emit.lower`; the printer's answer is `aligned(1)` on every plain access (c.mem `unaligned?`), a defined translation that
costs nothing on those targets. The two `AddressSanitizer failed to allocate` lines of the OOM cases (195, 922) are the cases' own purpose.
Leaks are off (the audit counts them on purpose). `-Wall -Wextra` is clean over the suites with the flags of section 2 (the harness
counts a gcc warning as a difference).

### The planted faults

`scripts/mutant-lir2c.sh` deforms a copy of the backend one way at a time, rebuilds lir2c from it and runs the targeted cases through the
differential harness (or a textual check of the C):

| Fault | Killed by |
|---|---|
| `wrap-trap`: the overflow flag always 0 | `c-backend/cases/arith-edges.lir`, `js/cases/overflow-trap.lir` |
| `signed-shift`: `lshr` shifts the signed value | `arith-edges.lir` |
| `strict-alias`: the typedefs lose `may_alias` | `alias-align.lir` (a loop and two pointers of one value stored and loaded as different types) |
| `musttail`: the attribute dropped (judged at -O0; at -O2 gcc's own sibling-call pass rescues these programs) | `audit/t-cf.lir`, `audit/t-tail-many-tailcc.lir`, `tail-lanes.lir` |
| `atomic-order`: `seq_cst` written relaxed | a textual check of the C of `instr/atomic.lir` (a weaker order is not observable on x86) |
| `lane-order`: `shufflevector` operands swapped | `tail-lanes.lir` |
| `phi-copy`: phis assigned one by one, and read back through the phi variable | `js/cases/branches.lir` (two phis that swap; fib by a pair swap) |
| `fallthrough`: an unconditional `br` emits no `goto` | `fallthrough.lir` |
| `align`: `aligned(1)` dropped | `alias-align.lir` |

"killed" nine of nine, after two rounds: `strict-alias` and `musttail` first survived (the first tests did not make gcc exploit the
fault), and `phi-copy` (the printer's edges read the let-bound copies, so the copy cannot be dropped alone); each was sharpened until it died.

The backend's own cases (`compiler/tests/c-backend/cases/`) pin the semantics the planted faults attack: `arith-edges.lir` (wrapping,
shifts, division, the counts, the overflow intrinsics, saturation, fmin/fmax, fma, bitcasts, masks), `alias-align.lir` (type punning,
unaligned accesses, aggregates, a packed mask), `tail-lanes.lir` (tail calls across prototypes 5 million deep, through a closure, to
itself; lane order; argument order), `fallthrough.lir` (blocks out of order), `six-lanes.lir` (refused by name and position). The
stdlib cases 8600 to 8639 are fibber programs with the same edges (section 4.1 of the report), run natively by the gate and through C by
`suite.sh`.

## 5. Speed

`compiler/tests/c-backend/speed.sh --fibc F --cc gcc -n 3` (median of 3, wall seconds, `/tmp/fibsuite.lock` held, native = LLVM `-O2` with the
host CPU, C = gcc 15.2.0 `-O2 -march=native`; the outputs are identical in every row):

| Program | native s | C s | C / native |
|---|---|---|---|
| lazy-bound | 0.04 | 0.08 | 2.00 |
| lazy-fused | 0.90 | 1.87 | 2.07 |
| map-assoc-get | 0.36 | 0.41 | 1.13 |
| num-f64 | 1.85 | 1.89 | 1.02 |
| num-nbody | 0.74 | 1.00 | 1.35 |
| set-conj | 0.36 | 0.35 | 0.97 |
| strings | 0.79 | 0.69 | 0.87 |
| vec-conj-pop | 1.08 | 1.38 | 1.27 |
| vec-index | 0.44 | 0.66 | 1.50 |
| vec-sort | 0.81 | 0.88 | 1.08 |
| n-body (shootout, 500 000) | 0.07 | 0.08 | 1.14 |
| binary-trees (16) | 0.17 | 0.16 | 0.94 |
| fannkuch-redux (10) | 0.16 | 0.17 | 1.06 |
| mandelbrot (2000) | 0.20 | 0.19 | 0.95 |
| fasta (1 000 000) | 0.10 | 0.10 | 1.00 |

Within 10% on 8 of 15 programs, and ahead of LLVM on 4 of those; 13% and 14% behind on two (`map-assoc-get`, `n-body`), and 27% to 2.1 times
behind on five (`vec-conj-pop`, `num-nbody`, `vec-index`, `lazy-bound`, `lazy-fused`). The owner's hope (within 10%) is met by about half. The short runs (0.04 to 0.2 s) are noisy. No profile was taken of the slow rows; the likely causes are
gcc not inlining across the many small `tailcc` functions the way LLVM does (each musttail function is `noinline`, c.func) and the
`aligned(1)` accesses being opaque to gcc's alias analysis. Not measured: clang. Build time: the compiler itself takes gcc 113 s at `-O2`
for 78 MB of C.

## 6. Bootstrap with only a C compiler

`scripts/bootstrap-c.sh` builds fibber on a machine that has a C compiler and nothing else: given `fibc.c` (the compiler's own C,
`F build --emit c compiler/fibc.fib -o fibc.c`), it compiles it with `cc` into a compiler that has no LLVM in it... except that
`compiler/fibc.fib` links LLVM for its JIT (macros) and its `build`: the C of the compiler still calls `LLVM*` functions, so the bootstrap
compiler needs the LLVM library at link time. What the C route gives today is a compiler built without an LLVM *compiler* (no clang, no
seed binary), from a C file and the LLVM shared library.

`scripts/bootstrap-c.sh fibc.c OUT` in a `ubuntu:25.10` container with `gcc 15.2.0`, `libc6-dev` and `libllvm21` installed (the LLVM shared
library; no clang, no fibber, `which clang fibc` empty), the tree mounted read-only (an Alpine or `debian:slim` image has no LLVM 21 library
in its repositories, and building one is another package):

```
clang: none; fibc: none; gcc (Ubuntu 15.2.0-4ubuntu4) 15.2.0
bootstrap: cc cc (Ubuntu 15.2.0-4ubuntu4) 15.2.0; LLVM library LLVM-21 in /usr/lib/x86_64-linux-gnu
bootstrap: fibc.c (77996252 bytes) compiled in 89 s: /out/work/fibc-c (7931696 bytes)
fibc 0.1.12
bootstrap: fibc-c built compiler/fibc.fib (LLVM, stage 2 of this tree) in 108 s: /out/work/F3
bootstrap: OK, the fixed point holds: fibc-c and F3 emit the same lIR for compiler/fibc.fib (31462597 bytes, 46 s for both emits)
```

`fibc.c` is 78 MB (1.68 million lines; 10 MB compressed): too large to check in, so it is a release artifact
(`fibc build --emit c compiler/fibc.fib -o fibc.c`), not a file of the tree.

## 7. Static via C

`fibc build --via c --static prog.fib -o prog`: the C is compiled to an object with `cc -c` and linked by `ld -static -nostdlib` against
the musl pieces of docs/design/static-linking.md (native.linkstatic `link-static-object`): no LLVM anywhere on the path, and no libc of
the build machine is read (the generated C includes only the freestanding headers).

```
$ fibc --via c --cc gcc --static build hello.fib -o hello-static     (FIB_MUSL_DIR = the musl pieces of scripts/build-musl.sh)
hello-static: ELF 64-bit LSB executable, x86-64, statically linked        83,200 bytes
$ ldd hello-static            not a dynamic executable
$ docker run --rm -v hello-static:/hello:ro <empty image> /hello           hello  (exit 0)
```

The image is `docker import` of an empty tar: nothing in it but the file. Cross: `F2 --target aarch64-unknown-linux-gnu --via c --cc
aarch64-linux-gnu-gcc build hello.fib` makes an aarch64 PIE that prints `hello` under `qemu-aarch64`.

## 8. Not in this package

- **The kernel dialect**: `kernelcc`, `sreg`, `barrier` are errors here; Metal Shading Language and OpenCL C are a second printer over
  the same modules (next).
- **C++ or C89 output**: GNU C11 only. The GNU-only list of section 2 is what a C89 or MSVC port would have to replace.
- **Windows MSVC**: no `musttail`, no `vector_size`, no `__atomic_*`: every tail call would be an error. Clang on Windows would do.
- **Cross-compiling via C** works by construction with a cross `cc` (`--target T --via c --cc T-gcc`); `aarch64-linux-gnu-gcc` is on this
  box and was tried on hello (section 7).
- **A universal prototype for `tailcc`** (every tailcc function taking the same C signature, so clang's musttail and gcc's stack-slot
  rule never bite): not built; the rows of section 2 say what happens instead.
- **Clang** is not installed on this machine; its rows in section 2 come from its documentation and the probe is what decides at run
  time. The suites were not run through clang here.
