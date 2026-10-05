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

Why the closure form is slower: `f` of `(pmap f xs)` is one closure shared by W threads, and every call to it touches its atomic reference count. Measured
directly (a chunk function that calls a shared closure per element, 1e6 elements, 8 workers): 60 to 65 ms with the closure, 3 ms with the same arithmetic written
inline. A body made inside the chunk (`pmap-each`, `pmap-range`, `pfor-range`, `preduce-each`, `preduce-range`, or a chunk function of `pmap-chunks` and
`pfold-chunks`) never shares a closure. This is the refcount-contention hazard of section 2.5 of the design (P-count-b: `freeze` and borrowed reads is the fix);
nothing in `lib/` can remove it. For a body that costs a microsecond it is noise (the next table).

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
