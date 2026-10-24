# Persistent Vecs of scalars: where the time goes, and what closes the gap

Status: measured at `a548530b` plus the `first` row (commit after it). Question from the owner: an image cutter written with persistent `Vec`s
(allerbees `tools/cut-sprites.fib` at `402ac1a`) took 4.2 s and 187 MB on a 2752x1536 RGB sheet, the same algorithm on mutable arrays
(`732b308`) 1.3 s and 126 MB. Which of the suspects is true: (a) small fixed pipelines do not fuse, (b) `nth`/`assoc` on a uniquely held
`Vec` of scalars is slower than `array-get`/`array-set!`?

Principle (owner, binding for the recommendations): idiomatic persistent code is the default and must not be punished; the compiler and
library do the optimising ("persistent to mutable when it can prove it safe", like tail calls and rc). Nothing below recommends writing
arrays or cells by hand.

## 1. Method and caveats

- Compiler: stage 2 `F` built from this tree with the v0.1.17 seed (`scripts/lib/stage2.sh`), `F build` default (-O2, **partitioned, cached
  units**, see section 4) unless a row says `--whole-program` or `FIB_LIRO_IMPORT=80`.
- Machine: 28 cores, shared with other workers. Several runs were taken at load 35 to 70 and discarded (they inflate everything 1.5 to 2.5x).
  The numbers below are the minimum of 5 runs (micro-benchmark) or the three runs listed, `taskset -c` on 3 to 4 cores, load 2 to 3.
  Run-to-run spread on a quiet machine: the cutter 6.49 / 6.85 / 6.62 s (+-3%); micro rows within 5%.
- Input: `sheet-sunny.png`, a real 2752x1536 RGB sheet from the allerbees session scratchpad (`/tmp/claude-1000/-tank-repos-allerbees/*/scratchpad/art/`),
  command `cut-sprites sheet.png OUT happy delighted curious worried sleepy --size 720`. Both versions write byte-identical webp files.
- No `perf` here (`kernel.perf_event_paranoid=4`), no valgrind; `gdb` attach is blocked. Attribution is therefore by (1) the runtime's free trace
  (`fibc run --trace`, counts of heap objects), (2) a micro-benchmark of each operation, (3) the lIR and assembly of the hot functions, and
  (4) a cost model (operations per pixel x unit cost) checked against measured phase times. It is not a sampling profile.

## 2. Reproduction of the two end-to-end numbers

| build of the cutter | wall | peak RSS | allerbees reports |
|---|---|---|---|
| pure Vecs (`402ac1a`), default build | 6.5 to 7.2 s (6.49, 6.85, 6.62 quiet) | 205 MB | 4.2 s, 187 MB |
| pure Vecs, `--whole-program` | 4.3 to 4.4 s | 205 MB | |
| pure Vecs, `FIB_LIRO_IMPORT=80` | 4.4 s | 205 MB | |
| arrays through cells (`732b308`), default build | 1.21 to 1.25 s | 156 MB | 1.3 s, 126 MB |
| arrays, `--whole-program` | 0.91 s | 157 MB | |

So the allerbees pair is reproduced in shape (3.2x there, 5.4x with my default build, 4.7x whole-program). **Their 4.2 s matches a
whole-program (or import) build of the same source; my default build is 1.5x slower than that**. I cannot say how they built; section 4
is why it matters. Per phase (ms, whole-program build, instrumented copies; the sprite+encode row is total minus the four timed phases):

| phase | pure | arrays | ratio |
|---|---|---|---|
| read PNG + unfilter (conj per byte) | 505 | 98 | 5.2x |
| background sweeps | 366 | 42 | 8.7x |
| 3x3 min + 7-tap blur (nth per tap) | 786 | 173 | 4.5x |
| label + figures | 71 | 3 | |
| crop / resample / encode (5 sprites) | ~2600 | ~600 | ~4.3x |

Default build, same phases: 884 / 589 / 1328 / 110 ms pure against 123 / 70 / 272 / 6 ms arrays.

## 3. Micro-benchmark: `scripts/bench/vec-scalar.fib` (+ `vec-scalar.sh`)

4 227 072 elements (one sheet's pixels), minimum of 5 runs, **picoseconds per element** (or per pipeline for the last block); a persistent row and
its array twin print the same checksum, `vec-scalar.sh` fails otherwise. `array-get`/`array-set!` loops over a whole array are vectorised by
LLVM, so the array columns are a floor, not a fair scalar stencil (a scalar stencil is about 1 ns a tap).

| row | default | whole-program | allocs | in place? |
|---|---|---|---|---|
| `nth` over `(Vec i8)` / `i64` / `f64` | 3 674 / 3 471 / 3 650 | 3 354 / 3 463 / 3 437 | 0 | n/a |
| `array-get` twin | 601 / 424 / 461 | 218 / 420 / 455 | 0 | |
| `assoc` loop on a unique Vec (i64 / i8 / f64) | 71 110 / 70 222 / 71 046 | 20 283 / 20 912 / 20 755 | **0** | **yes** |
| `array-set!` twin | 465 / 24 / 519 | 456 / 24 / 513 | 0 | |
| `conj` loop building a Vec (i64 / i8) | 23 722 / 24 162 | 11 452 / 11 573 | 0.29 per element (9.3 per leaf; 5.8 at depth 1, +1 per decade of size) | tail in place; shell and path partly allocated |
| `array-push!` twin (the bench's twin is now `array-grow`, a doubling array of `array-set!` and `array-blit!`, ADR 0015: 5 052 / 1 042 where `array-push!` gave 3 471 / 916 in the same run) | 2 643 / 998 | 2 569 / 806 | ~0 | |
| `mapv` over a Vec (i64) | 17 373 | 8 112 | 0.065 (2 per leaf) | bulk builder |
| array loop twin | 1 401 | 836 | | |
| pipeline `(reduce + 0.0 (mapv f (range -3 4)))` | 249 184 | 121 631 | **11 per pipeline** | |
| pipeline `(reduce + 0.0 (map f (range -3 4)))` | 101 482 | 43 942 | 3 (range, closure, recipe) | fused |
| the same, hand `loop` | 6 118 | 3 257 | 0 | |
| `(some? (first (filter p (range 9))))`, before the `first` row | 498 177 | not measured | **57 per pipeline** | memoised seq |
| the same, with the `first` row | 81 091 | 34 081 | 3 | fused |
| the same, hand `loop` | 2 719 | 2 856 | 0 | |

In place: counted, not assumed. 100 000 `assoc` allocate 0 objects after the build (`fibc run --trace`; the build alone is 6 477), for i64, i8
and f64, and case 4010 pins it. The `assoc` accumulator in the cutter's sweeps is unique, so nothing is copied.

## 4. Diagnosis: the gap split

Suspect (a), unfused small pipelines: **true for idiomatic code, and large**: 11 / 3 / 57 heap objects and 100 to 500 ns a pipeline against
3 to 6 ns for the hand loop (20 to 180x). The cutter at `402ac1a` already has the author's hand loops in `around?` and `sum` (that is the
12.4 s to 4.2 s step they report), so the pipelines no longer appear in the numbers of section 2; I did not rebuild the 12.4 s version.
The expander's fusion (stdlib P2) already covers `reduce` over `map/filter/range`, but (i) `first` and `mapv` were not terminals, (ii) even a fused
chain costs 3 allocations and an indirect call per element per stage (44 ns for 7 elements against 3), because the recipe struct and the closure
are values, not code.

Suspect (b), `nth`/`assoc` on a unique scalar Vec: **true, and it is not copying**. Table of the four causes asked for, on the cutter
(whole-program build, pure minus arrays: 1.42 s over the three timed phases, ~2.0 s over the sprite phase):

| class | evidence | share of the excess |
|---|---|---|
| (A) updates that are not in place (copies) | `assoc` 0 allocations; background phase allocates 27k objects for 422k pixels; no array-slice in the path | **~0** for `assoc`. `conj`'s leaf push allocates a shell and a new tail per 32 elements and the path nodes it cannot reuse (9.3 objects per leaf at 100 000, +1 per decade): about 0.29 objects per `conj` against 0 for `array-push!` |
| (B) trie walk and ownership protocol per `nth`/`assoc`/`conj` even when unique | `nth` 3.4 ns (5 levels at 4.2 M, each a dependent load pair, no retain/release in `vnode-get`) against 0.4 to 1 ns; `assoc` 20 ns = about 4 ns a level in the optimised build, 70 ns in the partitioned build; `conj` 11.5 ns against 2.6 | **nth ~42%** (~64 `nth` a pixel over the three phases x 2.4 ns excess), **conj/assoc/mapv builders ~30%** |
| (C) boxed or heap-object scalars | `(Array i64)` is a `%struct.fib.array.i64`, `Vec i8` is a packed byte array; RSS is +30% (205 against 157 MB), the shells and tails | **0** |
| (D) unfused small sequences | closures and recipes: ~2 allocations a pixel in the blur (82k + 324k objects on 422k pixels) | ~15%, in the cutter only the closure per `sum`/`around?` call |
| (E) other | float math, `rem`/`quot`, the loop | rest |

Model check (so the split is not just asserted): unfilter per pixel = 6 `conj` x 11.5 + 13 `nth` x 3.4 + ~20 other = 135 ns x 4.2 M = 570 ms against
505 measured; blur = 37 `nth` + 3 builders + 2 closure allocations + math = 185 ns = 780 ms against 786 measured; background ~390 ms against 366.
The model fits the three phases within 10%, which is the evidence for the split. **Not established**: the sprite phase (2.6 s of 4.3 s) is
modelled only by analogy: it is `nth`-dominated (two `nth` per resampling tap, 2 M outputs a sprite) but I did not count its taps.

**The largest single effect is not in the table: the partitioned default build.** ADR 0024 / `docs/design/compiled-lir-objects.md`: a default
`fibc build` splits the checked program into 16 name-hash groups plus runtime and glue and compiles them as separate cached objects, "no
cross-group LLVM inlining". `docs/design/compile-performance.md` already records "small user programs up to 3.6x slower" because
retain/release/drop fast paths and prelude specialisations stop inlining. Measured here:

- `assoc` 70 to 20 ns, `conj` 23 to 11.5, `mapv` 17 to 8, pipelines 2 to 3x, the cutter 6.5 to 4.3 s (-34%), the arrays version 1.21 to 0.91 s;
- `FIB_LIRO_IMPORT=80` (copy callees of at most 80 nodes into each unit) gives the same: `assoc` 20.7 ns, cutter 4.4 s; `FIB_LIRO_IMPORT=200
  FIB_LIRO_IMPORT_DEPTH=4` did not (72 ns; not understood);
- `scripts/bench/quick.sh` with the stage 2 of this tree: default build is +30% to +260% over `baseline.tsv` on nine of ten benchmarks
  (num-f64 5.25 s against 1.46 s). The baseline was recorded by a whole-program F, so those deltas are not regressions of this change (before
  and after the `first` row are equal within noise: num-f64 5.25 / 5.25, vec-conj-pop 1.68 / 1.66, lazy-fused 1.83 / 1.81, strings 0.84 / 0.83),
  they are the price of the default build. With `FIB_LIRO_IMPORT=80`: num-f64 1.46, vec-sort 0.44, map-assoc-get 0.29, set-conj 0.23, strings
  0.70, vec-conj-pop 0.99 (+10%), num-nbody 0.65 (+10%), lazy-fused 0.85 (+25%); with `FIB_LIRO_MODE=whole`: all within +12%.

## 5. Recommendations, ranked

Gain figures are what the measurements above support; those marked (est.) are model estimates, not measured.

0. **Make the default executable build inline the small runtime and prelude callees** (`FIB_LIRO_IMPORT=80` as the default for `fibc build`
   of executables, or whole-program below a size). Zero language risk. Measured: cutter 6.5 to 4.4 s, every persistent operation 2 to 3.5x,
   quick suite from +30..260% to within +10..25% of the whole-program baseline. Cost: the partitioned build's CPU doubles
   (`compile-performance.md`: 124 s against 56 s for the compiler itself; small programs are seconds). Owner decision; I did not change the default.
1. **Done: `first` is a terminal of T.** `(some? (first (filter p (range 9))))` 498 to 81 ns (default build), 57 to 3 objects; cases 4206, 4207.
   Same family, same one-line shape, not done: `some` (`some-p`), `second`, `take-nth`-style consumers; each needs its case.
2. **Inline the recipes of a fused chain into a counted loop when the stage functions are `fn` literals** (expander, `compiler/expand/fuse*.fib`).
   A chain `Mapped/Filtered/Taken` over `range` or a Vec with a terminal of T becomes the `loop` the author wrote by hand; the closure and the recipe
   struct disappear. Measured headroom: fused-but-not-inlined 44 ns and 3 objects against 3 ns and 0 for the hand loop on a 7-element pipeline;
   `first` over `filter` 34 against 3 ns. (est.) 10x on small pipelines, which is the whole of suspect (a) once the shape is covered; the
   cutter's 12.4 to 4.2 s was exactly this done by hand. Risk: effect order and traps (the existing rule "a stage is rewritten only if its
   argument is a symbol, literal or fn form" already pins order, case 831); test: the law cases 828 to 830 and 4206/4207 against the bound-seq
   reading, plus an `allocs: 0` bound.
3. **A flat representation for scalar Vecs** (built, section 8, as a VecOf with no trie rather than a new variant; new variant `VecFlat cnt arr` of `Vec`, used when built by `vec`/`into`/`mapv`/`zeros` of known
   size or by `conj` from empty; `nth` = bounds check + load; `assoc`/`conj` on a unique value = `array-set!`/`array-push!`; a shared `assoc`
   converts to the trie once, O(n), then stays a trie; `freeze`/printing/equality/hash read through `nth`). (est.) `nth` 3.4 to ~1 ns, `assoc` 20
   to ~2, `conj` 11.5 to ~3, `mapv` 8 to ~1: on the cutter the nth share (~42%) and builder share (~30%) of the excess shrink by two thirds,
   4.3 s to about 2.3 s. Semantics risk: value semantics are kept by the same `fib.unique?` test, but a persistent-history program
   (`assoc` on a shared 1 M-element flat Vec) pays one O(n) conversion where the trie paid O(log n); the lazy conversion makes it once per
   Vec, not per update. Every `match` on `VecOf` in `lib/prelude.fib` (about 25 sites: `vec-nth`, `vec-assoc`, `vec-conj`, `vec-pop`, `vec-each-from`,
   slicing, the sorts) gains an arm. Test: the 4000 to 4017 in-place cases, the differential cases against Clojure's `PersistentVector` already
   used for the stdlib, a generator that interleaves flat and trie versions of one Vec and compares every version after every update. Built:
   section 8.
4. **Unique-accumulator loops compile to an array-backed builder** (the owner's "persistent to mutable at compile time"). Needs 3 to be free at
   the exit: `loop [v (init)]` whose `v` is used only by `conj/assoc/nth/count` and escapes only through the loop result is thawed to an
   array and length at entry, mutated, and wrapped at exit (O(1) with the flat variant, O(n/32) with the trie). If `v` starts from a parameter
   the entry needs the run-time test (`fib.unique?`, O(1)) and falls back to the persistent loop when it fails. (est.) `assoc` and `conj` loops
   to 1 to 3 ns each. Overlaps with 3 for unique values, differs in that it also removes the per-iteration `unique?` and the shell reuse. Do 3 first.
5. **Cheaper `conj` at the leaf boundary**: today every 32nd `conj` allocates a `VLeaf` shell, a new tail array, a new `VecOf` and path nodes
   (about 6 to 10 objects, growing one per decade of size). A tail of spare capacity held in the shell, or the path updated in place under
   `unique?`, is a library-only change (`vec-push-tail`, `vnode-push`). (est.) `conj` 11.5 to ~6 ns; ~5% of the cutter. Small enough to try after 0.
6. **A shorter `nth`**: `vec-nth` + `vnode-get` is already a tail-call loop without retain/release; unrolling the first levels or caching the
   last leaf would save at most a third of 3.4 ns. Not worth a representation change; 3 supersedes it.

## 6. Where each optimisation sits with respect to whole-program visibility

What the compiler does today, from the code and ADR 0024 / `compiled-lir-objects.md` / `shared-libraries.md`:

- The **front end is always whole-program**: typing, ownership checking, specialisation (monomorphisation) and lIR emission "still run on every
  build" over the program and every library module it reaches. Uniqueness facts the compiler has (`compiler/own/lastuse.fib`: a last use hands over
  its count; parameter modes inferred owned per `defun`; shell reuse; `array-take!`) are per function today, but the information of all callers
  is available at that stage. Soundness never relies on it: every in-place primitive tests `fib.unique?` (flags and count == 1) at run time and
  copies if it fails (stdlib.md 2.5, "a wrong guess by the compiler can only cost a copy").
- Only the **LLVM step is partitioned** (16 hash groups + runtime + glue, one cached object each, cross-group calls external). What the cache
  loses is cross-group inlining: the 2 to 3.5x of section 4. It does not lose any ownership or uniqueness fact, because those are computed
  before partitioning. `FIB_LIRO_IMPORT` is the existing partial remedy (copy small callees into the caller's unit).
- **`fibc build --shared`** goes through the whole-program `--emit obj` path, not the partitioned cache (`shared-libraries.md` 3.4): the library
  and every module it depends on are compiled together, everything but the exports is `internal`. So uniqueness inference sees all internal
  callers. The exported entry points are the only unknown callers, and today they accept only scalars, `ptr` and `str` (a `str` is copied
  in; vectors, records, enums, options are refused): no fibber value crosses the C ABI, so the boundary is trivially conservative, and when
  handles arrive (stage N3) a value that crosses is owned and counted by the library and is not unique to a caller's loop.
- **Independently built artifacts** (`--emit liro`, `lair link`) own their runtime tables and type numbers and cannot be mixed by assuming
  numbering; a Fibber library is not today a separately compiled unit with an interface other units optimise against. If it becomes one
  (N1b-style object cache across packages), the unit's exported functions are the boundary.

For the recommendations:

| item | where it works | what separate compilation loses | run-time check |
|---|---|---|---|
| 0. import small callees | LLVM level | it *is* the fix for the cache's loss | n/a |
| 1/2. fusion and recipe inlining | expander (per module, syntactic; the program text of a chain is local) | nothing: the chain and its stage functions are in one form | none needed |
| 3. flat variant | library + runtime layout | nothing semantically; the `unique?` test stays | stays (`fib.unique?` of the flat array), it is what makes a shared `assoc` copy |
| 4. accumulator loop to builder | ownership/lowering, per function for a loop whose initial value is fresh (`[]`, `mapv`, a literal); per call graph (whole linked unit) when the initial value is a parameter | the parameter case: a separately compiled or exported function cannot know its callers, so it keeps the entry `unique?` test and a persistent fallback | entry test O(1) per loop; a link-time pass (or the always-whole front end) can drop it where every internal caller passes a unique value and the function is not exported |
| 5. cheaper `conj` | library | none | stays |

So the persistent-to-mutable optimisations (3, 4) are sound unit by unit with the run-time test they have now, are *fast* in a unit only
when the small primitives inline (hence 0 first), and a whole-linked-unit pass removes the test only for non-exported functions whose internal
callers are all proven unique. An object cache that compiles library packages separately would need an interface annotation per exported
function (parameter owned/unique, result fresh) to keep the facts across the boundary; without it the test is kept at the boundary, as for the
C ABI.

## 7. What was implemented, what was not established

Implemented: the `first` row of T (`compiler/expand/fusetab.fib`, `spec/stdlib.md` line 210), cases 4206 and 4207 (fail on the stage 2 without the row:
41 300 and 1 134 heap objects; pass with it: 3 000 and 12 or fewer), and `scripts/bench/vec-scalar.fib` with its harness. Checks run: stdlib cases
82x, 83x, 42xx and the 43 stdlib cases that mention `first`: all pass; the fixed point (F builds F3, same lIR); golden suites `expand-e1`,
`expand-r`, `expand-porter`; `scripts/bench/quick.sh` before and after: equal within noise. The full gate was not run (instructions).

Not established: the sprite phase's split; how the allerbees session built its 4.2 s binary; why `FIB_LIRO_IMPORT=200 FIB_LIRO_IMPORT_DEPTH=4` is
slow; the 12.4 s pipeline version was not rebuilt; `conj`'s 9.3 objects per leaf was counted, not traced to the lines that allocate; no sampling
profile was possible; every (est.) figure in section 5.

## 8. The flat Vec (recommendation 3): decisions and what was built

Status: built on branch `vec-flat` from `c68837d9`. Measurements are in 8.3; every claim of 8.1 is checked by a case named there.

### 8.1 Decisions

**Representation: a VecOf with no trie, not a new variant.** types §8.3 ("Vector patterns", Decided, owner 2026-09-28) fixes the pattern compiler's
core view of `Vec` as `VecEmpty` or `(VecOf cnt shift root tail)` over `VNode` arrays, element `i` read as `tail[i - (cnt - len tail)]` when
`i >= cnt - len tail`; `rt/vec.lir` (`fib.vec-elem-ptr`, `fib.vec-drop`, `fib.vec-slice-array`), `rt/vecbuild.lir` and `compiler/emit/macros/abi.fib`
read the same view. A `VecFlat cnt arr` variant would change that Decided layout (the spec, the pattern compiler, the runtime and the macro ABI
together). A VecOf whose `root` is `nil` and whose `tail` holds all `cnt` elements, of any number, is already a value of that layout (every Vec of
32 or fewer is one), so every reader handles it unchanged: `cnt - len tail` is 0 and element `i` is `tail[i]`. The invariant is now: `root` nil, the
tail holds every element; `root` some, the tail holds the last 1 to 32 (prelude comment at `Vec`). `nth` needs no new code: for a flat Vec it is the
bounds check, the loads of `cnt`, `tail` and its length, and the array load.

**Element types: all, not scalars first.** The layout does not depend on the element; `array-push!`, `array-set!`, `array-pop!` and `array-take!`
handle counted elements, and the conversion uses `array-copy`, which retains. A scalars-first rule would need a test of the element type that
generic prelude code cannot make, and gains nothing.

**Which builders make a flat Vec.** `vec-from-array` (the array itself, O(1); it copied into leaves), `vec-collect` (the walk's buffer grows by
`array-push!`, doubling, and becomes the Vec's array: `vec` and `to-vec` of a source without spans), `conj` from empty while the Vec is unique,
`mapv`/`vec`/`into []` of a `map` chain over a `range` (`compiler/expand/fuserange.fib` now writes one array of the count and calls
`vec-from-array`), the same over a flat Vec (`fusespan`: one span in, one span out, `vec-from-spans` of one span is `vec-from-array`), `sort`
(`vec-from-array`), and `into` onto a flat Vec (a `conj` loop). Still tries, equally valid, follow-up work: `fuseflow`'s `VecSink` (`filterv`, chains
with `filter remove keep take take-while`), whose spans of 32 are what stdlib §2.1 describes ("spans of 32 filled in place, made the Vec by
`vec-from-spans`"); a `map` chain over a trie (spans of 32 in, spans out); `fib.vec-build` and `fib.vec-drop` of the runtime (vector literals
built at run time, the rest of a vector pattern).

**When a flat Vec becomes a trie.** Once, by `vec-trie-of` (the old body of `vec-from-array`: O(n), 2 objects per 32 elements, elements retained),
when an update of a flat Vec of more than 32 elements finds its array held by someone else: `assoc` (`vec-assoc`), `pop` (`vec-pop`), `update`
and the other users of `vec-take-at`, and `conj` (a flat Vec of exactly 32 makes its array the first leaf, no copy, as before). The result is a
trie and stays one: trie operations never make a flat Vec (a `pop` down to 32 gives a VecOf with no root and a tail of 32, the old small form). The
old version, which someone holds, is untouched and stays flat. A flat Vec of 32 or fewer is today's small Vec and copies its at most 32 elements when
shared. `subvec` and the other views read through `vec-nth`; a rest pattern copies through `fib.vec-drop` into a trie, as before.

**The arbiter.** The test is `fib.unique?` of the array as the update sees it: the array is put in a local `cell` (after the field steal from a dead
owned shell, stdlib §2.5 item 4) and `array-sole?` reads its header (types §8.2: count, and flag bits 0 to 3) through `raw`, `load-i64` and
`load-i32` inside `unsafe`. Unique: `array-set!`, `array-push!`, `array-pop!`, `array-take!` in place. Not unique: the trie. The primitives still make
their own `fib.unique?` test, so a wrong answer of `array-sole?` can only cost a copy (forced true, every shared update copies the whole array and
stays flat: case 4025 fails on the representation and every value case passes). A builtin (`array-unique?`, tried) would be clearer, but the seed
that builds stage 2 (v0.1.17, `SEED`) compiles `lib/prelude.fib`, and a prelude that names a builtin the seed lacks does not build
(`unbound name array-unique?`); the builtin needs a release first.

**Equality, hash, printing, freeze.** `Eq`, `Ord` and `Hash` of `fib.seq.hash`, `Show`/`Debug` of `fib.print`, and every other reader go through
`vec-count`, `vec-nth` and `each-while`, which know nothing of the trie, so a flat Vec and a trie of the same elements are indistinguishable (case
4022 compares `=`, `hash` and `str` across the two after every step). `freeze` marks the array IMMORTAL: `array-sole?` is then false and an update
converts, as `array-set!` would copy. `span-count` of a flat Vec is 1 and `span-array 0` its tail; stdlib §4 (Reducible: "a Vec its leaves and tail,
32 each but the last") holds, with no leaves.

**Memory.** An array grown by `array-push!` has room for up to twice its length (`ROOMY`, types §2.13.1); the trie's overhead was two headers per 32
elements. A Vec built from a known size (`vec-from-array`, `mapv` over a range) is exact.

**Spec sentences that describe the old shapes** (reported, not changed): stdlib §2.1, the in-line rule: "writing a fresh array per span that
`vec-from-spans` makes the Vec without a copy" (over a range it is now one array and `vec-from-array`); stdlib §2.5: "about n/32 allocations" for
`vec` and `into` (now about log2(n/32) for a walk, one for an array); types §2.13.1, "Rule for callers": "with `len >= 32` it copies exactly
`len + 1` elements on every call" contradicts the table above it (doubling since LIBFIX-1) and the code.

### 8.2 What was built and how it is checked

`lib/prelude.fib`: `array-sole?`, `vec-conj`, `vec-assoc`, `vec-pop`, `vec-take-at` (the unique-or-trie choice), `vec-from-array` (flat),
`vec-trie-of` (the conversion), `vec-collect` (one growing array; `VecBuild` and its three functions are gone: 699 lines, from 720).
`lib/fib/seq/protocols.fib`: `span-count` of a flat Vec. `compiler/expand/fuserange.fib`: one array. Cases:

- 4022: a generator of 3000 seeded steps over one lineage: `cur` updated in place while unique, a pool of 16 old versions that forces sharing,
  six builders (two of them tries), `assoc conj pop update mapv filterv`; after every step `cur` and every pool version against an `(Array i64)` model,
  and `=`/`hash`/`str` against the other representation; both representations seen over 32 elements at least 100 times.
- 4023: `mapv` over `(range 100000)` is flat and 1000 unique `assoc` allocate nothing (3 objects; 6462 and result 103125 at `c68837d9`).
- 4024: 100000 `conj` from empty stay flat, 16 objects (29349 at `c68837d9`); 1000 `pop` in place.
- 4025: a shared `assoc`, `conj`, `pop` and `update` of a flat Vec of 1000 each make a trie and leave the old version flat and whole.
- 493 landed (bulk `vec` over 10240 under n/32 + c: 16 objects).
- A Clojure differential outside the suite (`clojure.main` 1.12.0 and the same seeded program in fibber, 2500 steps and 24 kept versions up to 1490
  elements): the 10 673 253 bytes printed are identical.

### 8.3 Measurements

Compilers: `F-main` is the gate's stage 2 of `c68837d9` (with that tree's `lib/`); `F` is this branch's stage 2, built by `F-main` (with this
branch's `lib/`). Default builds are partitioned; machine load 3 to 8 on 28 cores during the runs (other workers). Medians of 3.

`scripts/bench/vec-scalar.sh -n 1`, three times per row (`BENCH_FIBC`, `BENCH_BUILD_FLAGS`; picoseconds per element, the checksums equal
throughout and between twins):

| row | default main | default flat | whole main | whole flat |
|---|---|---|---|---|
| `nth` i8 / i64 / f64 | 3 898 / 3 528 / 3 628 | 1 415 / 1 147 / 2 797 | 3 541 / 3 436 / 3 449 | 1 174 / 1 221 / 1 547 |
| `array-get` twins | 635 / 502 / 499 | 615 / 436 / 444 | 218 / 465 / 479 | 225 / 418 / 467 |
| `assoc` i64 / i8 / f64 | 74 448 / 70 460 / 73 349 | 19 034 / 19 488 / 20 160 | 20 061 / 20 731 / 20 853 | 3 921 / 4 034 / 4 031 |
| `conj` i64 / i8 | 23 875 / 23 546 | 20 399 / 17 758 | 11 610 / 11 826 | 6 605 / 4 712 |
| `mapv` i64 | 3 220 | 1 159 | 2 985 | 1 138 |
| `(reduce + 0.0 (mapv f (range -3 4)))` | 104 064 | 78 963 | 62 550 | 45 187 |

`nth` is 3x faster, `assoc` 4 to 5x, `mapv` 2.7x. `assoc` and `conj` in the default build are still calls across units (`array-sole?`,
the primitives); `conj` gains least because it already pushed into a tail in place and now also asks `array-sole?`.

`scripts/bench/quick.sh -n 3` (`BENCH_FIBC`, `GATE_OUT` in scratch): no wrong answer; `vec-index` 0.48 to 0.16 s; the rest within noise
(rerun pinned with `taskset -c 20-23 ... -n 5`, twice: `vec-sort` 0.89/0.94 to 0.80/0.79, `vec-conj-pop` 2.23/2.42 to 2.18/2.13, `set-conj`,
`map-assoc-get`, `lazy-fused` equal within the spread of the runs).

The allerbees cutter (`tools/cut-sprites.fib` at `74c7d82`, copied; `sheet-healer.png`, `bee-healer happy sneezy resting --size 360`; wall and
peak RSS by `/usr/bin/time`): the three webp files are byte-identical in all 12 runs.

| build | main | flat |
|---|---|---|
| `--whole-program` | 2.60 s (2.58 2.60 2.62), 191 MB | 1.84 s (1.84 1.81 1.89), 159 MB |
| default | 3.96 s (3.96 3.98 3.76), 190 MB | 3.12 s (3.12 3.18 3.00), 150 MB |

-29% and -21%, and 30 to 40 MB less: the trie's leaves and their headers are gone. The estimate of section 5 (4.3 to 2.3 s from a default build
of the older cutter) was not reached: this cutter already writes its hot loops by hand, and the default build still calls across units.

The compiler (its Vecs are small or built by `conj`): `emit compiler/fibc.fib` 33.87 s main and 33.88 s flat, peak 1.78 and 1.77 GB; a build
of `compiler/fibc.fib` with `FIB_LIRO_CACHE=off`, 131.35 s and 131.57 s, peak 2.83 and 2.81 GB; the stage 3 it builds 14 881 952 and 14 786 176
bytes. No change outside the noise.

Checks run: all three case directories with `F` (ownership 377 pass, modules 32 pass, stdlib 1516 pass and 17 open, the same open set as main's
gate, 493 landed); `scripts/gate.sh --quick` with stage 2 built by the v0.1.17 seed: PASS; the fixed point (F builds F3, `F emit
compiler/fibc.fib` equals `F3 emit compiler/fibc.fib`, sha1 575cab12); the golden suites (expand-e1, expand-r, expand-porter regenerated:
gensym numbers only, checked by renumbering both sides, and for the 60 porter inputs by running the old and new `expand` tools); the
in-line vectorisation check (`compiler/tests/expand/fuseinline/check.sh`); the wasm stage (374 of 377 ownership cases, the 3 expected
failures, modules 32); the JS backend check; `make adr` (27 accepted). Mutants of the prelude (`array-sole?` forced true, forced false;
`vec-trie-of`'s tail one element off) and of `span-count` (a flat Vec counted in 32s) each fail at least one of 4022 to 4025.

Not done, in order of use: `VecSink` flat (filter chains; needs the stdlib §2.1 sentence changed), a `map` over a trie written into one
array, the runtime's `fib.vec-build`/`fib.vec-drop` flat, and `array-sole?` as a builtin once a released seed has one.
