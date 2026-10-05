# Parallelism for fibber: a work-stealing scheduler, structured scopes, parallel tensors, cheap shared counts

Status: **design with measured prototypes; nothing here is implemented in the compiler, runtime, library or spec.** Every number was produced in
this session on the toolchain of `main` at 455a85e (stage 2 built from that tree, tree stamp `3c81037761caea7c`), with the commands quoted. The
prototypes are in `docs/design/parallelism/proto/` (fibber programs `p1`..`p7`, C `sched.c` and `rc.c`, lIR `cl.lir`, the scripts that ran them);
the raw outputs are in `docs/design/parallelism/results/`. The owner's words: "I also want best in class parallelism". Goal context: destroy Python at AI and
scientific work (no GIL, no C extensions, deterministic memory without a GC), fast like Rust and Java, ergonomic like Clojure (tie-breaker: Clojure's
ergonomics unless they break memory safety, then Rust's).

Contents: 0 recommendation; 1 what exists (read from the runtime); 2 measurements; 3 design; 4 comparison; 5 staged plan; 6 what was not done.

---

## 0. Recommendation in one page

1. **Today's parallelism is real but coarse, and two of its pieces are traps.** `spawn` is one detached OS thread per task (9.8 us spawn+join on
   one pinned CPU, 15 us unpinned); `pmap` and `plet` are `spawn` per element (**8.4 s for `pmap` over 1e6 trivial items**, against about 3 ms sequentially); `async` is
   not parallel at all: it is a lazy state machine that the awaiter drives on its own thread (31 ns, which is why it is cheap) and the pool is only used when an
   async task parks on another. Chunked `spawn` tasks do scale: a reduce over 1e8 elements is 4.5x at 16 workers, a recursive `fib(35)` 5.7x at 28, a mergesort of 1e7 3.0x,
   `with-tiles` on a compute-bound kernel 10.7x at 28 (3.7x on 4 tasks). Nothing scales with the single mutex run queue once tasks are small (section 2.3).
2. **Replace the run queue by per-worker Chase-Lev deques plus one injector queue, run structured tasks child-first with help-while-waiting joins, and keep `spawn`
   as the thread tier.** Measured on a C transcription of both designs: with 2.2 million forks of fib(40) the mutex queue takes 1796 ms at 28 workers and the
   deque scheduler 132 ms (13.6x); with coarse tasks they are equal; uncontended push+pop is 15 ns for both, so single-thread code pays nothing. A Chase-Lev deque is 42 lines
   of lIR (`cl.lir`); its stress test finds a planted missing compare-and-swap at once and a planted weakened fence in about half the runs on x86.
3. **The hidden cost of parallel sharing is the reference count, and it is far larger than scheduling.** A retain+release pair on a SHARED object costs 19.7 ns
   alone, 973 ns with 28 threads on one object (about 100x a plain count) and 443 ns with 28 threads spread over 64 objects; on the M1 Ultra 3.6 us at 20 threads. The same loop
   on an IMMORTAL/frozen object stays flat (8 ns at 28 threads). In fibber terms (`p2`): 28 threads reading one shared node: 5934 ms; the same on a `def` (immortal) node: 260 ms;
   private copies: 237 ms. **Atom reads are worse: 28 threads reading one `(atom 0)` take 1104 ms for 200 000 reads each, 700x slower per read than one thread**, because `@a` takes a spinlock
   even for a scalar. Biased counts do not help read-mostly sharing (every reader is a non-owner). Recommended: scalar atoms lock-free; `freeze` (publish a graph as immortal);
   scoped task captures that borrow instead of count; stop `(. (vec-nth v i) f)` taking a count; no per-thread count caches (they delay frees; section 3.6).
4. **Determinism is cheap.** A reduction with a fixed grain (64 Ki elements) and a fixed pairwise tree gives the same bits for 1, 2, 3, 4, 7, 8, 16 and 28 workers, and was as
   fast as the naive chunk-per-worker one within noise (17.0 ms against 23.7 ms at 28 workers, 1e8 elements). The naive one differs in the 10th digit between worker counts.
   Matmul and elementwise kernels partitioned by rows are bit-identical for any worker count by construction.
5. **Order.** (a) P-race first (S): a ThreadSanitizer pipeline over lIR output exists and works (section 2.8); it found two real races in the existing runtime and library in its first run.
   (b) P-struct on the **current** runtime (chunked tasks over `spawn`; the owner's guess is right and measured: 4 to 5x on a reduce today): `pmap` that does not spawn per element, `pfor`, `preduce`,
   `with-tasks`. (c) P-count's atom fast path (S), because every `swap!` in a parallel body collapses at 8 threads. (d) P-sched behind the same API, then P-tensor, P-chan, P-det, the rest of P-count.
   Language changes are few (section 3.7): none for the first two packages; a scoped-closure colour for borrowed captures; a tile-offset binding for `with-tiles` (library); an interrupt flag
   for cancellation (runtime); trap isolation for pool tasks waits for exceptions stage 2.

---

## 1. What exists today (read from `rt/task.lir`, `rt/thread.lir`, `threads.fib`, `lib/`)

| Piece | What it is | Cost / limit |
|---|---|---|
| `spawn` / `future` / `plet` | `fib.share` the closure, allocate a task (count 2), `pthread_create` a **detached thread** per task (`fib.spawn`); the thread entry runs the closure, completes the task, releases | 9.8 us spawn+join (pinned), thread per task; `fib.threads-live` and `join-all` under the run-queue mutex; 16 GB address cap: a fork-join of 1024 spawns (`pfib` depth 10) failed (`spawn: cannot start a thread`), 256 (depth 8) passed |
| `async` / `await` / `join` | heap state machine (`resume` function, locals saved in the task); **created pending and never queued**: `join`/`drive` claims the task (`cmpxchg driver 0 1`) and runs it on the joiner's stack; `await` of a pending task queues it and parks the awaiter | 31 ns per `(join (async i))`; no parallelism unless an async task awaits another and a worker takes it |
| the pool | `fib.pool-start` on the first enqueue: `sysconf(84)` (`_SC_NPROCESSORS_ONLN`) workers, **one list, one mutex, one condition variable, `broadcast` at every enqueue and every completion**, malloc'd 16-byte nodes | `sysconf(84)` ignores affinity and cgroup quotas; the contention is section 2.3 |
| `pmap` (prelude) | `spawn` per element, then `join` in order | 8.4 us per element |
| `with-tiles` (`lib/fib/view/tiles.fib`) | `k` tiles = `k` `spawn`ed tasks, each builds its own `Win` from the array and a range; unique-write protocol once, then raw stores to disjoint ranges | the **body cannot learn its tile offset** (the macro binds only `w`), so it cannot read the matching input range: it can fill, not map |
| share marking | `fib.share` walks the closure and everything reachable at `spawn`, setting SHARED (atomic `or`), stopping at SHARED/IMMORTAL | 9.6 to 11.5 ns per object, once; later spawns of the same graph are 9 to 40 us (the thread) |
| counts | header `(i64 count, i32 type-id, i32 flags)`; flags SHARED (atomic count), STACK and IMMORTAL (no count, retain/release skip after one flag load); `fib.retain` fast path non-atomic | section 2.5 |
| atoms | header, a **spinlock word**, the value; `@a` = lock, load, retain, unlock; `swap!` = snapshot, `f`, lock, compare, store, unlock, retry unbounded | section 2.6 |
| traps in tasks | stage 1 of exceptions: a trap on a `spawn` thread ends that task, `try-join` returns `(Err (Trap msg))`; a trap in a pool worker still aborts the program | isolation needs the pthread entry |

Two facts shape everything below. First, `async` today is a *coroutine*, not a fork: its cheapness comes from running inline on the joiner, one `cmpxchg` on the driver word and
no thread, queue or lock; what limits it is that creation does not make parallelism. Second, spin-wait programs (cases 166 to 168: a task that waits by reading an atom another
task sets) need preemptive threads, which a fixed pool of run-to-completion tasks is not; so a pool tier cannot replace `spawn`, it can only sit beside it.

---

## 2. Measurements

### 2.0 Method

```
# stage 2 of main 455a85e: copy of ~/.cache/fibber-scratch/gate-main-gate6/F (its F.stamp is the tree stamp 3c81037761caea7c, checked by hashing the tree)
ulimit -v 16000000 ; export FIB_LIB=$PWD/lib ; F build pN.fib -o pN            # p1.. in docs/design/parallelism/proto
flock /tmp/fibsuite.lock taskset -c SET ./p1 KERNEL N W RUNS                    # run 0 is a warmup and is dropped; the table is the median of the others
```

CPU sets for W workers (one logical CPU per core while possible): `1: 0`, `2: 0,2`, `4: 0,2,4,6`, `8: 0,2,..,14`, `16: 0-15` (8 P cores, both SMT threads), `28: 0-27` (adds the 12 E cores).
Machine: i7-14700KF, 8 P cores (cpu 0-15, `/sys/devices/cpu_core/cpus`) + 12 E cores (16-27, `cpu_atom`), 1 NUMA node, 33 MB L3. **The machine was shared**: other agents'
compile and gate jobs (which do not all take the lock) ran during every measurement, load average 4 to 20, swap full. Expect 20% noise on a row; ratios of 2x or more are safe,
differences of 10% are not. Every parallel result is checked against the sequential one (`v=` column in the raw files; equal in every row except the `for` kernel, whose value depends on the tile
index by construction). The `ulimit -v 16000000` cap was kept for all runs; the only runs that hit it are named. Scripts: `all.sh`, `all2.sh`, `all3.sh`, `all4.sh`, `p2run.sh`, `p7run.sh`, `schedmat.sh`, `rcmat.sh`.

### 2.1 The cost of a task

| What | Command | Result |
|---|---|---|
| spawn + join of `(fn () i)`, 20 000 times, 1 CPU | `taskset -c 0 ./p1 spawnjoin 20000 0 4` | 196.6 ms: **9.8 us each** (unpinned and loaded: 301 ms, 15 us; documented 27 us) |
| `(join (async i))`, 1e6 times | `./p1 asyncjoin 1000000 0 4` | 31.3 ms: **31 ns each** |
| `pmap` of `(+ x 1)`, one thread per element, 28 CPUs | `./p1 pmap N 0 2` | N=500: 4.6 ms; 4000: 34.5 ms; 20 000: 0.17-0.19 s; 100 000: 0.82-0.87 s; **1 000 000: 8.1-8.6 s** (it completes under the cap because finished threads detach; 8.4 us per element) |
| first `spawn` capturing a Vec of N nodes | `taskset -c 0 ./p6 N` | N=1e6: 9.6 ms, i.e. **9.6 ns per object once** (11.5 ms in an earlier run); N=1e4: 0.11 ms; repeats: 9 to 40 us |
| sequential comparison | `taskset -c 0 ./p1 seqsum 10000000 0 5` | 10.2 ms for 1e7 elements: a trivial item costs 1 ns, so `pmap`'s 8.4 us per element is 8400 items of work |

### 2.2 Scaling of today's patterns (tasks on `spawn`, W tasks on W CPUs)

Medians, ms. `seq` is the same kernel without tasks, pinned to one CPU. Speedup = seq / W. Commands: `./p1 KERNEL N W RUNS` (`sum`, `map`, `pfib`), `./p3 N DEPTH RUNS` (mergesort), `./p4 mm|fma N W RUNS`
(row blocks of `fib.tensor` `mmul` on a `slice`), `./p7 N W H RUNS` (`with-tiles`, owner made outside the timed region).

| Kernel | seq | W=1 | 2 | 4 | 8 | 16 | 28 | best speedup |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| reduce 1e7 (`sum`) | 10.2 | 9.4 | 4.7 | 2.6 | 2.8 | 2.5 | 2.2 | 4.6x |
| reduce 1e8 | 102.9 | 92.4 | 49.8 | 39.5 | 32.3 | **20.5** | 25.4 | 5.0x at 16 (800 MB in 20 ms: 40 GB/s, bandwidth bound: inference) |
| map 1e7, chunk tasks return arrays, main concatenates | 27.1 | 48.5 | 41.6 | 40.4 | 36.4 | 36.8 | 32.7 | **0.83x: never wins** (alloc and copy) |
| `with-tiles` store only, 1e7 f64 (H=0) | n/a | 6.2 | 3.4 | 3.1 | 2.4 | 2.1 | 1.7 | 3.6x |
| `with-tiles`, 30-step f64 chain per element (H=30) | n/a | 110.1 | 55.1 | 30.0 | 17.9 | 17.4 | **10.3** | 10.7x |
| fork-join `fib(35)`, spawn down to depth log2 W | 32.3 | 32.7 | 21.4 | 13.8 | 10.5 | 6.7 | **5.7** | 5.7x (depth 8: 5.8; depth 10 = 1024 threads: `trap: spawn: cannot start a thread` under the cap) |
| mergesort 1e7 i64, spawn to depth log2 W, sequential merges | 326.0 | 326.0 | 185.7 | 126.7 | 126.6 | 109.8 | 118.9 | 3.0x (Amdahl: the top merges are sequential) |
| matmul f64 1024, row blocks | 43.0 | 40.5 | 25.1 | 16.4 | 16.2 | 15.7 | 20.5 | 2.7x |
| matmul f64 2048, row blocks | 304.9 | 313.9 | 174.3 | 118.7 | 97.5 | 101.3 | 95.8 | 3.2x |

Reading the table. (a) Compute-bound, data-parallel work (`with-tiles` H=30) scales to 10.7x on 28 threads; the machine's capacity is about 15 to 18 P-core equivalents
(section 2.4), so this is 60 to 70% efficient (the capacity figure is an estimate from the rows of 2.4). EV2's 3.2x on 4 tasks (`~/.cache/fibber-scratch/windows-bench/rows.tsv`: `par-1` 284 ms, `par-4` 87.7 ms) is the same effect at W=4. (b) Bandwidth-bound loops stop at about 16 workers. (c) `map`
by "return a chunk and copy" never beats sequential: results must be written in place (tiles) or the combine must be free. (d) **Matmul by row blocks is poor** (3.2x of a possible
15): each task calls `mmul`, which packs the whole of B again (`panel-blocks` in `gemm-f64.fib`) and the 28 tasks stream the same B; [hypothesis, not profiled] a parallel GEMM must share one packed B and split
both dimensions. Checksums are identical for every W (`v=7.73359536508728E8` for 1024). The timed region includes a checksum pass (about 2 ms at 1024, 10 ms at 2048). (e) The `fma` 1e8 rows
(`./p4 fma 100000000 W 3`: 2607 ms at W=1 to 106 ms at W=28, sequential 2514) are dominated by first-touch page faults of 3.2 GB under memory pressure and are not a scaling result; they are in `results/all.out.txt` only.

### 2.3 Scheduler: mutex queue against Chase-Lev deques

`sched.c` runs the same task protocol as `rt/task.lir` (state word, claim by `cmpxchg`, join runs the task itself if unstarted, else other queued tasks, else sleeps) on two queues.
**A** is a transcription of the runtime: one list under one mutex, `broadcast` at every enqueue and every completion, malloc'd nodes, joiners sleep on the condition variable. **B** is one Chase-Lev
deque per worker (Le, Pop, Cohen, Nardelli 2013, C11 atomics) with random-victim stealing and spin-then-`sched_yield` idling (no parking: the real design needs it, section 3.1).
Workload: fork-join `fib(40)`: `ptask(n)` forks `n-1`, runs `n-2` inline, joins; below `CUT` it is sequential `sfib`. `forks` is the number of tasks. Median ms of 5, `W` includes the main thread.
Command: `flock /tmp/fibsuite.lock taskset -c SET ./sched A|B W CUT 40 5` (`schedmat.sh`); raw in `results/schedmat.out.txt`.

| tasks (CUT) | impl | W=1 | 2 | 4 | 8 | 16 | 28 |
|---|---|---:|---:|---:|---:|---:|---:|
| 24.2 M (5), leaf ~10 ns | A mutex | 908 | 6392 | 6112 | 14249 | 14938 | 17198 |
| | B deques | 460 | 937 | 911 | 907 | 908 | 1194 |
| 2.18 M (10), leaf ~0.1 us | A mutex | 163 | 825 | 1074 | 1492 | 1469 | **1796** |
| | B deques | 114 | 140 | 117 | 110 | 103 | **132** |
| 17 710 (20), leaf ~10 us | A mutex | 103 | 70.2 | 30.8 | 18.8 | 18.4 | 18.4 |
| | B deques | 102 | 68.8 | 30.9 | 17.4 | 16.6 | 21.2 (min 11.1) |

The mutex queue gets *slower* with workers once tasks are small (2 workers: 7x slower than 1 at 24 M tasks) and costs 13 to 14x more at 28; with 10 us tasks the two are the same, so granularity decides everything.
Uncontended `./sched micro` (`taskset -c 0`): Chase-Lev push+pop 15.0 ns, mutex queue 14.8 ns: the deque costs nothing when there is no parallelism, and B at W=1 is 1.4 to 2x faster than A (no malloc, no lock, no broadcast).
At 2.2 M tasks B does not speed up (114 to 103 ms, sequential about 90 ms): a fork+join of a 0.1 us leaf costs about 10 ns, so grain control (section 3.2) matters even with the best scheduler.
**M1 Ultra** (`bash macrun.sh` over ssh, `~/fibber-a64-scratch/par`, no installs; 20 cores): CUT=10, W=20: A 874 ms, B 241 ms (3.6x); CUT=20, W=20: A 33.1, B 20.0 ms; W=1 CUT=10: A 373, B 315; micro: Chase-Lev 8.1 ns, mutex 20.7 ns.

**The deque in lIR** (`cl.lir`, 42 lines of lIR for push, pop, steal; `lairf run -O 2 cl.lir`): the owner pushes 4e6 items and pops every fourth, three thieves steal; every item must be taken once (`dup` = taken twice, `lost` = never).
`clmut.sh` plants faults and runs 20 times each: original `dup 0 lost 0` 20/20. **M1** steal without the compare-and-swap on `top`: duplicates 700 000 to 830 000 per run, 20/20. **M2** `fence seq_cst` in `pop` weakened to `release`:
`dup 1..668`, `lost` up to 668, in 7 and 10 of 20 runs (two sessions; the rest clean). **M3** pop of the last item without the race for it: `dup 1..327 656` in 10 and 14 of 20. **M4** the fence between the loads of `top` and `bottom` in `steal` removed: **not detected** on x86 (20/20 clean; x86 stores
are ordered). Cross-compiled to arm64 (`lairf build --target arm64-apple-macosx --emit obj`, linked with `cc` on the M1 Ultra, `a64.sh`): original clean 20/20, M1 caught 20/20, M3 5/20, **M2 and M4 not detected in 20 runs**.
So the stress test finds missing atomics and a missing race for the last item and does not find missing fences even on a weakly ordered CPU: ordering bugs need a model checker (section 3.8).

### 2.4 Hybrid cores, SMT and thread counts

`./p1 seqsum 100000000 0 5` and `sfib 35` pinned to one CPU (`all3.sh`): P core 115.0 ms and 37.1 ms, **E core 186.7 ms (1.6x slower) and 48.3 ms (1.3x)**. Two threads of `./p1 sum 100000000 2 4`: distinct P cores 49.9 ms, SMT siblings
85.3 ms (only 1.08x of one thread: 92.4), two E cores 93.7 ms. Sixteen equal chunks: 8 P with SMT (`0-15`) 21.1 ms; 12 E only 35.1 ms; 8P+8E 24.2; 8P+12E without SMT, 20 chunks 22.1; **all 28, 28 equal chunks 24.2 (the E cores are
stragglers); all 28 with 112 chunks (4 per CPU) 19.2**. Equal static chunks lose about 25% on this chip; over-decomposition with stealing recovers it. Intel exposes the split as `/sys/devices/cpu_core/cpus` and `cpu_atom/cpus`
(`cpu_capacity` reads 1024 for every CPU here, so it does not distinguish them); the runtime's `sysconf(84)` counts 28 whatever `taskset` says.

### 2.5 Reference-count contention

`rc.c`: N threads each do 2e6 `retain` + `release` pairs on objects with fibber's 32-byte layout (`count` first, `flags`), object `(i*7+tid) % K`. ns per pair per thread (ideal: flat). `taskset -c SET ./rc MODE K T 2000000` (`rcmat.sh`).

| mode | K | T=1 | 2 | 4 | 8 | 16 | 28 |
|---|---:|---:|---:|---:|---:|---:|---:|
| `p` plain count, each thread its own objects (the unshared path) | 64 | 5.0 | 5.0 | 5.1 | 4.1 | 6.9 | 10.1 |
| `a` SHARED, atomic count (the shared path) | 64 | 17.5 | 26.5 | 60.6 | 107.6 | 263.0 | **442.9** |
| `a` SHARED, atomic count | 1 | 19.7 | 32.0 | 94.5 | 269.5 | 514.3 | **973.2** |
| `P` as `a`, one object per 64-byte line | 64 | 17.1 | 21.1 | 52.0 | 85.6 | 163.7 | 275.0 |
| `P` as `a`, padded | 1 | 17.7 | 22.9 | 84.4 | 162.3 | 362.9 | 975.1 |
| `f` frozen/IMMORTAL: `retain` reads the flags and returns | 64 | 5.2 | 3.7 | 5.3 | 4.0 | 7.5 | 8.4 |
| `c` per-thread count cache (64 slots, flush every 4096 ops) | 1 | 5.2 | 5.3 | 5.4 | 5.5 | 8.3 | 9.6 |

(The plain row measured 2.1 ns in a quiet run; the machine noise above applies.) **M1 Ultra**, T=1,2,4,8,16,20: plain 1.2, 1.2, 1.6, 1.5, 2.5, 2.4; frozen 0.9 to 1.8; cache 1.2 to 2.4; atomic K=64 8.9, 49, 131, 229, 493, 624;
atomic K=1 13.9, 97, 312, 1324, 2874, 3603 ns. Padding removes false sharing (K=64: 443 to 275 ns) but not true sharing (K=1: unchanged). **The same thing in fibber** (`p2`, T threads each doing 2e6 "take a count of a node of a shared Vec, hold it in a fresh Vec, drop it" iterations; ms, median of 3; `p2run.sh`):

| mode | T=1 | 2 | 4 | 8 | 16 | 28 |
|---|---:|---:|---:|---:|---:|---:|
| private copies (counts not shared) | 77 | 80 | 124 | 146 | 216 | 237 |
| shared Vec, K=1 node | 85 | 286 | 629 | 1245 | 2652 | **5934** |
| shared Vec, K=64 nodes | 88 | 156 | 184 | 318 | 484 | 709 |
| `def` (IMMORTAL) node | 72 | 71 | 113 | 172 | 228 | 260 |
| shared Vec, plain field read `(. (vec-nth nodes i) a)` | 77 | 108 | 123 | 200 | 345 | 441 |

The last row is the surprise: **a plain field read of an element of a shared Vec is not free**: `vec-nth` returns an owned value, the lIR has `call @f.fib.prelude.vec-nth.str` then `call @fib.release`, so the read is a retain
and a release (one atomic pair on a SHARED object). The `def` row shows what freezing buys: 22x at T=28, no code change but the object's flag. (The private and `def` rows also degrade 3 to 3.6x from T=1 to 28 because each iteration allocates a Vec and the machine has 8 P + 12 E cores; the comparison is between rows, not against a flat line.)
Uncontended, SHARED costs 17 against 5 ns per pair (3.5x), 10% on the whole fibber loop (85 against 77 ms).

### 2.6 Atoms

`./p5 MODE T 200000` (T threads each doing 2e5 `(swap! a (fn (v) (+ v 1)))` or `@a` on one shared `(atom 0)`, or each on its own), median of 3, ms (`all.sh`):

| mode | T=1 | 2 | 4 | 8 | 16 | 28 |
|---|---:|---:|---:|---:|---:|---:|
| `swap!`, own atom | 2.6 | 3.1 | 5.4 | 8.2 | 8.1 | 7.3 |
| `swap!`, one shared atom | 2.6 | 8.4 | 11.6 | 145.7 | 783 | **3835** |
| `@a`, own atom | 1.2 | 1.3 | 2.6 | 2.6 | 6.2 | 4.0 |
| `@a`, one shared atom | 1.5 | 3.3 | 5.5 | 90.9 | 249.9 | **1104** |

No update was lost (the final value is T x 2e5 per run). At 28 threads a read costs 5.5 us and a `swap!` 19 us; the aggregate throughput of 28 readers (5 M reads/s) is 26 times *below* one reader's (133 M/s). The atom holds an `i64`; the lock is taken for the scalar anyway.

### 2.7 Determinism

`./p4 sumfast 10000000 W 1` (naive: W chunks of n/W summed by `fib.tensor/sum-fast`, partials added in task order) against `./p4 dsumfast 10000000 W 1` (fixed 65 536-element chunks, chunk `c` computed by worker `c mod W`, partials combined in a fixed pairwise tree
by the joiner), data `((11 i mod 101) - 50)/7` (inexact in binary), `det.sh`:

```
naive  W=1 -0.8571428572967577   W=2 -0.8571428573077444   W=3 -0.8571428572967417   W=4 -0.8571428572802731
       W=7 -0.8571428572873341   W=8 -0.8571428572747095   W=16 -0.8571428572760791  W=28 -0.8571428572755497
fixed  W=1,2,3,4,7,8,16,28: all -0.8571428572784239      sequential sum-fast: -0.8571428572967577 (a third value)
```

Timing, 1e8 elements, medians, ms (`all2.sh`): sequential 48.2; naive W=1..28: 45.3, 30.2, 26.0, 20.0, 18.3, 23.7; **fixed-grain tree: 38.0, 25.0, 20.6, 19.8, 17.9, 17.0**, value `-7.142857144208232` at every W. The cost of determinism here is not measurable (the fixed grain gave 1526 chunks, strided over the workers).
Note the sequential result is a third value: determinism needs the sequential path to use the *same* tree.

### 2.8 Race detection: ThreadSanitizer works on lIR output

Pipeline (`mktsan.sh`, ~10 lines): `F emit prog.fib` (lIR) -> `lairf emit-llvm` -> add `sanitize_thread` to every `define` (342 functions of one program, runtime included) -> `opt -passes=tsan` (LLVM 21) -> `llc -relocation-model=pic -filetype=obj` -> `gcc -fsanitize=thread`
(GCC 15's `libtsan` has LLVM's ABI). No fibber change; `tsan.sh` runs six kernels. First run, unpatched: **a data race on the global `fib.mt`** in five of six kernels: `fib.spawn` and `fib.pool-start` store `1` to the plain `i32` `@fib.mt` on every spawn
(`rt/thread.lir`) while allocation and release read it on other threads (`fib.alloc`, `fib.alloc-slow`, `fib.free-block`): benign in practice, a race in the LLVM memory model; the fix is `atomic-load/store monotonic`. With `fib.mt` made atomic
(`mktsan.sh prog.fib out patch`), five kernels are silent and **`with-tiles` reports a real race**: `tile-window` (`lib/fib/view/tiles.fib`) reads `(array-get arr 0)` as the seed in every task while tile 0's task writes element 0; the fix is to read the seed once
before spawning. TSAN on the lIR deque and its four mutants (`cltsan.sh`): no reports on any (every access is atomic); `dup` counts as in 2.3. So: TSAN finds races on non-atomic memory, which is where the runtime has them; it cannot find ordering bugs among atomics (section 3.8).

---

## 3. Design

### 3.1 Scheduler (P-sched)

**Three tiers, one `Task` type.** T0 `async`/`await`: stackless, lazy, runs where it is awaited (exists; IO concurrency and coroutines). T1 **pool tasks** (new): `(fork f)` = `fib.par/fork`, run-to-completion fork-join on the work-stealing pool, reusing the `(Task a)` object
(state, driver word, waiters, result) so `join`, `try-join`, `@t` and the `Send` rules need no change; creation pushes to the creating worker's deque, `join` claims the task (`cmpxchg driver`) and runs it inline if nobody stole it. T2 `spawn`/`future`: an OS thread, preemptive,
blocking allowed (IO, long-lived tasks, spin-waits on atoms: cases 166 to 168 keep working). `pmap`, `pcalls`, `pvalues`, `plet` and every parallel library function use T1; `future` stays T2 (Clojure's `future` may block).

**Deques and the injector.** Per worker a Chase-Lev deque (`cl.lir`, section 2.3: push, pop, steal in 42 lines of lIR; fixed ring of 4096 with overflow running the task inline, which bounds nothing that matters: a recursion deeper than the ring runs its children inline).
One injector queue (the existing mutex list, kept) for tasks made by threads that are not workers (main, T2 threads). Workers look: own deque, injector, then one random victim per round, then a full sweep.

**What makes `async` 31 ns and what limits it**, so T1 can keep it: no thread, no queue operation, no lock on the inline path; one `cmpxchg` to claim, one atomic store to publish. T1 inline cost = task allocation + `fib.share` of the captures (9.6 ns per object, once) + a push and a pop (15 ns for the pair, `sched micro`) + the claim:
estimated 60 to 100 ns, i.e. grain of about 1 us pays. It can be lower with **stack-allocated task records** for structured scopes (the `STACK` flag already makes retain/release no-ops; a task that cannot outlive its scope needs no count). Limits: blocking (below), the share walk of large captured graphs, and the closure allocation.

**Join = help-while-waiting.** A worker that joins runs, in order: the joined task if unstarted (inline), its own deque (LIFO: its newest children), then steals; it never sleeps while any work exists. This is what `fib.drive` already does with one shared queue, so no deadlock from workers blocked on children
(the case the owner asked about): with 1 worker and a recursion of depth 1000 the program completes. Cost: *latency inversion*, a joiner may start an unrelated long task from the steal path and cannot resume until it finishes; mitigation (TBB's rule): while joining, steal only from deques whose
task was created after the one being waited for, or only when the own deque is empty and the joined task is running elsewhere. Stack depth: helping nests frames; bound by a depth counter (past it, the joiner parks on the task's waiter list instead).

**Blocking.** A T1 task must not spin-wait for another T1 task (documented; T2 exists for that) and may block only through `join`, channel operations (which park the task as an async-style waiter, section 3.4) and `(blocking body)`, a scope that tells the pool one worker is about to block in the OS (read, sleep,
a lock) so it starts a replacement worker for the duration (Go's syscall handoff, Java `ManagedBlocker`, Tokio `block_in_place`). Worker count therefore floats between `P` and `P + blocked`.

**Parking.** Idle workers spin about 20 rounds of steal attempts (a few microseconds), then wait on an **eventcount** (one `i32` futex word plus a count of sleepers); `fork` publishes the task, then, only if `sleepers > 0`, wakes one (so the push path is one relaxed load more than `cl.lir`).
This replaces `broadcast` at every enqueue and completion. The classic bug is a *lost wakeup* (a worker checks the deques, finds nothing, a push and its wake-up happen, then the worker sleeps); the eventcount's re-check after publishing the "I am about to sleep" intent is what prevents it and the test of section 5 plants its removal.

**Thread count and cores.** `workers = min(sched_getaffinity count, cgroup cpu.max quota)`, replacing `sysconf(84)`; `FIB_THREADS=n` overrides; on Intel hybrid chips all logical CPUs are used by default and the library **over-decomposes** (at least 4 chunks per worker: measured 25% better than equal chunks on 8P+12E, section 2.4);
bandwidth-bound kernels (reduce, elementwise on large arrays) cap at `min(workers, 16)` (measured plateau). No pinning by default (the OS scheduler does better under other load); `FIB_PIN=1` for benchmarks. NUMA: none on this machine; none on Apple (no affinity API, `QOS` classes only), so no NUMA code until a target needs it.
The 64 MB malloc arena per allocating thread (about 250 live threads exhaust a 16 GB cap) stops mattering once tasks are not threads; a pool of `P` workers needs `P` arenas.

**Traps in pool tasks.** Stage 1 of exceptions isolates a trap at the *thread entry* of a `spawn` task (`pthread_exit` after completing the task as failed). A pool worker cannot exit; the isolation needs either exceptions stage 2 (unwinding, `docs/design/exceptions.md` 3.3 (c), "catch as a lightweight task") or `setjmp` at the task entry,
which lIR cannot express safely (no `returns_twice`). Plan: P-sched v1 keeps a trap in a pool task fatal (as a trap in a pool worker is today); `with-tasks` v1 runs its tasks on T2 when the caller asks for trap isolation (`:isolate`), and v2 gets isolation from stage 2.

### 3.2 Structured concurrency and the API (P-struct)

```clojure
(:use fib.par)                                   ; all of this is library; the macros expand to fork/join of section 3.1

(with-tasks [t1 (fork #(parse a))                ; scope: every task forked in it is joined on exit, in order of creation
             t2 (fork #(parse b))]
  (+ @t1 @t2))                                   ; @t = join; a trap in t1 reaches here as join's trap (try-join semantics)

(pmap f xs)                                      ; Clojure's: ordered, eager over a Vec; chunked; lazy window over an LSeq
(pmap f xs :chunk 512 :window 64)                ; chunk = items per task; window = chunks in flight
(pfor [i (range n)] body)                        ; for effect; body writes through tiles/windows, not shared cells
(preduce + 0 f xs)                               ; = fold: combine must be associative; fixed tree (3.5)
(pscan + 0 xs)                                   ; inclusive prefix: chunk scans, scan of chunk totals, add offsets: 2x the memory traffic, ~W/2 speedup
(pcalls f g h) (pvalues e1 e2) (plet ((a e1) (b e2)) ..)   ; thunks and expressions, pool tasks
(par-scope [:workers 8] ...)                     ; an explicit pool when a program wants one; default: the global pool
```

**`with-tasks`** closes over the exceptions design: exit joins every child (`try-join` each); the first trap cancels the siblings (sets the scope's interrupt flag, section 3.4), joins them, then the scope traps with the first message (`join` semantics), or returns the `Err` list when
written `try-with-tasks`. Leak accounting is the stage 1 verdict `abandoned` for what a failed task owned. Java's `StructuredTaskScope` (`ShutdownOnFailure`) and Rust's `thread::scope` are the models; Clojure has no equivalent, so the form is new, in Clojure's syntax (a binding vector, `@` deref).

**Grain.** The default chunk is `max(1, n / (8 * workers))` clamped so a chunk is at least about 1 us of work *when a cost hint exists* (`:chunk` otherwise); adaptive splitting (rayon's `Splitter`: split in halves while the deque is empty, i.e. only when a thief could use the half) is the second step.
The measured floor: a thread task needs about 100 us to pay (9.8 us spawn+join, 10%); a pool task about 1 to 5 us (fork+join ~10 ns of the C prototype, ~100 ns estimated in fibber). `pmap` over 1e6 trivial items under that rule is 28 x 4 = 112 chunk tasks: 1e6 x 3 ns / 112 = 27 us per chunk, so about 3 ms in total on the pool, and about 5 ms on threads today (112 x 10 us = 1.1 ms of spawn on top of 3 ms / 28): from **8.4 s to single-digit ms**, which is the first user-visible win.

**Chunked and ordered.** `pmap` over a `Vec`/array/range/tensor view splits by index (a `Splittable` protocol: `count`, `split-at i`, O(1) for those); results are written into a pre-sized output through tiles (the `map` kernel of 2.2 shows returning arrays loses). Over an `LSeq`: an `LSeq` holds a `Cell` and **cannot cross a task** (stdlib S10), so the producer
realises the next chunk on the calling thread into a Vec, hands the Vec to a task, keeps `:window` chunks in flight (backpressure, bounded memory, order kept), as Clojure's `pmap` keeps `ncpu + 2` futures.

**Atomics.** `swap!`, `compare-and-set!`, `reset!`, `swap-vals!` stay as specified; section 3.6 changes their implementation, not their types. A parallel body that updates one shared atom per item collapses (2.6); the library offers `(adder)`, a striped counter (`inc!`, `sum`), Java's `LongAdder`.

### 3.3 Parallel tensors (P-tensor)

Today's `fib.tensor` has no tasks (`docs/shootout/tensor.md`: "All threads: not produced"). Parallel kernels inside the library, a global pool, and **no `:parallel` option on every call**: an operation parallelises itself when its estimated work exceeds a threshold (elementwise and reductions above about 64 Ki elements, matmul above 128^3 flops; the threshold from the 1 us / 100 us grain figures) and `(t/set-workers! n)` or `FIB_THREADS` caps it. Fibber has no dynamic variables (`binding` is C11), so a global setter and an explicit `(par-scope [:workers n] ..)` replace a `:parallel` keyword threaded through every signature.
What each kernel needs:

| Kernel | Partition | Needs |
|---|---|---|
| elementwise, broadcast, map | flat ranges (contiguous) or row blocks, over-decomposed 4x | output windows: `with-tiles` with a **tile offset binding** (today the body cannot read its input range, section 1) |
| reductions (`sum`, `mean`, `max`, axis reductions) | fixed-grain chunks, fixed tree (3.5); axis reductions: one task per output row block | determinism mode |
| matmul / dense layers | 2-D tiles of C; **one shared packed B** (panel packed once, by a first parallel phase), per-task packed A blocks (BLIS loop structure) | a barrier between phases = a scope with two joins; the current `panel-blocks` re-packs B per call (2.2: 3.2x on 28) |
| batch dims (`b x m x k` times `b x k x n`) | the batch axis first (no shared packing needed), then tiles within a matrix when `b < workers` | none |
| softmax, layernorm | row blocks | row-wise kernels already exist |

SIMD kernels run inside tasks unchanged (`fib.simd`, `(has-fma)` dispatch). `simd/fma` is exact and `muladd` is fused only where the target has FMA, so **results can differ between machines** (not between worker counts): a determinism flag across machines (3.5) must use `fma` (exact; a libm call per lane without hardware FMA) or no fusion.
Matmul partitioned by output tile is bit-identical for any worker count provided the k loop of each element runs in the same order whatever the tile: true for the current kernels (the checksums of 2.2 are equal for W=1..28). **Interaction with exclusive views**: tiles are the safe way to write disjoint output (`with-tiles` joins every task before it returns, so no task outlives the frozen owner);
windows are not `Send` (and must not be), each task builds its own from the array and a range, so the unsafe core is the one trusted `tile-window`. It has the race of section 2.8 (the seed read), to be fixed before more is built on it.

### 3.4 Channels, futures, cancellation, pipelines (P-chan)

- **Channels**: `(chan n)` bounded MPMC (`n = 0` rendezvous), `(put! c v)`, `(take! c)` -> `(Option a)` (nil when closed and empty), `(close! c)`, `(alts! [c1 c2 ..])` select, in `fib.chan`; the names are core.async's. A ring buffer under one mutex with two eventcounts first (correct, measurable), Vyukov's bounded queue later if it shows. Element type must be `Send`; `put!` share-marks the sent graph (9.6 ns per object, section 2.1);
  a **unique-move send** (the sender gives up its only reference, count 1 all the way down) can skip marking and leave the receiver with plain counts: an optimisation to measure, not a day-one feature. Blocking ops on T1 tasks park the task (an async-style waiter), on T2 threads block the thread.
- **Futures**: `future` stays T2 (spawn); `(fork f)` returns the same `(Task a)`. `future-cancel`/`(cancelled?)` (stdlib S16, M5, not delivered): one `i32` interrupt flag in the task, polled by `(cancelled?)`, observed by `take!`, `put!`, `sleep` and by every library parallel loop at chunk boundaries; cancelling a scope sets the flags of all its tasks; a cancelled task ends by trapping with a distinguished message that `try-join` returns.
  Cooperative only, as Java's interrupt (the spec already says so); no asynchronous kill.
- **Pipelines**: a stage is a function over chunks; `(pipeline n f in out)` as core.async's, plus `(pmap f xs)` for the pure case. `fib.seq` laziness stops at the chunk boundary (above).
- **Agents**: not recommended (Clojure itself prefers atoms and channels); STM `ref`/`dosync`: no.

### 3.5 Determinism mode (P-det)

Two guarantees, named separately. **Across worker counts** (default on for every library reduction): partition by a fixed grain constant (65 536 elements, independent of workers), reduce each chunk with the same sequential kernel, combine the chunk results in a fixed balanced pairwise tree. The sequential implementation uses the same tree, so
`(t/sum x)` has one value everywhere. Measured cost: none visible (2.7). It differs from the strict left-to-right `t/sum` (a serial chain of adds: 102 ms for 1e8 against 35 for `sum-fast`, `docs/shootout/tensor.md`); `sum` stays the strict ordered one, `sum-fast`/`preduce` the tree. **Across machines** (`(set-portable! true)`): additionally no `has-fma` dispatch (exact `simd/fma` or unfused), no libm-vs-vector `exp` choice;
costs the FMA throughput gain of the machine (about 2x on matmul inner loops) and, without hardware FMA, a libm call per lane. Integer reductions are exact in any order; only floats need this.

### 3.6 The reference-count fix (P-count)

The numbers of 2.5 and 2.6 say contention on a count or a lock is a 20 to 700x effect, far larger than the scheduler's. The options, with what the measurements say:

| Option | Effect | Verdict |
|---|---|---|
| **Frozen / immortal publish** (`(freeze x)`: set IMMORTAL on the whole reachable graph; `fib.immortalise` exists for static data) | retain/release read the flags and return: 8.4 ns at 28 threads (flat), `def` row of 2.5: 22x | **Do first.** Right for model weights, lookup tables, config: data that lives to the end of the program. Costs: never freed (the audit must count it as intentional, like literals); the graph must not be mutated afterwards (a frozen cell or atom is refused) |
| **Scoped borrowed captures** (`with-tasks`/`with-tiles` closures may capture *borrowed* locals, no share-marking and no count: the scope joins before the owner continues, as Rust's `thread::scope`) | removes the share walk (9.6 ns per object) and the counts on what the tasks only read | **Do second**; needs a closure colour (3.7). Soundness argument is the freeze rule of exclusive views: the lender cannot touch the lent value while the scope is open |
| **Do not count what a field read only peeks** (`(. (vec-nth v i) f)`, `(first xs)`, `(get m k)` returning a borrowed element) | the last row of 2.5's fibber table: 441 against 237 ms | Library/ownership work: `vec-nth` and friends in borrowed contexts use the peek form (spec 6.3 "cell peeks" is the same idea) |
| Biased counts (owner thread non-atomic, others atomic; Python 3.13 free-threaded) | helps data shared but mostly used by one thread; **every reader of a read-mostly object is a non-owner**: no change to the rows above | Not worth a header redesign |
| Per-thread count cache (rc.c mode `c`: deltas in a 64-slot table, flushed every 4096 ops) | flat 8 to 10 ns at 28 threads | **Reject as default**: the count is stale until a flush, so a free is delayed to a flush point; that gives up "free at the last release", the property fibber sells. Possible inside a `with-tasks` scope if the flush is the scope's join; not now |
| Padding objects to a cache line | K=64: 443 to 275 ns, K=1: none | Not worth the memory; arrays are not affected (one count per array, elements are scalars or counted children) |
| Atom reads and swaps | 2.6 | **Scalar atoms**: `@a` is one atomic load, `swap!` a compare-and-swap loop with exponential backoff and `compare-and-set!` the primitive; **object atoms**: keep the lock (the retain of the loaded pointer must be atomic with the load) but test-and-test-and-set with backoff, and `(adder)` for counters |

The `fib.share` walk at spawn (9.6 ns per object) is paid once per graph; a channel send or a task result pays it on the sent graph every time.

### 3.7 What needs a language change

| Item | Change | Needed by |
|---|---|---|
| Scoped (borrowing) task closures | types: a closure colour `:scoped` that may capture borrowed non-cell locals, accepted only by `with-tasks`/`with-tiles`/`fork` in a scope that joins before it ends (the lender's freeze rule; exclusive-views types 6.15) | 3.6 option 2 (an optimisation; the first version share-marks and works today) |
| Tile offset in `with-tiles` | library macro: `(with-tiles [w lo a k] ..)` binds the range start; no compiler change | P-tensor elementwise |
| Interrupt flag, `(cancelled?)` | runtime (a task field and the blocking calls that read it), prelude builtin | cancellation (S16) |
| Trap isolation in pool tasks | exceptions stage 2, or a pool of T2 threads for isolated scopes | `with-tasks` on the pool with `try-join` semantics |
| `freeze` | a builtin over `fib.immortalise` (all types; checked: no cell/atom/weak inside) | P-count |
| `Splittable` protocol | library | `pmap`/`preduce` over any source |
| Dynamic variables | **avoid**: a global setter and an explicit scope instead of `binding` | pool parameters |
| `async` creates concurrency | **not recommended** (decision D4: tasks run lazily to completion); T1 `fork` is the new, explicit form | none |

### 3.8 Race detection we can and cannot build

Can: (1) the TSAN pipeline of 2.8 as `scripts/tsan.sh` (~30 lines), run on a threaded case subset in CI: it already finds two races. (2) Stress + mutant tests for every lock-free structure, as `clmut.sh`: they find missing atomics and missing races for the last item.
(3) A planted-race self-test: the `fib.mt` store made non-atomic must be reported (it is). (4) The audit's leak/abandon verdicts for trapped tasks (exist).
Cannot with these tools: memory-ordering bugs (a weakened fence): TSAN's analysis of standalone fences is incomplete and the deque mutants M2 and M4 passed 20/20 runs on x86 and on the M1 Ultra (2.3). That needs an exhaustive model checker over the C11 model (Relacy, CDSChecker, GenMC, loom): none installed here; a checker over the *lIR* atomics is
a project (the lIR semantics is LLVM's); until then the deque ships with the Lê et al. orderings unchanged, a citation of the paper's proof, and the stress mutants.

---

## 4. Comparison

| System | What it gives | Take | Avoid |
|---|---|---|---|
| **Rust rayon** | Chase-Lev deques, `join`, `scope`, `par_iter` with adaptive splitting, ownership-checked races, panics propagate | `join`/`scope`/adaptive splitting; compile-time race freedom (fibber has `Send` and exclusive views); stack-allocated join records | no cancellation, no determinism guarantee for float reductions; `Arc` clones are the same atomic cost as ours (borrows are free there: our scoped captures are the answer) |
| **Java ForkJoin, virtual threads** | work-stealing pool, `CountedCompleter`, parallel streams, `StructuredTaskScope`, virtual threads (stackful M:N) | structured scope with shutdown-on-failure; `ManagedBlocker` (replacement workers); a common pool | the shared common pool starving; parallel streams' unpredictable splitting; virtual-thread pinning |
| **Go** | goroutines (2 KB stacks, preemption), channels, `select`, `WaitGroup` | channels and `select`; cheap blocking | unstructured goroutine leaks; shared mutable state without checks |
| **Julia** | `Threads.@spawn`, `@threads :dynamic`, `Threads.Atomic`, task migration | `@threads`-like `pfor` ergonomics | `threadid()` indexing races; static schedule default; no race checking |
| **Clojure** | `pmap` (ncpu+2 lazy window), `future`, agents, core.async (`go` state machines on a small pool), `reducers` `fold` (fork-join) | `pmap` and `fold` signatures; core.async names, with `go` = async state machines which fibber already has; atoms | `pmap` per-element overhead (we chunk); STM; unbounded agent pools; a lock-free read of a ref that is a lock here (2.6) |
| **Python** | `multiprocessing` (processes, pickling), free-threaded 3.13 (biased + deferred counts, immortal objects), GIL | immortalisation (our `freeze`); the lesson that single-thread speed must not pay for threads (our non-atomic fast path stays) | process-per-core copies; per-object locks; deferred counts that delay frees |
| **OpenMP** | `parallel for` with `schedule`, `reduction`, tasks with `depend`, `num_threads` | `schedule(dynamic, chunk)` semantics = over-decomposition + stealing; a reduction clause | order of reduction unspecified unless static with the same thread count (our fixed tree); global barrier-structured regions |
| **TBB** | work-stealing, `parallel_for/reduce` with partitioners, `parallel_deterministic_reduce`, `task_arena`, flow graph | `deterministic_reduce` (our default tree); partitioners; arenas to isolate helping (latency inversion) | the API size |
| NumPy+OpenBLAS (the user's reference) | threaded BLAS, GIL released inside kernels | what `fib.tensor` must beat with all threads on | `OPENBLAS_NUM_THREADS` oversubscription when combined with process pools |

---

## 5. Staged plan

Sizes are lines of new code (lIR in `rt/`, fibber in `lib/` and `compiler/`) plus tests, an estimate, not a measurement; "agent-days" are a rough sizing in the project's unit.

| Package | Content | Size | Depends on | Tests that can fail |
|---|---|---|---|---|
| **P-race** | `scripts/tsan.sh` (the pipeline of 2.8) over a threaded case subset; fix `fib.mt` (atomic) and `tile-window`'s seed read | S: 30 lines of shell, 6 lines of lIR, 3 of fibber; 0.5 day | none | the planted-race self-test (non-atomic `fib.mt` must report); each fix has a case that reports before and is silent after |
| **P-struct** | `fib.par`: `with-tasks`, `pmap` (chunked, ordered, windowed), `pfor`, `preduce` (fixed tree), `pscan`, `pcalls`/`pvalues`; `Splittable`; on `spawn` threads with chunk >= 100 us, behind `fib.par/fork`+`join` so P-sched replaces the backend | M: ~500 lines of fibber, 40 cases; 3 days | P-race (fix the tile seed), the `with-tiles` offset binding | `pmap` over 1e6 items finishes in < 100 ms (today 8.4 s); results equal the sequential ones for random inputs and every W (differential, like the stdlib cases); a trap in one chunk reaches the scope as `Err` and the siblings end; **planted**: a scope that forgets to join (a leaked task still running at scope exit) must fail a case that reads after exit |
| **P-count-a** (atoms) | scalar atom `@a` as an atomic load, `swap!` as CAS with backoff, TTAS lock for object atoms, `(adder)` | S: ~80 lines of lIR; 1 day | none | the `p5` rows as a bench with a bound (28 readers < 20 x one reader); a concurrent increment case that loses updates under a planted non-atomic CAS; the existing atom cases (1709, 166 to 168) |
| **P-sched** | `rt/sched.lir`: worker deques, injector, eventcount parking, `fork`/`join` help-while-waiting, `FIB_THREADS`, `sched_getaffinity` and cgroup count; `(blocking ..)`; trap policy of 3.1 | L: ~400 lines of lIR, 80 of fibber, 3 to 5 days | P-struct's API (so the backend swaps), exceptions stage 2 for isolation | **planted lost wakeup** (remove the eventcount re-check: a stress with 1 producer and N sleepers hangs, caught by a 10 s timeout); **double steal** (`cl.lir` with M1 to M3 must fail; the unmutated must pass 1000 runs at 28 threads, and arm64 on the M1 Ultra); **missed join** (complete before storing the result: the joiner reads a zero result: case on `i64` and on object results); **deadlock** (join that blocks instead of helping: 1 worker, recursion depth 1000, must fail by timeout, passes unmutated); stack bound (depth 100 000 chain of joins); leak audit with `abandoned` on pool tasks; TSAN clean on the new tests |
| **P-tensor** | parallel elementwise/broadcast/reductions/axis reductions, then batched matmul, then 2-D tiled matmul with a shared packed B; `t/set-workers!` | L: elementwise and reductions 200 lines, matmul 300; 5 days | P-struct, `with-tiles` offset; P-det for reductions; P-sched for small tensors | bit-identical results for W = 1,2,3,7,28 on every kernel (planted: a reduction chunked by W must fail it); thresholds (a 1000-element add must not spawn: counter of tasks forked); the 2.2 matmul rows as a bench with a bound (2048: < 60 ms at 28); NumPy differential on the existing tensor cases |
| **P-chan** | `fib.chan`: bounded MPMC, `close!`, `alts!`, `pipeline`; cancellation flag, `(cancelled?)`, `future-cancel` | M: 300 lines of fibber over atoms and an eventcount, 40 lines of lIR; 3 days | P-sched (parking), the interrupt flag | FIFO per producer under 8x8 stress with checksums; `close!` wakes all blocked takers (planted: a close that wakes one: hang); `alts!` fairness (no starvation over 1e5 rounds); cancel of a task blocked in `take!` ends it |
| **P-det** | the grain constant, the tree combiner, sequential = same tree, `(set-portable! true)` | S-M: 100 lines of fibber; 1.5 days | P-struct, P-tensor reductions | `det.sh` as a case: equal bits for 8 worker counts; planted naive chunking fails; portable mode equal to a no-FMA build |
| **P-count-b** | `freeze`; `with-tasks` scoped captures (the closure colour); borrowed `vec-nth` reads | M: 150 lines of lIR/fibber for `freeze`, 300 in the checker for the colour; 4 days | exclusive views' `with-view` machinery (the freeze rule), P-struct | after `(freeze x)`, 28 readers leave `x`'s count unchanged and trace mode shows no `A`/`F` lines for it (planted: `freeze` that skips children: a count changes); the `rc`/`p2` rows as benches (shared K=1 at 28 threads within 3x of private); a scoped closure that captures a cell or escapes is rejected |

**Order for the soonest user-visible win.** The owner's guess is right with one amendment. (1) P-race (half a day; two real races fixed). (2) P-struct on today's runtime: `pmap` over 1e6 items from 8.4 s to milliseconds, `preduce` 4 to 5x, `with-tasks`: all measured as possible with chunked `spawn`
tasks. (3) P-count-a in parallel with (2) because the first thing a user writes in a `pmap` body is `swap!` on an atom and it collapses at 8 threads (145 ms against 2.6). (4) A measured **P-tensor elementwise + reduction** on the same chunked backend (2.2: 4 to 5x on 1e8-element reductions, 10x on compute-bound
element work with tiles) before P-sched. (5) P-sched, which the fine-grained and recursive code (fork-join `fib`, sorts, small tensors) is waiting for: 13x on small tasks in the prototype. (6) P-chan, P-det proper, P-count-b. Not before: P-tensor matmul (needs the shared-B design and P-sched).

---

## 6. What was not done

- No change to `rt/`, the compiler, `lib/` or `spec/`; the prototypes live in `docs/design/parallelism/proto/`. The scheduler comparison is a C transcription of the runtime's protocol, not the lIR runtime itself; fibber-level numbers for the new scheduler are estimates until P-sched exists. The C `sched` has no parking (it spins), so idle-power and wake latency are not measured.
- No parallel-GEMM with a shared packed B was built (the 3.2x row is row-block `mmul`; the explanation is a hypothesis); no NumPy+OpenBLAS all-threads column was measured (the reference is single-thread, `docs/shootout/tensor.md`). No parallel quicksort (mergesort measured), no `pscan`, no channels prototype, no `future-cancel`.
- `with-tiles` was measured with `spawn` per tile; it was not run on the (not yet existing) pool. The `map`/`fma` rows are noisy (page faults, memory pressure, load average up to 20 from jobs outside the lock).
- aarch64: the deque and the count microbenchmarks ran on the M1 Ultra; no fibber program was run there. Weak-memory bugs (M2, M4) were not found by stress on either CPU; no model checker was run.
- 250 simultaneous threads under `ulimit -v 16000000`: not lifted. The 1024-thread `pfib` depth-10 run failed under the cap as expected and was not repeated without it; `MALLOC_ARENA_MAX` was not used.
- ThreadSanitizer: run on six kernels and the deque, not on the case suite; the two findings are not fixed (design task).
- The `fib.share` unique-move optimisation, striped adders, adaptive splitting and the helping-depth bound are described, not measured.
