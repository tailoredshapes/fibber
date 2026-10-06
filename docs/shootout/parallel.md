# fib.parallel (P-struct): measurements

Machine: 28 CPUs, `cpu-count` = 28 (sysconf 28, affinity 28, no cgroup quota file). Compiler: the v0.1.6 seed's `fibc run`, library from this tree
(`-I lib`). Every run is under `ulimit -v 16000000`, `MALLOC_ARENA_MAX=2` and `flock /tmp/fibsuite.lock`; one run each (not medians: the numbers
below are single runs on a machine shared with other agents, so read them to within 10 to 20%). The programs are in `docs/shootout/parallel/`; the command
is `fibc run docs/shootout/parallel/NAME.fib -I lib` from the repository root.

## pmap over 1e6 trivial items `(* x 2)` (`pmap-trivial.fib`)

| Variant | ms |
|---|---|
| prelude `pmap` (thread per element), 100 000 items (`pmap-prelude-baseline.fib`) | 872 (8.7 us per item; 1e6 was 8.4 s in docs/design/parallelism.md 1) |
| sequential Vec loop (`conj`) | 57 |
| `pmap-each`, 1 worker (no task, one chunk) | 19 |
| `pmap-each`, 8 workers | 9 |
| `pmap-each`, 28 workers (the default) | 7 to 10 |
| `pmap` with a closure made by the caller, 28 workers | 47 |
| pre-sized array written through `run-tiles!` and raw stores, 8 tiles, then `vec-from-array` | 6 |
| the same, array only, 28 tiles | 3 |

Results of all variants equal the sequential result (the last line of the program prints `equal true ...`).

Why the closure form was slower: `f` of `(pmap f xs)` is one closure shared by W threads, and a closure call retains the closure and the body releases it, so every
call touched its atomic reference count. Measured directly (a chunk function that calls a shared closure per element, 1e6 elements, 8 workers): 60 to 65 ms with the
closure, 3 ms with the same arithmetic written inline. This is the refcount-contention hazard of section 2.5 of the design. **P-count-b fixed it in the library**
(next section): each chunk calls a `private-copy` of the closure. The table above is the v0.1.6 measurement and is kept as it was.

## The closure forms after P-count-b (`scripts/bench/parallel-closure.sh`)

Command: `FIBC=F scripts/bench/parallel-closure.sh "pmap pmap-each preduce preduce-n preduce-r" W 10000000` (ulimit -v 16000000, MALLOC_ARENA_MAX=2, under
`flock /tmp/fibsuite.lock`; ms, median of three rounds; 1e7 elements; the machine is shared, so trust ratios of 2x and more). Before: the tree at 789aa92 (library
without `private-copy`), after: this tree, the same compiler binary. The checksums of every row are the same before and after.

| row | W=8 before | W=8 after | W=28 before | W=28 after |
|---|---:|---:|---:|---:|
| `(pmap p f xs)`, closure `(* x 2)` | 470 | 126 | 539 | 105 |
| `pmap-each` (body made in the chunk) | 114 | 115 | 91 | 98 |
| `(preduce p + 0 f xs)` | 385 | 25 | 452 | 12 |
| `(preduce-n p + 0 f n)` | 367 | 7 | 361 | 5 |
| `preduce-range` (macro) | 2 | 3 | 3 | 2 |

The closure `pmap` was 5.9x and 5.3x slower than `pmap-each` (W=28, 8) and is 1.07x and 1.1x now; `preduce-n` was 180x slower than the macro at W=28 (361 against 2 ms)
and is 2.5x now (5 against 2 ms, the per-element call that remains). The `*-each` forms did not change. The `:scoped` colour was not built: the closure rows are within
the 2x criterion of the design without it (`pmap`) or are a per-element call of a closure (`preduce-n`, which has no macro-free form faster than a call).

The files `pmap-trivial.fib`, `pmap-compute.fib` and `preduce-1e8.fib` of `docs/shootout/parallel/` were written for v0.1.6 and `pmap-trivial.fib` stopped compiling
(the tile function of `run-tiles!` takes a fourth argument, the seed element, since the P-race fix: `fibc` said `cannot unify (fn (a i64 i64) unit) with (fn :send
((Array i64) i64 i64 i64) unit)`). The rows above come from the program that replaced them for the closure question. **2026-10-06:** `pmap-trivial.fib` is fixed
(`(fn (arr lo hi seed) ..)`), the other two compiled unchanged, and all three are re-run in the next section; `compiler/tests/shootout-compile.sh` (in the full
gate's tools stage) builds every program of this directory so that the next API change shows.

## The three programs again, 2026-10-06 (the tables above are kept as they were)

Compiler: stage 2 built from this tree by the v0.1.7 seed; library from this tree (`-I lib`). Commands, from the repository root: `F build docs/shootout/parallel/NAME.fib
-I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o OUT`, then `ulimit -v 16000000; MALLOC_ARENA_MAX=2 flock /tmp/fibsuite.lock OUT`. One run each, on the shared
machine, after the unit-test survey had finished: read them to 20%. Every `equal` line is true, and the values of `preduce-1e8` are bit for bit the recorded ones
(-7.142857139339605 sequential, -7.142857140969321 for the fixed tree at every worker count and for the closure and macro forms).

| program | row | ms, v0.1.6 table | ms, now |
|---|---|---:|---:|
| `pmap-trivial` | sequential Vec loop | 57 | 17 |
| | `pmap-each`, default workers (28) / 1 / 8 / 28 | 7 to 10 / 19 / 9 / 7 to 10 | 9 / 8 / 6 / 4 |
| | `pmap` with a shared closure | 47 | 4 |
| | tiles array, 8 tiles, then `vec-from-array` / 28 tiles, array only | 6 / 3 | 4 / 1 |
| `pmap-compute` | sequential | 363 | 181 |
| | `pmap-each` 1 / 2 / 4 / 8 / 16 / 28 workers | 344 / 184 / 100 / 64 / 37 / 37 | 171 / 125 / 57 / 36 / 20 / 52 |
| | `pmap` (shared closure), 28 | 34 | 22 |
| `preduce-1e8` | sequential scalar loop | 314 to 331 | 53 |
| | fixed tree, 1 / 2 / 4 / 8 / 16 / 28 workers | 322 to 330 / 171 to 189 / 106 / 65 to 69 / 36 to 38 / 33 to 34 | 61 / 34 / 32 / 24 / 39 / 20 |
| | `preduce-range`, 28 | 54 | 19 |
| | `preduce-n` (closure per element), 8 | 4855 to 4891 | 70 |

The ratios agree with the sections above: the shared-closure `pmap` is no longer slower than `pmap-each` (4 ms against 4 to 9), and `preduce-n` at 8 workers went from 75x
slower than its chunk loop to 3x (70 against 24). The absolute times fell by 2 to 6x for the sequential loops, which is the compiler of today and not the library; the
28-worker `pmap-each` of `pmap-compute` (52) is a noisy single run on a machine with other agents (16 workers gave 20).

Per-chunk Vec results concatenated with a `conj` loop cost 60 ms for 1e6 elements; a chunk result as an array (filled at 3 ns an element) concatenated
into one array and turned into a Vec with `vec-from-array` (5 ns an element) costs 3 + 5 ms. That is what `pmap` does. Writing straight into one pre-sized array
(the `tiles` row) saves the concatenation, 2 to 3 ms of 9, and only works for the element types of `ViewElem` (f64, f32, i64, i32), so it is not in the library.

## pmap over 1e6 elements, 60 sqrt steps each (`pmap-compute.fib`)

| Variant | ms | speedup |
|---|---|---|
| sequential | 363 | 1.0 |
| `pmap-each`, 1 worker | 344 | 1.1 |
| 2 | 184 | 2.0 |
| 4 | 100 | 3.6 |
| 8 | 64 | 5.7 |
| 16 | 37 | 9.8 |
| 28 | 37 | 9.8 |
| `pmap` (shared closure), 28 | 34 | 10.7 |

## preduce of 1e8 f64 (`preduce-1e8.fib`)

Data `((11 i mod 101) - 50) / 7` in an `(Array f64)`; `pfold-chunks` with a scalar chunk loop.

| Variant | ms | value |
|---|---|---|
| sequential scalar loop | 314 to 331 | -7.142857139339605 |
| fixed tree, 1 worker | 322 to 330 | -7.142857140969321 |
| 2 | 171 to 189 | same bits |
| 4 | 106 | same bits |
| 8 | 65 to 69 | same bits |
| 16 | 36 to 38 | same bits |
| 28 | 33 to 34 | same bits |
| `preduce-range` (macro), 28 | 54 | same bits |
| `preduce-n` (closure per element), 8 | 4855 to 4891 | same bits |

The tree's value differs from the sequential left to right loop's (a third value, as design 2.7 found): determinism needs the sequential path on the same tree,
which the 1 worker row is. Against the design's numbers (2.7: sequential `sum-fast` 48.2 ms, fixed-grain tree 38.0 at 1 worker, 17.0 at 28 workers, which are SIMD
chunk kernels): here the chunk kernel is a scalar loop, so the sequential row is 330 ms and the 28-worker row 33 ms, 9.7x. A chunk kernel that is `fib.tensor`'s
`sum-fast` on a window would be the design's 48 to 17 ms; `pfold-chunks` takes any such function. This is NOT measured here (`fib.tensor` is not used by this
package). The closure-per-element `preduce-n` is 75x slower than its chunk-loop form at 8 workers: two shared closures (`combine` and `f`) are touched per element.

## Determinism

Case 7609 runs `preduce` and `pfold-chunks` over 400 003 inexact f64 values at 1, 2, 3, 4, 7, 8, 16 and 28 workers and compares the bits with each other and
with a reference tree written out with plain loops. The `:fast` mode (one chunk per worker, left to right) fails the same check, which the case records.
