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

## P-sched: the work-stealing pool (2026-10-07; docs/design/parallelism.md section 8)

Before = main at 828b88e (`gate-main-gate48` compiler, library of that commit): W OS-thread tasks with a fixed stride, nested calls multiply. After = this tree: the pool.
Programs: `docs/shootout/parallel/sched-rows.fib` (rows on both backends) and `sched-fork.fib` (rows that need `fork-task`). Median of 5 runs after a warm-up run, `ulimit -v 16000000`,
`MALLOC_ARENA_MAX=2`; for the pool `FIB_THREADS=W` as well as the `Par` of the call. Every checksum is identical across W and across before/after.

### Final numbers: Ryzen 7 5800X (8 cores, 16 threads, WSL2), x86-64-v3 binaries built on the dev box and run there, quiet machine (load 0.00)

ms, W = workers (1, 2, 4, 8, 16). Raw: `bench-ry` of the session scratch (`~/.cache/fibber-scratch/PSCHED/bench-ry.tsv`); command `RUNS=5 bash bench.sh . "1 2 4 8 16"`.

| row | n | | W=1 | 2 | 4 | 8 | 16 |
|---|---:|---|---:|---:|---:|---:|---:|
| `preduce-range` f64 sum | 1e8 | before | 212.8 | 107.9 | 81.6 | 50.0 | 47.1 |
| | | after | 212.3 | 107.6 | 56.1 | 32.9 | **28.8** |
| `pmap-range` trivial body | 1e6 | before | 6.7 | 8.3 | 8.5 | 8.2 | 8.8 |
| | | after | 6.9 | 8.6 | 7.9 | 7.9 | 7.4 |
| `pmap-range` trivial body | 1e7 | before | 91.6 | 102.1 | 98.1 | 102.1 | 93.0 |
| | | after | 94.4 | 102.3 | 95.6 | 93.0 | 91.4 |
| `skew`: 2e4 items, one in eight 3x heavier | | before | 31.0 | 16.5 | 8.5 | 4.8 | 4.8 |
| | | after | 30.7 | 15.6 | 8.1 | 4.6 | **2.8** |
| `nested`: pmap of pmap of pmap, 64 x 64 x 64 | | before | 2.1 | 352.4 | 652.1 | 1344.1 | 2702.3 |
| | | after | 2.2 | 14.9 | 39.3 | 124.0 | **190.7** |
| `pfor-range` allocating an 8-Vec and `swap!` on a shared atom (see below) | 1e6 | before | 108.8 | 163.3 | 185.1 | 212.0 | 235.4 |
| | | after | 108.7 | 172.2 | 187.0 | 220.8 | 342.8 |

Reading it. `preduce` scales 7.4x at 16 (before: 4.5x): the old strided tasks are 16 equal chunks and one slow thread holds the call; the pool balances. The `skew` row is the over-decomposition
recovery of design section 2.4 (4.8 to 2.8 ms at 16; the static chunks cannot move work off the slow chunk). `nested` is the W x W problem: before, W=2..16 made W^3 threads and a call
at W=28 on the dev box ended in `trap: spawn: cannot start a thread`; after, the thread count is the pool's (case 8642 reads /proc/self/task). `pmap` of a trivial body is memory-bound
(allocation and the result copy) and flat in both.
The `pfor-alloc` row is slower after at 16 workers (343 against 235 ms). It is not the pool: the body allocates **inside the function of `swap!`**, which re-runs on every compare-and-set
retry, so contention multiplies the allocation; the faster runners retry more. Without the shared atom (`pmap-range` that only allocates) 1e6 items take **35 ms at 28 workers on the dev box
under both backends** (probe `alloc-probe.fib` of the session scratch: 37 old, 35 new; with the atom 470 old, 639 new), so the allocator under the pool is not the limit (below).

### The cost of a task (`sched-fork.fib`), Ryzen

| row | result |
|---|---|
| fork + join of a trivial task inside a pool task, 1 worker (`FIB_THREADS=1`) | **72 ns** per round (the design estimated 60 to 100; `async` is 31 ns) |
| the same, 16 workers (idle thieves steal the task as soon as it is pushed) | 651 ns |
| the same from the main thread (through the injector), 16 workers | 897 ns |
| 1e5 tasks forked before any is joined, 16 workers | 650 ns per task |
| fork-join `fib(38)` with a sequential cut at 20: 1, 4, 8, 16 workers | 223.3, 58.7, 31.3, **21.7 ms** (10.3x at 16 threads on 8 cores) |

How this was reached, because the first version was 20 times worse on this machine: with the wake-up rule of the design as first built (a fork signals when `sleepers > 0`) an immediate fork-join with idle workers cost **14.1 us**
on WSL2 (the futex wake of every fork; 1.1 us on the dev box). `FIB_SCHED_SKIP` (default 1: a fork does not signal while some worker is still in its spin phase, the Rayon rule) takes it to 0.65 us. The matrix
(Ryzen, ns per round, storm / main / wide): spin 32 skip 0: 14359 / 14581 / 14654; spin 32 skip 1: 668 / 888 / 641; spin 4 skip 1: 4606 / 102066 / 4608; spin 0: 14097 / 229221 / 14638; spin 300 skip 1: 657 / 1012 / 1556.
So 32 rounds of spinning (`FIB_SPIN`) with the skip is the default. A joiner also looks at the task's state 3000 times before it parks (the main-thread round on the dev box went from 8.2 us to 1.5 us). Dev box (28 CPUs, load average 12 to 23 from other work,
so only ratios mean something): fork+join 84 to 110 ns with one worker, about 1 us with 28.

### Scaling on the dev box (i7-14700KF, 28 logical CPUs, shared and loaded: read to 20%), ms

| row | | W=1 | 2 | 4 | 8 | 16 | 28 |
|---|---|---:|---:|---:|---:|---:|---:|
| `preduce` 1e8 | before | 180.2 | 106.8 | 65.4 | 38.1 | 27.2 | 23.6 |
| | after | 176.6 | 91.4 | 46.4 | 27.0 | 19.2 | **17.5** |
| `skew` | before | 31.8 | 20.7 | 12.3 | 6.0 | 3.6 | 3.8 |
| | after | 33.3 | 18.8 | 8.1 | 5.0 | 2.7 | **1.8** |
| `nested` | before | 1.8 | 78.7 | 137.2 | 426.3 | 874.5 | **trap: spawn: cannot start a thread** |
| | after | 1.8 | 18.8 | 53.6 | 216.4 | 377.0 | 353.5 |
| `pmap` 1e7 | before | 79.7 | 105.0 | 88.0 | 84.9 | 83.7 | 83.8 |
| | after | 79.1 | 89.7 | 83.0 | 79.8 | 80.2 | 79.4 |

(The `seq` row of the same run is a plain loop with no task and moved from 2.5 to 15 ms between two builds of the same loop; on this loaded hybrid-core machine it is noise, which is another reason the Ryzen is the final table.)

### The allocator under the pool (ADR 0018 unchanged)

* Allocation itself under the pool is fine: 1e6 allocating items on 28 workers, 35 ms (above). No allocator change was made; the large-block cache, its decay and cases 7791/7792 are untouched and pass in the gate.
* JSON-3's probe `scripts/bench/json/mt-mode.fib` (twitter.json DOM parse and drop, four CPUs, MB/s): single-thread mode 367 / 359 before, 446 / 449 after (build to build noise); after the first `pmap` (multi-thread mode) 223 / 265 before,
  264 / 270 after. **The 40 percent cost of the multi-thread mode is not changed by the pool**, as the design (allocator.md section 10) said: it is the slow path of `fib.alloc` / `fib.free-block` once `fib.mt` is set (a call, `fib.lc-tick`, `malloc`/`free`) and the atomic counts, not the scheduler.
  The cheap part of the fix is not cheap: a per-worker small-block cache needs a thread-local in lIR (a `pthread_getspecific` call per allocation costs what it saves) and a block made on one worker is freed on another (a chunk made by a thief, dropped by the joiner), so it needs a bounded hand-back. Not built.

### Start-up, hello world

`scripts/bench/hello.fib`: binary 36 872 bytes after, 36 952 before (the unused pool is dropped by the linker; the emitted lIR text is 40 KB larger, 165 506 to 205 971 bytes, all of it runtime source). A strace of the program shows one `clone3`, the main-stack thread
that existed before: no worker is started until the first `fork-task`.

### Planted faults, TSAN, stress (`scripts/mutant-sched.sh`, `scripts/tsan-planted.sh pool-handoff`)

| planted fault | result |
|---|---|
| M1 steal without the compare-and-swap on `top` | killed: 8640 and 8645 crash (exit 139); `wide 20000` crashes; `tree 18` answers right |
| M3 pop of the last item without the race for it | killed: 8640 and `tree 18` crash; 8645 and `wide` answer right |
| M2 pop's fence `seq_cst` weakened to `release` | **survives** on x86, as the design's experiment found (a weakened fence needs a model checker): run and reported, not claimed |
| lost wakeup (the eventcount's re-check removed) | killed by a hang under a 60 s timeout with `FIB_THREADS=1`; with 2 workers 2e5 rounds did not hang (the window is narrow) |
| completion's `seq_cst` fence removed (a joiner registers after a completion saw no waiter) | killed: hangs at 2 and at 4 workers |
| join neither runs the task nor helps (the design's "deadlock" mutant) | killed: 8640 fails, `tree 16` crashes. (Parking instead of helping alone does *not* deadlock: a join that runs its own unstarted child is deadlock-free by itself.) |
| a thief runs a task without claiming its driver | killed: 8640, 8645, `tree 18` |
| the runner completes the task before it stores the result | killed by 8640 (wrong sum); 8647 survives |
| planted TSAN race: ring slot published and read with plain accesses | reported (`atomic read fib.sched-wake-waiters | write malloc`, x43 in 3 runs), unplanted run silent |

TSAN (`scripts/tsan.sh --check`, 3 runs each, baseline stays empty): the seven pool kernels (`scripts/tsan/sched.fib`: tree, storm, wide, pmap, nested, trap, blocking), the older threaded kernels (async, pmap, fan, chain, trap, atom, many) and ownership cases 24, 32, 44,
174, 233: no report. Two things TSAN made me change: it does not model standalone fences, so the deque's ring slots are stored with release and loaded with acquire as well as the paper's fences (the same machine code on x86), and
the waiter list is read by `fib.sched-wake-waiters` outside its lock, so its stores are atomic. Stress: 300 runs (`storm`, `wide`, `tree`, `trap`, `nested` at 2, 3, 5 and 28 workers, 60 s timeout each): 0 hangs, 0 wrong answers.
