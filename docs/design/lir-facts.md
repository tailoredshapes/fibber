# Facts the front end knows and lIR cannot say

Current status: the form of section 7 is **Decided** (owner, 2026-10-24) and implemented as spec/lir.md 6.13: the words `nsw`/`nuw`,
`(assume c)`, `(load (range LO HI) T p)` (LLVM's own forms in LLVM's spelling), and the check mode `FIB_LIR_FACTS=use|trap|ignore` in lair;
the gate runs the ownership and stdlib cases under `trap` (`make cases-trap`). The measurements below are the spike's, from before.

Everything quoted was produced on this machine (Intel i7-14700KF, LLVM 21 `opt`/`llc`, `-mcpu=native`) on 2026-10-24 with
stage 2 built from `9b329978`. The scripts are in `docs/design/lir-facts/` (they `cd` into the scratch directory
`/tank/data/fibber-scratch/lirfacts` they were written in; edit the first lines before reusing them) and the raw output is
`docs/design/lir-facts/results.log`.

## Summary

1. **Ceiling.** With every bounds, Vec index, overflow and no-match check removed from the program and its library (an
   unsound rewrite of the emitted IR), the allerbees cutter runs in 0.95 s instead of 1.50 s (1.58x). The blur and the
   resampling each run twice as fast (396 to 189 ms, 318 to 153 ms), the PNG unfilter 1.3x.
2. **No single kind of check matters on its own.** Removing one kind at a time moves the run by 0.97x to 1.05x. The
   reason is in the profile: 39% of the run is spent in out-of-line calls of `nth` on a persistent `Vec`, and each of
   those calls carries five checks on its flat path. LLVM does not put `nth` in line while those checks make it large,
   and it cannot hoist a check out of a loop when the check sits inside a call. The kinds pay together.
3. **Premises do not pay; conclusions do.** Facts given to LLVM so that it derives the rest (`!range` on lengths,
   `llvm.assume(len <= cnt)`, `!invariant.group`, a scoped-noalias domain for immutable fields, TBAA) gave 0.97x to
   1.03x and removed no check from a hot loop. `nsw` on the arithmetic gave nothing beyond removing the overflow test
   (all checks removed: 0.95 s with `nsw`, 0.93 s with plain wrapping ops). What pays is the front end dropping a check
   it can prove redundant.
4. **What the front end can prove, measured.** Library invariants of the Vec read path, `unreachable` after an
   exhaustive match, and `nth` put in line: 1.28x. Adding the index facts a front-end interval analysis can establish
   (a loop guard on the same immutable Vec, a literal lambda specialised into a loop bounded by `count`, the blur's
   clamped index arithmetic): 1.40x, 0.43 s of the ceiling's 0.55 s. The rest are indices read from data (`src`,
   `alpha`, `labels`), which no static fact covers.
5. **Soundness splits in two.** An overflow check that is dropped by emitting a *wrapping* operation stays memory safe
   even when the proof is wrong (the bounds check still catches a bad index). A dropped bounds check, `nsw`, `!range`
   and `assume` are undefined behaviour when wrong. The first kind needs no lIR change at all; the second is where an
   lIR form earns its place, because lair can then check it.
6. **Proposal.** Facts as droppable instructions and flags: `nsw`/`nuw` words on `add sub mul shl`, an `(assume c)`
   instruction, a `(range LO HI)` clause on an integer `load`. Every backend may ignore any of them (a fact only removes
   behaviours, so dropping it never changes a correct program), and lair gets a check mode that lowers each fact to a
   test and a trap, so the case suite turns a front end's false claim into a trap with a message instead of undefined
   behaviour.
7. **First step** (no lIR change, 1.18x to 1.19x measured, memory safe even if wrong): in the Vec read path, write the
   two subtractions as wrapping, lower an exhaustive match's fall-through to `unreachable`, and mark `vec-nth` `inline`
   in the lIR. The next step, dropping the tail bounds check of `vec-nth` (+0.06 s), is the first undefined-behaviour
   fact and should wait for the check mode.

## 1. Method

**Program and input.** `/tank/data/fibber-scratch/combined-cut/cut-sprites.fib` (346 lines; every image is a persistent
`Vec` threaded through loops), input `sheet-healer.png` (2752 x 1536 RGB), arguments
`OUT/bee-healer happy sneezy resting --size 360`. Built from inside the worktree so that `lib/` resolves.

**Baseline equals the real build.** `fibc build --whole-program` runs in 1.54 to 1.66 s. The experiments start from the
unoptimised IR of the same build and run LLVM's pipeline themselves, the way docs/design/vectorisation.md did:

```
F build --whole-program --no-cache -O 0 --emit llvm cut-sprites.fib -o cut-O0.ll
opt -S -passes='default<O2>' -mcpu=native IN.ll -o NAME.opt.ll     # build.sh: what compiler/llvm/passes.fib runs
llc -O2 -mcpu=native -filetype=obj -relocation-model=pic NAME.opt.ll -o NAME.o
cc NAME.o -o NAME -lz -l:libwebp.so.7 -lm -lpthread -ldl
```

This rebuilt baseline runs in 1.55 to 1.74 s and writes the same three WebP files byte for byte. Every variant below
was checked the same way (`mk.sh` compares the three files with the baseline's); all of them matched, except the two
noted as crashing.

**Rewrites.** `lfstrip.py` rewrites the unoptimised IR of every program and library function (names `f.*`, `l.*`,
`m.*`; the runtime `fib.*` is left alone) before `opt`. `mkc.sh` chains passes with a different function set each;
`redirect.py` points chosen `nth` call sites at an unchecked clone of `vec-nth`.

| flag | rewrite | sound? |
|---|---|---|
| `bounds` | a branch into a block that calls `fib.trap-index` goes to the in-bounds side | no |
| `nthrange` | a branch into `fib.trap("nth: index out of range")` is never taken (`vec-nth`'s `i < 0 or i >= cnt`) | no |
| `nomatch` | a branch into `fib.trap-c("no match clause matched")` is never taken | yes: the type checker proved the match exhaustive |
| `ovf` | every `llvm.s{add,sub,mul}.with.overflow` becomes a wrapping op and its overflow bit `false` | no as a blanket rule |
| `ovfnsw`, `ovfnuw` | the same, the op carries `nsw` (resp. `nuw nsw`) | no as a blanket rule |
| `range` | `!range [0, 2^60)` on every load of an array length and of a Vec's `cnt` | yes (allocation bound; see 6) |
| `vecinv` | `llvm.assume(len(tail) <= cnt)` in `vec-nth` | yes: the Vec invariant |
| `tbaa`, `tbaahdr` | TBAA tags on array lengths, elements per type, the Vec's fields, and the header words in `fib.retain/release` | argued in vectorisation.md 6 |
| `inv` | `!invariant.load` on loads of a Vec's tag and fields | **no**: crashes (section 4) |
| `invg` | `!invariant.group` on those loads, and `llvm.launder.invariant.group` on the shell of a reuse block | argued in 4 |
| `scoped` | those loads in one alias scope; every store and call outside a reuse or steal block `!noalias` it | argued in 4 |
| `inl` | `alwaysinline` on `vec-nth` and `Reducible.nth` | not a fact: separates inlining |
| `uniq` | `fib.unique?` is `true` (applied to `vec-conj` only) | no |

**Phases.** `cut-timed.diff` adds `sys-clock-now` timers to the cutter: `png` (read, inflate, `unfilter`/`unfilter-row`),
`bgmask`, `alpha` (the 3x3 minimum and the two blur passes, `sum` over a literal reading two Vecs through `at`),
`regions`, and per sprite `sprite-src` (the cropped, premultiplied source), `resample` (`resample-axis` twice) and
`encode+write`. Each variant was built twice, once from each source.

**Timing.** `time.sh`: wall time of the whole run pinned to one core (`taskset -c 6`), the variants interleaved, median of
5 (3 for the leave-one-out table); `tphase.sh`: the same for the phase timers, median of 3. Load average during the runs
2.2 to 5.8 on 28 threads; other workers were running. Run-to-run spread is about +-0.08 s, so differences under 0.05 s
are noise.

**Profile.** `perf` is not available (`perf_event_paranoid` 4), so `sampler.c` is an `LD_PRELOAD` library that records
the program counter on `SIGPROF` (about 1 ms apart) and `sym.py` maps samples to symbols with `nm`.

**Checks left after optimisation.** `loopchecks.py` asks `opt -passes='print<loops>'` for the loops of the optimised IR
and counts, per loop and excluding inner loops' blocks, the overflow intrinsics, the branches into each kind of trap
block, atomics and calls.

## 2. Where the time goes

Profile of the baseline (1500 samples, `profile.txt`):

| share | function | what it is |
|---|---|---|
| 21.3% | `f.cut` | `read-png`, `unfilter-row`, `background-mask`, the blur loops, all put in line |
| 18.8% | `f.fib.prelude.vec-nth.i64` | `nth` on a `(Vec i64)`, **not** put in line (`unfilter-row`, `sprite`) |
| 15.1% | `Reducible.nth.$Vec.f64` | `nth` on a `(Vec f64)`, not put in line (resampling, blur) |
| 9.5% | `f.resample-axis` | the resampling loops |
| 6.2% | (shared libraries) | zlib and libwebp |
| 5.4% | `Reducible.nth.$Vec.i8` | `nth` on a `(Vec i8)` (`rgb`, `alpha`, `bg`) |
| 4.7% | `l.f.soft-alpha.26555` | the blur's `at` closure, called out of line |

After all checks are removed (`checks`) the three `nth` functions are gone from the profile: they were put in line.

**The checks of one `nth`.** `vec-nth` (lib/prelude.fib:133) on the flat path runs a tag test whose default is the
no-match trap, `cnt - len(tail)` with an overflow test, the range test `i < 0 or i >= cnt`, the `i >= tailoff` choice,
`i - tailoff` with an overflow test, and the bounds test of `array-get` on the tail: five checks for one load. After
`default<O2>` the out-of-line `Reducible.nth.$Vec.f64` still has 3 overflow intrinsics, 3 `fib.trap-index`, 3
`fib.trap` and 5 `fib.trap-c` call sites in 146 lines (it includes the trie walk).

**Checks per iteration of the hot loops, baseline, after LLVM** (`loopchecks.txt`, the `base` section):

| loop | own checks | calls per iteration |
|---|---|---|
| `unfilter-row`, per byte (`loop8.i.i.i.i` in `f.cut`) | 7 overflow tests, 1 no-match switch | 4 x `vec-nth.i64` (5 checks each), 1 x `vec-conj.i64` |
| resampling, per tap (`check.i.i.i` in `f.resample-axis`) | 4 overflow traps (2 still `with.overflow` intrinsics, 2 turned by LLVM into compares against precomputed bounds) | 2 x `nth.f64` (5 each), the `index` closure (indirect, 2 overflow tests), `fib.retain-slow` of that closure |
| blur, per tap (`ok14.i*` in `f.cut`) | 1 overflow test | 2 x `nth.f64` (5 each), `at` (4 overflow tests), a retain |

So about 15 checks per tap in the two hot inner loops, almost all inside calls. LLVM hoisted none of them. It did prove
every induction increment `add nuw nsw` itself (as vectorisation.md section 4 found), and the fused `mapv` fill loops are
vectorised with no checks in them (the `vector.body` loops). With every check removed (`checks`), these loops have no
check, `nth` is in line, and what is left per tap is the trie walk loops of `vnode-get` (never entered on a flat Vec)
and one retain.

## 3. The ceiling

### Table 1: one kind of check, or one fact, at a time (wall s, median of 5, `final-times.txt`)

| variant | s | vs base | variant | s | vs base |
|---|---|---|---|---|---|
| base | 1.50 | 1.00x | `range` | 1.46 | 1.03x |
| `bounds` | 1.51 | 0.99x | `vecinv` | 1.50 | 1.00x |
| `nthrange` | 1.47 | 1.02x | `tbaa` | 1.54 | 0.97x |
| `ovf` | 1.45 | 1.03x | `invg` | 1.52 | 0.99x |
| `ovfnsw` | 1.43 | 1.05x | `scoped` | 1.52 | 0.99x |
| `nomatch` | 1.43 | 1.05x | `inl` | 1.31 | 1.15x |
| `uniq` on `vec-conj` | 1.67 | 0.90x | | | |

`ovfnuw` traps (`trap: array index -1 out of range 0..65544`, then a crash): a blanket `nuw` is wrong because some
subtractions legitimately go negative before a guard. `inv` crashes with signal 11 (section 4). The `uniq` slowdown was
not chased; it removes nothing the program needs.

### Table 2: kinds together and leave-one-out (wall s)

| variant | s | vs base |
|---|---|---|
| `checks` = `bounds nthrange ovfnsw nomatch` | 0.95 | 1.58x |
| `checkswrap` = the same with wrapping ops instead of `nsw` | 0.93 | 1.61x |
| `all` = `checks range tbaa` (first run, base 1.53 s) | 0.99 | 1.55x |
| `checks` without `bounds` | 1.33 | 1.14x |
| `checks` without `nthrange` | 1.30 | 1.17x |
| `checks` without `ovfnsw` | 1.16 | 1.31x |
| `checks` without `nomatch` | 1.03 | 1.48x |
| `bounds nthrange` | 1.46 | 1.04x |
| `bounds nthrange nomatch` | 1.13 | 1.35x |
| `ovfnsw nomatch` | 1.45 | 1.05x |

(The leave-one-out rows are a separate run, median of 3, base 1.52 s.) Each kind removed is worth something only once
the others are gone: `bounds nthrange` alone is 1.04x, adding `nomatch` 1.35x, adding the overflow tests 1.58x. Text size:
302,061 bytes for `checks` against 301,525 for the baseline.

### Table 3: per phase, ms (median of 3, `final-phases.txt`)

| variant | png | bgmask | alpha | sprite-src | resample | sum |
|---|---|---|---|---|---|---|
| base | 293 | 114 | 396 | 323 | 318 | 1516 |
| `checks` (ceiling) | 224 | 59 | 189 | 229 | 153 | 919 |
| `libnthovf` (section 5, every index fact and every overflow fact) | 239 | 60 | 183 | 228 | 142 | 918 |

(`regions`, `sprite-out` and `encode+write` stay at about 31, 0 to 3 and 36 ms in every variant.)

## 4. Premises do not pay: what LLVM did with facts

These are facts given to LLVM in the hope that it derives the check away itself.

* **`!range [0, 2^60)` on lengths and counts** (sound): 1.03x. It does let InstCombine turn the `cnt - len` overflow
  test into `sub nsw` (3 to 1 overflow intrinsics in the out-of-line `nth`), which is what a front end would emit anyway.
* **`llvm.assume(len <= cnt)`** in `vec-nth`, with `nsw` on both subtractions: the tail bounds test
  `i - (cnt - len) <u len` stays, although `cnt - len <= i < cnt` implies it (the optimised `nth` in `v-nswsound.opt.ll`).
  LLVM did not combine the facts.
* **TBAA** on lengths, elements, the Vec's fields and the header count of the in-line retain/release: 0.97x alone, and
  with `nth` in line (`libinltbaa`) 1.31x against 1.28x without it: noise.
* **`!invariant.load`** on a Vec's tag and fields: the program crashes. It is unsound in fibber: a constructor whose
  plan names a shell reuses a unique object's memory in place (compiler/emit/lower/objects.fib:59, the `reuse`/`fresh`
  blocks in the IR), and a destructuring of a unique value steals a field by storing `null` into it (the `steal` blocks).
  The same address then holds other field values, and `!invariant.load` declares that never happens.
* **`!invariant.group`** on those loads with `llvm.launder.invariant.group` on the shell of each reuse block (the form
  LLVM made for objects rebuilt in place): 0.99x, and with `nth` in line 1.19 s against 1.15 s for `libinl` in the same
  run.
* **One alias scope for immutable fields** (the loads of a Vec's tag and fields in the scope, every store and call
  outside a reuse or steal block `!noalias` it, which ownership justifies: nothing writes a borrowed object's fields):
  0.99x, and 1.28x with `nth` in line, the same as without.

Why the last three found nothing, from the optimised IR of `resample-axis` with `nth` in line (`v-libinlsc.opt.ll`):
the loads of `cnt`, `root` and `tail` sit behind the tag test inside the loop, and LICM may not move a load above a
branch unless it knows the address is dereferenceable. A `VecEmpty` object is smaller than a `VecOf`, so the field
offsets are not dereferenceable before the tag is known; the facts about aliasing are never reached. The range test
`i < cnt` then compares with a value loaded in the loop, which LLVM cannot equate with the `(count w)` that bounds the
loop. Each obstacle could be removed (unswitching on the tag, full-size empty objects plus `dereferenceable`), but each
is one more inference LLVM has to make about things the front end already knows.

**`nsw`/`nuw` as flags** add nothing measurable here beyond removing the overflow test (`checks` 0.95 s, `checkswrap`
0.93 s). LLVM already proves the induction increments; the index expressions are `i64`, so there is no `sext` for `nsw`
to remove.

## 5. What the front end can prove, measured

The variants below keep every check a front end could not prove. `lib` applies `ovfnsw nomatch bounds range vecinv` to
the Vec read path only (`vec-nth`, `vnode-get`, `Reducible.nth` on `Vec`) and `range nomatch` everywhere else. The
`r*` variants also send chosen call sites to an unchecked clone of `vec-nth` (`redirect.py`).

| variant | what is added | s | vs base | sound by |
|---|---|---|---|---|
| `safe0` | Vec read path: wrapping subtractions, `range`; `nth` in line | 1.25 | 1.18x | memory safe even if wrong |
| `safe1` | `safe0` + `nomatch` | 1.24 | 1.19x | + exhaustiveness |
| `lib` | Vec read path facts incl. the tail bounds test; `range nomatch` everywhere | 1.40 | 1.07x | Vec invariant |
| `libinl` | `lib` + `nth` in line | 1.17 | 1.28x | Vec invariant |
| `rLPSKinl` | `libinl` + the provable `nth` range tests (below) | 1.18 | 1.27x | + interval facts |
| `rA` | `rLPSKinl` + no overflow test in the blur's index arithmetic | 1.07 | 1.40x | + interval facts |
| `libnth` | `lib` + every `nth` range test removed | 1.14 | 1.32x | no (index from data) |
| `libnthovf` | `libnth` + every overflow test removed | 0.98 | 1.53x | no |

(`safe0`, `safe1` are from a run where base was 1.48 s, `inl` 1.28 s and `libinl` 1.18 s.) Per phase, ms:

| variant | png | bgmask | alpha | sprite-src | resample | sum |
|---|---|---|---|---|---|---|
| base | 293 | 114 | 396 | 323 | 318 | 1516 |
| `lib` | 309 | 99 | 359 | 270 | 271 | 1382 |
| `libinl` | 268 | 79 | 336 | 256 | 203 | 1211 |
| `rLPSKinl` | 258 | 75 | 322 | 269 | 214 | 1208 |
| `rA` | 258 | 100 | 229 | 267 | 200 | 1124 |
| `libnth` | 248 | 62 | 282 | 238 | 183 | 1080 |
| ceiling (`checks`) | 224 | 59 | 189 | 229 | 153 | 919 |

`rLPSKinl` equals `libinl` in total: once `nth` is in line, the range tests at the provable sites are cheap, and the
sites that cost are the unprovable ones. Text size: +3.5% for `libinl` and +3.8% for `rA` (`inl` alone +6.4%: the
checks are most of `nth`'s size).

The provable sites, and the argument for each:

| site | index | why it is in `[0, count v)` | analysis needed |
|---|---|---|---|
| `unfilter-row`: `(nth prev i)` | loop variable | the loop exits on `(>= i (count prev))`; `prev` is an immutable local | local: a guard on the count of the same immutable value in the same loop (the hoist pass already relies on "an array bound to a live local never changes length", compiler/emit/lower/hoist.fib:102) |
| `unfilter-row`: `(nth prev (- i bpp))` | `i - bpp` | taken only when `(>= i bpp)`, and `i < count prev` | local, linear |
| `unfilter-row`: `(nth cur (- i bpp))` | `i - bpp` | `cur` starts `[]` and gains one `conj` per iteration, so `count cur = i` | a loop invariant on a count through `conj` |
| resampling: `(nth w j)` | `sum`'s loop variable | `(sum 1 (count w) f)`: the specialisation of `sum` for the literal (compiler/emit/lower/spec.fib) calls `f` only inside `lo <= i < hi` | the callee's loop range carried into the specialised literal |
| blur: `(nth kernel (+ d 3))` | `d + 3` | `sum -3 4`: `d` in `[-3, 4)`, and `kernel` is a `mapv` over `(range -3 4)`, count 7 | a count fact from fusion (compiler/expand/fuserange.fib writes one array of the range's count) |
| blur: the index arithmetic of `at` and its callers (`rA`) | `(+ (* (clamp y 0 (- h 1)) w) (clamp x 0 (- w 1)))`, `(+ (rem i w) d)`, `(+ d 3)` | `w`, `h` come from `be32`, in `[0, 2^32)`; `clamp` bounds `y` by `h - 1`; `(* w h)` already succeeded when `mn` was built, so `y*w + x < w*h` fits | intervals, plus "a dominating checked product bounds a smaller one" |

Not provable by any static fact in this program: `(nth src ...)` in the resampling (the index comes from `lo`, a double
read from the weights Vec), `(nth mn (at ...))` and `(nth across (at ...))` in the blur (in range, but only by
`y*w + x < w*h`, which is non-linear in the counts), and the sprite source's `(nth alpha i)`, `(nth rgb ...)` and
`(nth owner (nth labels ...))`. These are most of the gap between 1.40x and 1.58x.

## 6. How each fact is guaranteed

A wrong uniqueness guess costs a copy. A wrong fact here is, for most kinds, undefined behaviour. The table says how
each is kept true.

| fact | source | if it were wrong | guarantee |
|---|---|---|---|
| exhaustive match: the fall-through is `unreachable` | the type checker's exhaustiveness; emitted at compiler/emit/lower/ctrl.fib:111 and pattern.fib:216 | UB on a tag no constructor writes (memory corruption, a compiler bug) | trusted emission; the check mode keeps the trap |
| lengths and counts in `[0, 2^60)` | the allocator: an array's bytes fit the address space, a Vec's `cnt` is a sum of array lengths | UB through `!range` | trusted emission. Caveat: an array of a zero-size element type has no memory bound; use `[0, 2^63)` there, or no range |
| Vec read path: `cnt - len` and `i - tailoff` do not overflow | the Vec invariant (types section 8.3: the tail holds the last `len` elements, `tailoff = cnt - len >= 0`) | with wrapping ops: a wrong index, caught by the bounds test. With `nsw`: UB | the prelude is trusted code; write them `unchecked-subtract` (stdlib section 7, exists) |
| Vec read path: the tail index is in bounds | the same invariant plus `tailoff <= i < cnt` | out-of-bounds read | the first fact that needs an unchecked read in the prelude; gate it on the check mode and the Vec persistence cases |
| `nth` index in range at a provable site | section 5, by the front end | out-of-bounds read | an interval analysis in the emitter, derived only by trusted emission; re-checked by the check mode |
| index arithmetic cannot overflow | section 5, by the front end | with wrapping ops: a wrong index, caught by any bounds test that stays; with `nsw`: UB | emit the proven operation as wrapping `add`/`mul`, not with `nsw`, unless the bounds test it feeds is also dropped |
| a borrowed object's fields do not change | ownership (compiler/own) | stale field values | not proposed: `!invariant.load` is false under shell reuse, and the sound forms measured nothing (section 4) |
| the unique-write test hoisted out of a loop | already done for `array-set!` loops (emit.lower.hoist) | a missed copy | exists; `uniq` on `vec-conj` measured no gain here |

The useful split: overflow facts can be spent as wrapping arithmetic, which keeps memory safety when the proof is wrong;
bounds facts cannot. Measured, the wrapping form loses nothing (Table 2, `checkswrap`).

The lIR checker (compiler/lir/check) sees one function's instructions and literal operands. It can reject a fact on the
wrong form, a malformed range, or a literal that already violates the fact (`(add nsw (i64 9223372036854775807) (i64
1))`); it cannot establish a fact about run-time values. That is the reason for the check mode below: the verification is
dynamic, over everything the case suite runs.

## 7. The lIR form (proposed)

The owner's principle is that lIR is just instructions, and the one precedent of an annotation is the fast-math word.
The proposal keeps both: each fact is either a word on an instruction or an instruction of its own, never a side table,
and every fact is **droppable**: a backend that ignores it computes the same results for every program whose facts are
true. Unlike the fast-math words, a false fact is undefined behaviour, so spec/lir.md section 6.12 would list them.

| form | meaning | checker | lair (LLVM) | check mode | C, JS, wasm, WGSL |
|---|---|---|---|---|---|
| `(add nsw a b)`, `(sub nuw nsw a b)`, `mul`, `shl` | the operation does not wrap as signed (`nsw`) or unsigned (`nuw`) | words as for fast-math (`duplicate flag nsw`, `unknown integer flag foo`, a flag on another form); a literal operation that wraps is an error | `LLVMSetNSW`/`LLVMSetNUW` | lowered as `s/uadd.with.overflow` and a trap naming the fact | ignored (C keeps its wrapping helpers) |
| `(assume c)` | `c : i1` is true here; void, not a terminator | `assume needs an i1 operand`; a literal `false` is an error | `llvm.assume` | `br c` to the next instruction or a trap | C may write `if (!(c)) __builtin_unreachable();`; the others drop it |
| `(load (range LO HI) T p)` | the loaded integer is in `[LO, HI)` | `T` integer, `LO < HI`, both literals representable in `T`; written like `(align N)` | `!range` metadata | two compares after the load and a trap | ignored |

The check mode is a lair option (an environment variable such as `FIB_LIR_FACTS=trap|use|ignore`, with `use` the default
for builds): `trap` turns every fact into a test, `ignore` drops them all (the reference semantics), `use` gives them to
LLVM. The gate would run the case suite once with `trap`, so a front end that emits a false fact gets a trap with a
message naming it instead of a silent miscompile. That is the only way an lIR fact can be "verified by lair".

Not proposed now: `exact` on divisions and shifts (nothing measured needs it); TBAA classes on loads and stores
(vectorisation.md section 7 item 3 has the case; nothing measured here needs it); `noalias`, invariant loads and scopes
(section 4); an "index in bounds of array X" fact (a dropped bounds test is simpler and is what paid).

What the forms buy over the front end simply dropping the check, which lIR already allows (wrapping `add`, no `icmp`
and `br`): a portable spelling of the claim that other front ends share; the check mode, which plain omission cannot
have; and the facts LLVM does use (`!range` made it drop overflow tests of its own accord). On this program the measured
payoff of the forms themselves is small (Table 1, `range` 1.03x); the payoff is in the analyses that produce the claims.

## 8. Risks

* **A false fact is undefined behaviour**, not a trap. The check mode only covers what the suite executes; a fact that is
  false on an input the suite lacks is a miscompile. Keep overflow facts as wrapping arithmetic wherever a bounds test
  stays behind them.
* **Shell reuse and steals rewrite fields in place.** Any fact about "this field does not change" must be scoped by
  ownership and laundered at reuse; `!invariant.load` is simply wrong (it crashed the cutter).
* **The self-hosting trap.** If `assume` or a range form is ever exposed as a fibber builtin, its name must not be one
  the compiler defines (CLAUDE.md); grep `compiler/` first. As lIR words only, they do not collide.
* **Inlining grows code.** `nth` in line costs +3.5% text with the facts and +6.4% without; a program with many `nth`
  sites in cold code pays size for nothing. An `inline` on the flat path only (the trie walk out of line) would be the
  refinement.
* **The interval analysis is the real work** and is new code in the emitter: loop guards, callee loop ranges carried into
  specialised literals, count facts out of fusion, dominating checked products. It has to be conservative across
  `set!`, cells, closures that escape and `async`.
* **Measurement noise.** Single machine, shared with other workers; spread about +-0.08 s. The ranking of the big
  effects (1.58x ceiling, 1.28x with the Vec read path, 1.40x with interval facts) is stable across three runs; the
  differences under 0.05 s are not.

## 9. Ranked plan

1. **Vec read path, no lIR change** (memory safe even if wrong): `vec-nth`'s two subtractions as `unchecked-subtract`
   (lib/prelude.fib:137, :141), the no-match fall-through after an exhaustive match lowered to `unreachable`
   (compiler/emit/lower/ctrl.fib:111, pattern.fib:216; keep the trap under a debug option), and `vec-nth` emitted with
   lIR's `inline` modifier (spec/lir.md 4.3; there is no source-level marker, so the emitter decides for this one
   accessor). Measured 1.18x to 1.19x on the cutter (`safe0`, `safe1`). Needs: the Vec persistence cases and the golden
   checks after the prelude change.
2. **The check mode and the three forms** (`nsw`/`nuw`, `assume`, load `range`): spec/lir.md 6.1, 6.5, 6.12;
   compiler/lir/{ast,parse,check}.fib; compiler/native/lower/{arith,memory}.fib (`LLVMSetNSW`, metadata on loads, which
   compiler/llvm/core.fib needs externs for); the C, JS and WGSL printers ignore them. Owner decision on the form.
3. **The tail bounds test of `vec-nth` dropped** (needs an unchecked tail read in the prelude), and `range` on the length
   and `cnt` loads: +0.06 s over step 1 (`libinl` 1.17 s against `safe1` 1.24 s). The first undefined-behaviour fact;
   land it with step 2's check mode.
4. **Interval facts in the emitter**: loop guards on immutable locals, callee loop ranges into specialised literals
   (compiler/emit/lower/spec.fib), count facts from fusion (compiler/expand/fuse*.fib), and overflow-free index
   arithmetic emitted as wrapping ops. Measured 1.27x to 1.40x with steps 1 and 3 (`rLPSKinl`, `rA`).
5. **Later, if a program needs it**: dereferenceability and tag unswitching so that LLVM hoists a borrowed Vec's fields
   (section 4's obstacle), TBAA for the in-place loops (vectorisation.md item 3).

The compiler is itself persistent-Vec code, so steps 1 and 3 should speed up F too; that was not measured here.

## 10. Not measured, and caveats

* Kernels were measured in place, by phase timers in the real program, not extracted.
* One program. Code that indexes Arrays rather than Vecs has fewer checks per access (one bounds test, which LLVM hoists
  in read loops: vectorisation.md section 4), so it would gain less.
* The `r*` variants model what an analysis would prove by sending hand-picked call sites to an unchecked clone; no
  analysis was written.
* Count operations in the loops (the `index` closure's retain in the resampling, `fib.retain-slow` calls) and the
  closures' indirect calls are a separate cost, owned by the closure specialisation and count elision work
  (vectorisation.md section 7 items 2 and 4); they also keep LLVM from reasoning across the loop body.
* `regions` and `encode+write` were not studied (under 70 ms together).

## 11. Implemented (2026-10-25)

spec/lir.md 6.13 has the forms and the check mode. The emitter proves two kinds of fact by construction (compiler/emit/lower/facts.fib
has the argument for each); every one is re-tested by the gate's `cases-trap`, where a false one traps with `lIR fact violated`:

1. **The tail read of `vec-nth`** (plan item 3): its bounds test becomes `(assume (icmp ult j len))`, by the guard and the Vec invariant.
2. **Bounded steps** (plan item 4, its first part): `(+ b 1)` in the then-branch of `(< b y)` (and `(- b 1)` under `(> b y)`), `b` an
   immutable integer binding, is `(add nsw ..)` with no overflow test: the counted loops of fibber code and of the fused range loops.
   The cutter emits 80 of them.

Measured on the cutter (`/tank/data/fibber-scratch/facts/cut/time.sh`, 5 interleaved runs, outputs byte-identical to main's; median
wall s; `a` is fact 1 alone, `b` fact 2 alone; two rounds, the second pinned with `taskset -c 6`, load 2.8 to 4.1):

| build | before | a | b | both |
|---|---|---|---|---|
| `--whole-program`, round 1 | 1.055 | 1.016 | 1.067 | 1.007 |
| `--whole-program`, round 2 | 1.032 | 0.997 | 1.072 | 1.009 |
| default, round 1 | 1.859 | 1.880 | 1.847 | 1.790 |
| default, round 2 | 1.897 | 1.816 | 1.871 | 1.839 |

Fact 1 is worth about 0.03 to 0.04 s in the whole-program build (section 5 predicted 0.06 s), fact 2 nothing measurable: LLVM proves
the increments itself (section 4), so the fact only replaces an overflow test LLVM had already folded. Both are within the run-to-run
spread of about 0.08 s; the direction of fact 1 held in both rounds. The cutter built under `FIB_LIR_FACTS=trap` runs in 1.36 s with
the same output.

Not done: the interval facts of section 5 that pay (`nth` range tests at provable sites, the blur's index arithmetic). They need what
section 8 lists: a lower bound for a loop variable (an induction fact: the start and the sign of the step), a relation between a
guard's `(count v)` and the `cnt` field vec-nth loads after it is put in line (the same immutable `v`, across the call), the callee's
loop range carried into a specialised literal (`sum`'s `lo <= i < hi` into `f`), the count fusion knows for a `mapv` over a `range`,
and the dominating checked product of the blur. Each is a fact the front end can state as an `assume` at the call site; the work is
the analysis that proves it.
