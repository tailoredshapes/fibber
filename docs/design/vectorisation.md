# Why our loops do not vectorise, and what would make them

SIMD and tensors work, package V. Status: measurements and proposals; no compiler source was changed. Everything quoted
here was produced on this machine (Intel i7-14700KF, AVX2 and FMA, no AVX-512; LLVM 21 `opt`/`llc`; the v0.1.5 seed
`fibc`), under `/tmp/fibsuite.lock`; raw output is in `docs/design/vectorisation/results.log` and the `remarks-*.txt`
beside it, the scripts that made it are in the same directory (they `cd` into the scratch directory they were written
in; edit the first lines before reusing them).

## Summary

1. The pipeline is already `default<O2>` over the host CPU's name and feature string
   (`compiler/llvm/passes.fib:14`, `compiler/llvm/target.fib:102-103`). `default<O3>` changes nothing measurable
   (table 2), and the CPU is not the problem on the host (it is on a `x86-64-v2` release build: half the width).
2. No loop vectorises, for reasons that are different per loop and that stack. Removing one blocker at a time moves
   almost nothing except for pure reductions; the in-place loops need three things at once.
3. Measured ceilings, the blockers removed by editing the emitted IR (not by a compiler change): `sum`/`dot` over
   `(Array f64)` 5.9x and 2.8x from one flag (`reassoc`); `array-set!` loops (`y[i] = y[i] + k*x[i]`, `y[i] = x[i]*x[i]`)
   8.1x and 3.7x from three emitter facts (no count pair around the cell read, uniqueness carried instead of re-tested,
   type-based alias tags); the i64 variants 2.8x from the overflow trap alone, and no loop with a trapping `+` over data
   can vectorise without a different formulation (section 5).
4. Bounds checks are NOT the blocker people expect. For a loop bounded by an array length LLVM already hoists the check
   in front of the loop (section 4, `dot-f64`); removing it alone changes nothing.
5. mandelbrot (data-dependent early exit) and n-body (copy-on-write copies per call, three-lane straight-line code) are
   out of reach of auto-vectorisation; an AVX2 version of mandelbrot written in C runs 3.3x faster than the scalar
   one (650 ms to 200 ms at N=4000), which is the case for the explicit SIMD types of package D.
6. Ranked proposals (section 7): opt-in FP reassociation; elide the retain/release pair around a cell read that feeds
   `array-get` (needs an owner decision: spec/types.md 6.12 says cell reads are not elided); carry a known-unique bit
   and tag memory accesses with struct-path TBAA; run the default pipeline's loop passes twice; devirtualise literal
   closures; range facts for index arithmetic.

## 1. What runs today

* `compiler/native/passes.fib` calls `lp/optimize` (`compiler/llvm/passes.fib:9-18`): `LLVMRunPasses` with the string
  `default<O` + level + `>` and the target machine; level 0 runs nothing. `fibc build` uses level 2
  (`compiler/driver/args.fib:48`, `build-opt-level`), `fibc run` level 0 (`run-opt-level`).
* The target machine is the host's CPU name and feature string, or the CPU named by `FIB_TARGET_CPU` with no extra
  features (`compiler/llvm/target.fib:8-10, 62-65, 90-108`; `compiler/native/target.fib:6`). So `x86-64-v2` for a release
  gives SSE4.2, not AVX2.
* The lowering emits no function attributes except `noinline` on the slow paths (`compiler/native/lower.fib:93-103`), no
  `nsw`/`nuw` (spec/lir.md 6.1: "No `nsw`, `nuw` or `exact` flags exist"; spec/types.md 8.12), no fast-math flags, no
  `noalias`, no TBAA, no `!range`. lIR has none of these (spec/lir.md grep: no `noalias`, no fast-math).
* Every `+ - *` on integers is `llvm.s{add,sub,mul}.with.overflow` and a branch to `fib.trap-c`
  (`compiler/emit/lower/arith.fib:81-85`, spec/types.md 8.12); every `array-get`/`array-set!` is an `icmp ult` against the
  length and a branch to `fib.trap-index` (`compiler/emit/lower/builtins.fib:174-185`, `array-index`).

## 2. The experiments

Method. Kernels I wrote (`kern.fib`: `sum`, `dot`, `axpy` over `(Array f64)` and `(Array i64)`, `map-sq` writing
`o[i] = x[i]*x[i]` through a cell, `map-into` calling a function argument) were emitted with `fibc emit`, turned into
LLVM IR with `lairf emit-llvm`, optimised with `opt -passes='default<O2>' -mcpu=raptorlake -pass-remarks-output=`
(the remark text below is from the YAML) and linked. `strip.py` rewrites the unoptimised IR before `opt`:

| flag | rewrite | sound? |
|---|---|---|
| `ovf` | every `*.with.overflow` becomes a wrapping op, the flag `false` | no (experiment only) |
| `bounds` | the `icmp ult i, len` before a `br .., %oobN` becomes `true` | no |
| `rc` | delete `call @fib.retain/@fib.release` in the kernel functions | no (leaks when objects die inside) |
| `uniq` | `fib.unique?` becomes `true` | no |
| `noalias` | `noalias` on every pointer parameter | argued in 6 |
| `reassoc` | `reassoc` on `fadd fmul fsub` | opt-in only |
| `uflag` | an `alloca i1` per `array-set!` site, false at entry, tested before the call to `fib.unique?`, set true at the merge after the write | yes, see 6 |
| `tbaa` | `!tbaa` tags on loads and stores whose address is a `getelementptr` of `%struct.fib.array.T` field 3 (length), 4 (elements, one tag per `T`) or `%struct.fib.cell.*` field 3 | argued in 6 |

Timings: n = 100000 elements, 2000 repetitions (2e8 elements), best of 3, ms, on the isolated kernel function
(`noinline`). Baseline = the compiler's IR through `default<O2>`; it agrees with the real build (`kern sumf` 80 ms,
`axpyf` 283, `sq` 109 against 83, 275, 118 here).

### Baseline remarks (exact text, `remarks-kernels.txt`)

```
CantReorderFPOps f.sum-f64 | loop not vectorized: cannot prove it is safe to reorder floating-point operations
NonReductionValueUsedOutsideLoop f.sum-i64 | loop not vectorized: value that could not be identified as reduction is used outside the loop
PotentiallyFaultingEarlyExitLoop f.sum-i64 | loop not vectorized: Cannot vectorize potentially faulting early exit loop
CantReorderFPOps f.dot-f64 | loop not vectorized: cannot prove it is safe to reorder floating-point operations
TooManyUncountableEarlyExits f.dot-i64 | loop not vectorized: Cannot vectorize early exit loop with more than one early exit
NoCFGForSelect f.axpy-f64 | loop not vectorized: Control flow cannot be substituted for a select
CantVectorizeLibcall f.axpy-f64 | loop not vectorized: call instruction cannot be vectorized
TooManyUncountableEarlyExits f.axpy-f64 | loop not vectorized: Cannot vectorize early exit loop with more than one early exit
NonSimpleLoad f.axpy-f64 | loop not vectorized: read with atomic ordering or volatile read
```

(`f.axpy-i64`, `f.map-sq`, `f.map-into` give the same four; `f.map-sq` adds `NonReductionValueUsedOutsideLoop`.)
Reading them: the sum and dot loops have exactly one blocker each, FP order (f64) or the trapping `+` (i64). The store
loops have the `NonSimpleLoad` (the atomic load of the count word that `fib.retain`/`fib.release`/`fib.unique?` inline),
the `Libcall` (`fib.array-slice`, `fib.release-slow`, `fib.trap-index` calls inside the loop) and the two exits.

### Table 1: one blocker at a time, ms (baseline in the first row)

| variant | sumf | sumi | dotf | doti | axpyf | axpyi | map-sq |
|---|---|---|---|---|---|---|---|
| baseline | 83 | 42 | 82 | 80 | 275 | 294 | 118 |
| `reassoc` | **14** | 43 | **29** | 81 | 282 | 290 | 111 |
| `ovf` | 79 | **15** | 81 | **37** | 273 | 246 | 110 |
| `bounds` | 79 | 44 | 77 | 77 | 226 | 239 | 84 |
| `rc` | 77 | 41 | 80 | 82 | 145 | 154 | 109 |
| `uniq` | 78 | 39 | 79 | 80 | 218 | 253 | 71 |
| `noalias` | 79 | 39 | 77 | 79 | 236 | 267 | 96 |
| `tbaa` | | | | | 268 | 282 | 112 |
| `ovf bounds rc uniq noalias reassoc` | 14 | 14 | 30 | 35 | 31 | 35 | 32 |
| the same without `reassoc` | 78 | 13 | 78 | 34 | 33 | 34 | 28 |

Vectorised instruction counts (packed FP instructions in the function, `exp.sh`): `sum-f64` 0 to 10 and `dot-f64`
0 to 15 with `reassoc` (width 4, interleave 4: `Vectorized f.sum-f64 | vectorized loop (vectorization width: 4,
interleaved count: 4)`); `sum-i64` 0 to 12 and `dot-i64` 0 to 22 with `ovf`; `axpy-f64` 0 to 10 and `map-sq` 0 to 5 with
the combination. Nothing vectorises with `bounds`, `rc`, `uniq` or `noalias` alone.

### Table 2: the in-place loops, which blockers together

| variant | axpyf | axpyi | map-sq |
|---|---|---|---|
| `rc uniq` | 89 | 102 | 77 |
| `bounds rc uniq` | 87 | 91 | 54 |
| `bounds rc uniq noalias` (vectorised) | 32 | 64 | 35 |
| leave out `bounds` from that | 85 | 90 | 60 |
| leave out `rc` | 180 | 183 | 29 |
| leave out `uniq` | 98 | 105 | 71 |
| leave out `noalias` | 79 | 83 | 62 |
| `tbaa uniq` | 219 | 238 | 32 |
| `tbaa rc uniq` (no `noalias`, no `bounds`) | **30** | 79 | 31 |
| `uflag` | 273 | 284 | 107 |
| `uflag tbaa` | 249 | 257 | 89 |
| `uflag tbaa rc` | **32** | 120 | 88 |
| same, then `default<O2>` a second time | 34 | 93 | **40** (30 for `uflag tbaa`) |

Findings. (i) TBAA replaces `noalias`: `tbaa rc uniq` is as fast as the `noalias` version. (ii) `uflag` is the sound
form of `uniq`: the real `fib.unique?` stays, the first write tests it, and LLVM peels one iteration and drops the test
from the rest (the IR of `u_uflag_tbaa.ll`'s `f.map-sq` shows `inb14.peel`, `chk1.peel`). (iii) For `map-sq` the peeled
loop was not vectorised after one `default<O2>`: the remark is `CantIdentifyArrayBounds f.map-sq | loop not vectorized:
cannot identify array bounds` because peeling runs after the LICM that would hoist the cell-content load, so the
destination pointer is not loop invariant; a second run of the pipeline hoists it and the loop vectorises
(`Vectorized f.map-sq | vectorized loop (vectorization width: 4, interleaved count: 4)`). In a fully inlined module (the
shape real programs have) one pass sufficed: `uflag tbaa rc` on `kern.ll` gives `sq` 32 ms (baseline 107), so the second
pass is needed only for some inlining shapes. Compile time of a second pass: 0.13 s once, 0.21 s twice on the 6161-line
module (`ctime.sh` in `results`, three runs each). (iv) `axpyi` stays at 93 to 120 ms because its `k*x[i]` and `+` carry
two overflow traps; with `bounds` removed as well it is 63.

### CPU and level

| `-mcpu` for the all-removed variant | sumf | sumi | dotf | doti | axpyf | axpyi | sq |
|---|---|---|---|---|---|---|---|
| `x86-64` (SSE2, width 2) | 22 | 21 | 40 | 67 | 40 | 42 | 38 |
| `x86-64-v2` | 25 | 21 | 42 | 71 | 43 | 41 | 33 |
| `x86-64-v3` (AVX2) | 13 | 13 | 29 | 34 | 30 | 37 | 30 |
| `raptorlake` | 13 | 13 | 28 | 35 | 31 | 34 | 26 |

`default<O3>` over the baseline: 76, 39, 77, 80, 275, 283, 107 (no change); over the all-removed variant: 14, 14, 32, 35,
31, 40, 28 (no change). A release built for `x86-64-v2` runs at half width (and a user program built on the host is not
affected). Multiversioning is the answer for a prebuilt binary; it is package D's.

## 3. The shootout kernels

Remark files: `remarks-mandelbrot.txt`, `remarks-spectral-norm.txt`, `remarks-n-body.txt`, `remarks-fannkuch-redux.txt`.
All the helper functions are inlined into `main` by the time the vectoriser runs, so the remarks of the hot loops are
under `main`. Packed/scalar FP instruction counts in the binary reproduce the lead's: mandelbrot 0/25, spectral-norm
0/65, n-body 10/64 (`results.log`, `-- ... packed/scalar` lines).

* **mandelbrot.** The hot loop is `inside?`: `(or (>= i MAXIT) (> (+ tr ti) 4.0))` is a data-dependent exit.
  Remarks: `UnsupportedUncountableLoop main | loop not vectorized: Cannot vectorize uncountable loop` (5),
  `CantVectorizeLibcall` (6), `TooManyUncountableEarlyExits` (1). The pixels of a byte have different trip counts, so
  this is outside any auto-vectoriser. With `ovf`, `bounds`, `reassoc` removed it is still 0 vectorised loops and the
  time does not move (160 to 180 ms at N=2000). An AVX2 C version (`mb_avx.c`: two 4-lane vectors per byte, sticky
  escape mask, same arithmetic order, output md5 equal to the scalar C) takes 200 ms at N=4000 against 650 ms
  (fibber) and 670 ms (scalar C `-O2`): 3.3x. This needs explicit SIMD.
* **spectral-norm** (N=4000, 720 ms). Inner loop of `mul-av`: `CantVectorizeLibcall f.mul-atav` and
  `RecurrencesInEarlyExitLoop f.mul-atav | loop not vectorized: Cannot vectorize early exit loop with reductions or
  recurrences`: the exits are the overflow traps of `(i+j)*(i+j+1)/2 + i + 1` (three) and a bounds check, and the sum is
  an FP reduction. `ovf` alone 510 ms (1.41x); `ovf reassoc` 470 (1.53x, 3 loops vectorised); adding `bounds` changes
  nothing; `uflag tbaa rc` 670. The ceiling is low because the loop is bound by `sitofp` and `fdiv` of a scalar-computed
  index, and an i64 multiply has no AVX2 instruction.
* **n-body** (2e6 steps, 470 ms). The only loop remark is `RecurrencesInEarlyExitLoop f.energy`; the work is straight-line.
  SLP vectorisation fires on the position update only as 2-wide (`xmm`): its remarks are `NotPossible: Cannot SLP
  vectorize list: vectorization was impossible with available vectorization factors` (30) and `NotBeneficial: List
  vectorization was possible but not beneficial with cost 0 >= 0` (12). The larger finding is not vectorisation: with
  `uniq` removed (every write in place) the run drops from 470 to 230 ms because `pair &a i j dt` copies the
  35-element array on its first write: under gdb `fib.array-slice` is hit 10000 times for 1000 steps (ten pairs per step).
  That is spec/types.md 6.6 (copy-in always acquires), not a codegen problem; the in-place-update work owns it.
  (The `rc` variant is slower, 980 ms, because stripping `release` leaks the `Six` structs: it is not a result.)
* **fannkuch-redux** (N=10, 370 ms; all loops are data-dependent array shuffles, no vectorisation possible or
  wanted). Scalar speed-ups from the same blockers: `rc` 190 ms (1.9x), `uniq` 300, `ovf` 350, `bounds` 350, all
  together 140 (2.6x). The retain/release pair around the `@p` cell read that feeds each `array-get` is the biggest cost.
* **`(map f arr)`-style loop with a function argument** (`map-into`): `CantVectorizeLibcall` (the indirect call). Even
  with the other blockers removed and the loop inlined into its caller the call stays indirect: the closure is an
  `alloca` whose address is passed to the callee as its environment, so its code pointer is reloaded from memory
  (`call tailcc double %t20.i190.i(ptr nonnull %t136.i, ...)` in `o/i_uflag_tbaa_rc.ll`); 291 ms baseline, 258 ms with
  everything removable removed, 201 to 235 in the isolated function. Needs the closure devirtualised before LLVM.

## 4. Blockers, classified

| blocker | where it comes from | measured effect | LLVM can remove it? |
|---|---|---|---|
| FP reduction order | `fadd` without `reassoc` (`CantReorderFPOps`) | sum 5.9x, dot 2.8x | no: it changes results |
| overflow trap on data (`sum-i64`, `dot-i64`) | `arith.fib:81-85` | 2.8x, 2.2x | no: a call on the exit path |
| overflow trap on index arithmetic | same | spectral-norm 1.41x | only with range facts it lacks |
| loop counter overflow | same | none: LLVM proves `add nuw nsw` itself | yes (seen in every optimised loop) |
| bounds check, length loop-invariant, loop reads only | `builtins.fib:174-185` | none alone | yes: `f.dot-f64` after `reassoc` has the check in the preheader (`%.not.not = icmp ugt i64 %t28, %0; br i1 %.not.not, label %iter.check, label %oob31`) and a vector body without it |
| bounds check guarding a store | same | 118 to 84 alone (`sq`) | only after peeling and a second pass (table 2) |
| retain/release around `@cell` read | `array-get` on `@y` is a cell read, spec/types.md 6.3, 6.12 | axpy 1.9x, fannkuch 1.9x alone | no |
| `fib.unique?` + copy path per `array-set!` | `builtins.fib:193-` (`array-set`) | 1.2x to 1.7x alone; needed for vectorising | no: the copy path changes the array |
| no type-based alias info; cell content pointer and lengths reloaded after every store | no TBAA in lowering | needed with the two above | with TBAA or `noalias` |
| indirect call through a closure | stack closure whose address escapes to the callee | map with a lambda stays scalar | no |
| early exit on data (mandelbrot) | the program | 0 | no |
| i64 index width | not a blocker for contiguous accesses (vector loads with a 64-bit induction are generated) | not measured as a cost | n/a |
| alignment | elements start at byte 24 of the object (`%struct.fib.array.double = { i64, i32, i32, i64, [0 x double] }`), 8-aligned; LLVM emits unaligned vector loads | not measured | n/a |

## 5. The overflow trap on i64 data

`(+ s (array-get a i))` traps at the first prefix sum that leaves i64 (spec/types.md 8.12). A vector reduction forms
different partial sums per lane, so the per-lane wrap or overflow test is neither equivalent to the sequential test
(lane partial sums can overflow where no prefix does, and the reverse) nor free of false negatives if only the final sum
is tested. Deferring the trap is also unsound for the trap message when two kinds of trap are possible in one loop.
So there is no cheap sound vector form of an i64 sum, and I do not propose one for v1. Two sound options exist for later:
(a) a block scheme: for blocks of 2^k elements test with a vector OR of `x ^ (x >> 63)` that every `|x| < 2^(62-k)`, and
accumulate the block unchecked, falling back to the scalar loop for blocks that fail the test; (b) a library `sum` on
`i32` or narrower integers widened to `i64` in the accumulator, which cannot overflow within 2^32 elements. Neither is
measured here. For index arithmetic (spectral-norm) a precheck of the loop bound `n` (loop versioning: if `n < 2^31`
run the loop with plain adds, else the checked loop) is exact and cheap.

## 6. Soundness arguments for the proposals

* **Known-unique bit.** After `(array-set! &c i x)` the content of cell `c` has count 1 held by `c`: in-place path it was
  already unique; copy path stores a fresh copy and releases the old (`compiler/emit/lower/builtins.fib:193-240`,
  spec/types.md 8.3 via `array-set`'s comment). It stays 1 until something acquires it. A flow-sensitive set of cells
  known unique, cleared at any call that receives the cell, any `set!` of it, any copy-in, any `@c` whose result
  escapes the step, and any loop whose back edge does not preserve it, is therefore exact, needs no alias analysis
  (types.md's organising principle, line 14) and leaves the check in front of the first write. The `uflag` experiment
  is this transformation with the clearing rules vacuous (the loops contain no calls).
* **TBAA.** Tag a load or store by the struct and field of the `getelementptr` that produced its address (struct-path
  tags: `fib.array.T` field 3 length, `fib.array.T` field 4 elements per `T`, `fib.cell.T` field 3, the header words
  as their own classes). It is sound as long as no object is accessed through two different tags at overlapping bytes.
  fibber accesses array elements only through the typed `getelementptr` of the monomorphic array struct; the
  header words are accessed by the runtime through untagged byte offsets (untagged accesses alias everything, so
  they stay correct, they just are not optimised). Anything that type-puns through `ffi/bytes` or `unsafe` must go
  through untagged pointers. The test that would show it firing: the whole case suite compiled with the tags
  (`fibc cases`, 4000-4017 persistence cases) plus a case that stores an `i64` through one array and reads the same
  bytes as `f64` through `f64->bits` (a bitcast of the value, not of memory, so it must keep passing).
* **`noalias` from ownership** (not needed given TBAA, so not proposed): at an `array-set!` after the unique test
  succeeds the cell is the only holder of the array, and a `:borrow` parameter derived from the same cell would have
  needed an uncounted alias of an object with count 1, which `@c` (an acquire, spec/types.md 6.6) cannot produce. I did
  not find the case that would break it, and I have not proved it; if it were used it needs the ownership checker to
  state it as a rule and a case that tries to pass `x` and `&y` of one array.
* **Eliding the pair around `@c` read by `array-get`.** Spec/types.md 6.12 says the compiler "does not elide cell reads
  (§6.3)", and justifies each elision it does make by "another count on the same object is provably held by a binding
  that nothing can write during the interval". For `(array-get @c i)` with every other argument a variable, constant
  or primitive arithmetic, the interval is the evaluation of those arguments plus one load: no user code runs, so
  `c` itself is the binding that holds the other count and nothing can write it. The multiset of freed objects is
  unchanged, which is what the audit compares. This is a change of a stated spec rule, so it is an owner decision, and
  it must come with a `fibc --explain` line and a case where an argument calls a function that writes `c` (then the
  pair must stay).
* **Loop versioning for checks** (`bounds`, index overflow): run the checked loop when the precheck fails; the
  precheck passing implies no trap in the fast loop, so every trap, its message and its index are produced by the
  original loop. Exact.
* **`reassoc`.** Opt-in per call (a form, a pragma on a `defun`, or a function with its own name such as `fsum`);
  it changes only the rounding of a result, never memory safety or a trap, and the Clojure-differential oracle must
  not be run against it.

## 7. Proposals, ranked by payoff over risk

Each line: change, files, measured payoff, risk.

1. **Opt-in FP reassociation.** An lIR fast-math flag on `fadd fsub fmul` (`(fadd reassoc a b)`) and a surface form;
   files: spec/lir.md 6.1, `compiler/lir/{ast,parse,check}.fib`, `compiler/native/lower/arith.fib` (needs an extern
   for `LLVMSetFastMathFlags` in `compiler/llvm/core.fib`), `crates/lair` and `crates/lir` for the Rust side while it
   exists, `compiler/emit/lower/arith.fib` and the checker/expander for the surface, `lib/fib/seq/consumers.fib` for
   `fsum`. Payoff: sum 5.9x, dot 2.8x on the isolated loops, with no other change. Risk: low (opt-in). Needs the
   owner's choice of surface (rule P1: Clojure has no such thing; Rust has `f64::algebraic_add`).
2. **Elide retain/release of a cell read consumed by `array-get`/`array-len`.** Files: `compiler/own/` (mode of `@c` in
   that position), `compiler/emit/lower/cells.fib`, `compiler/emit/lower/builtins.fib`, spec/types.md 6.3 and 6.12
   (owner decision), `compiler/mirror-pending/NAME.md` for the Rust. Payoff: axpy 275 to 145, fannkuch 370 to 190 alone;
   it is the largest single item of the in-place loops. Risk: low, if the rule above holds; the audit shows it.
3. **Known-unique bit and struct-path TBAA, plus the loop passes twice.** Files: `compiler/emit/lower/builtins.fib`
   (`array-set`), `compiler/native/lower/memory.fib` (tags from the GEP that made the address), a metadata extern in
   `compiler/llvm/core.fib`, `compiler/llvm/passes.fib` (run `default<On>` twice when the module contains an
   `array-set!` loop, or always: +0.08 s on a 6k-line module), `crates/lair` for the Rust side. Payoff with 2: axpy
   275 to 32 to 34, `sq` 118 to 30 to 40 (3.7x to 3.9x). Alone: none (table 2). Risk: low for the bit (the checks stay);
   medium for TBAA (an exact-aliasing proof obligation, section 6). With 1 it makes `y[i] += a*x[i]` and `dot` match C's `-O3`.
4. **Devirtualise literal closures** passed to a `defun` that is inlined (the expander's fusion machinery,
   `compiler/expand/fuse*.fib`, or specialisation in `compiler/emit/mono.fib`). Payoff: unmeasured directly; the other
   blockers removed leave `map-into` at 258 ms against 32 for the direct loop. Risk: medium, code growth.
5. **Range facts for index arithmetic and versioning on the loop bound.** `!range` on array lengths (`[0, 2^60)`) and a
   precheck loop version for `i+j` style arithmetic. Payoff: spectral-norm 1.41x. Risk: low for versioning; the range
   needs an lIR `!range` form or an `llvm.assume` intrinsic that the lIR does not have.
6. **CPU:** nothing for host builds. For prebuilt `x86-64-v2` binaries multiversion the numeric library kernels
   (width halves, table above). Package D.
7. **Not worth doing:** `-O3` (no measured change), a separate bounds-check hoisting pass for read loops (LLVM does
   it), `noalias` annotations (TBAA gives the same without a new ownership theorem), vectorising i64 sums (section 5).

What none of this fixes: mandelbrot and n-body-like code need explicit SIMD types (3.3x measured for mandelbrot).

## 8. Prototype

No compiler source was changed: the three emitter-side rewrites (`uflag`, `tbaa`, and the two-pass pipeline) are
prototyped as transformations of the emitted IR in `docs/design/vectorisation/strip.py` and `twice.sh`, and each
sound one produced the figures in table 2 with the real runtime (`uflag tbaa rc` kept `fib.unique?` and the bounds
checks; only the count pair was removed, which proposal 2 justifies). The smallest real change, the pipeline run
twice (`compiler/llvm/passes.fib`), changes nothing by itself (baseline run twice: 294, 285, 107 ms for axpyf,
axpyi, sq), which is why it is item 3's last step and not a first one. A source-level measure that needs no compiler
change: four independent accumulators in fibber source (`kern2.fib`, `sum4-f64`) run 37 ms against 83 for the
sequential fold (2.2x, not vectorised: the trapping index adds stay), at the cost of a different, but deterministic,
rounding order.

## 9. Not measured, and caveats

* Alignment and i64 index width effects (table in section 4): not measured.
* The `rc`, `uniq` and `bounds` rewrites are not sound programs; they show what removal could give. Only `uflag` and
  `tbaa` are sound as written, and only on loops without calls.
* Timings are single machine, best of 3, n=100000 (800 KB per array, in L2): the f64 kernels are near memory
  bandwidth at 14 ms, so larger vectors would show less.
* The outputs of the stripped shootout builds had equal md5 to their baselines (`results.log`), except that my
  link of the emitted module prints the program's return value `0` after its output, so md5s there are not
  `sizes.txt`'s; the real `fibc build` output matches `sizes.txt` (mandelbrot N=2000: `520440dc...`).
* The harness is the lead's `scripts/shootout` kernels at their small sizes, scaled up for timing where noted
  (spectral-norm N=4000, n-body 2e6 steps, mandelbrot N=4000 for the C comparison).
