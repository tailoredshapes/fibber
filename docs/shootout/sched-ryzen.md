# SCHED-RYZEN: is the idle-spinner cap of 2 (DARWIN-3) right on the Ryzen 5800X?

DARWIN-3 (commit 2cea1313) let at most `FIB_SPINNERS` (default 2) idle pool workers spin; it was measured on an M1 Ultra and on a 28-CPU Linux box. This re-measures it on the Ryzen 7 5800X (8 cores / 16 threads, one CCD with 32 MB L3 as WSL2 reports it; WSL2 Ubuntu).

## Result

**The default of 2 is wrong on this CPU. It is 3x slower than no cap on the fork-heavy workloads and gains nothing on the others.** Case 8645 at 16 workers: 2443 ms with the default, 743 ms with `FIB_SPINNERS=16` (what 0.1.13 did before the cap). A cap that is a fraction of the pool wins clearly: 8 spinners at 16 workers (half) is within 10% of uncapped on every row (section 5), 4 is 1.7x worse than uncapped, 2 is 3.2x worse. Nothing measured here is faster with the cap than without it. Chunked `pmap`/`preduce`/`pfor`, `pfib` and parallel LZ4 do not care. The Mac result (cap helps) and this one disagree plausibly because `sched_yield` is an expensive system call on Darwin and a cheap one here; the rule should be per platform, or proportional to the pool (for example `max(2, workers / 2)`), not a constant. No fibber code was changed in this task.

## Method

* **Releases** (fetched with `gh release download`, copied to the Ryzen, `sha256sum -c` against each release's `SHA256SUMS` printed `OK` for both): `fibc-0.1.12-linux-x86_64.tar.gz`, `fibc-0.1.13-linux-x86_64.tar.gz`. All work is in WSL `~/fibber-scratch/sched`. Nothing was installed (`curl gcc make python3 flock` were there).
* **What 0.1.12 is.** It predates P-sched: `git ls-tree v0.1.12` has no `rt/sched.lir`, `rt/park.lir` or `rt/deque.lir`, and compiling case 8645, case 8642 and `sched-fork.fib` with it fails with `unbound name fork-task`, `lz4-bench.fib` with `module fib.compress is not at ...`. Its parallel library runs OS-thread tasks. So it is a comparison row for `pmap`/`preduce`/`pfor` only; it is NOT the pre-cap pool. The pre-cap pool is 0.1.13 with `FIB_SPINNERS=16`: in `rt/sched.lir` an idle worker parks at once when `idlers >= spinners`, and with 16 workers at most 15 are idle, so 16 never caps. (`git merge-base --is-ancestor 2cea1313 v0.1.13` is true, `... v0.1.12` is false.)
* **Builds** (each version with its own embedded library; sources from main): `fibc build scripts/bench/parallel-scale.fib -o pscale`, `fibc build docs/shootout/parallel/sched-fork.fib -o sfork`, `fibc build cases/stdlib/8645*.fib -I cases/stdlib/support -o c8645` (same for 8642), `fibc build scripts/lz4-bench.fib -O 3 -o lz4b`.
* **Runs.** `ulimit -v 16000000`, `MALLOC_ARENA_MAX=2`, `FIB_THREADS=W` (pool size; 0.1.12 has no pool and takes W from `Par W`), `FIB_SPINNERS=k` or unset. Every cell is the median of 5 runs with the configurations interleaved inside each repetition, so drift hits all alike. The harness was a small python script (`subprocess`, `time.perf_counter`) kept in the scratch. Commands: `pscale MODE W N`, `c8645`, `c8642`, `sfork ROW N 1`, `lz4b MODE silesia/all 1 5 W 1048576`. Case timings are wall time of the whole built program.
* **Quiet state.** The machine had been rebooted 4 minutes before the first run: `14:51:02 up 4 min, load average: 0.93, 0.69, 0.30`; `vmstat 1 3` showed 82 to 92% idle, 4 to 14% iowait (boot), 0 runnable. Background right after boot: `k3s-agent` 7% CPU, `containerd` 6%; by the end of the first block 0.8% and 0.2%, nothing else above 1%. The load averages quoted by the harness afterwards are the benchmark's own. No other job of mine ran. The Windows host was not inspected and WSL2 is a guest, so absolute figures carry its noise (about 10% on parallel rows per `docs/shootout/lz4.md`); the effects below are 3x.
* Total wall time of the run: about 1 hour.

## 1. Pool benchmarks (scripts/bench/parallel-scale.fib; call time in ms, median of 3 rounds within a run, median of 5 runs)

`pmap`, N = 10 000 000 (`pmap-each [x xs p] (* x 2)`):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| 0.1.12 (threads) | 86.9 | 69.8 | 53.7 | 50.8 | 45.9 |
| 0.1.13 sp=0 | 86.4 | 69.5 | 52.9 | 47.0 | 44.5 |
| 0.1.13 sp=1 | 86.3 | 69.8 | 53.4 | 46.8 | 44.6 |
| 0.1.13 default (2) | 86.6 | 72.8 | 53.1 | 47.2 | 45.6 |
| 0.1.13 sp=4 | 86.5 | 69.8 | 52.7 | 46.8 | 44.6 |
| 0.1.13 sp=16 | 86.4 | 70.4 | 52.9 | 47.4 | 44.6 |

`preduce`, N = 1e9 (`preduce-range [i N p] + 0 (bit-xor (* i 3) 5)`):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| 0.1.12 (threads) | 1992.9 | 1005.9 | 520.7 | 505.6 | 295.4 |
| 0.1.13 sp=0 | 1983.1 | 1020.1 | 531.0 | 305.2 | 266.4 |
| 0.1.13 sp=1 | 1983.8 | 1019.5 | 531.7 | 301.1 | 266.9 |
| 0.1.13 default (2) | 1979.3 | 1020.0 | 531.6 | 310.4 | 266.9 |
| 0.1.13 sp=4 | 1985.1 | 1019.7 | 532.2 | 302.3 | 267.4 |
| 0.1.13 sp=16 | 1983.9 | 1020.7 | 531.9 | 311.0 | 267.4 |

`pfor-alloc`, N = 10 000 000 (`pfor-range`, each element builds a Vec of 6):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| 0.1.12 (threads) | 1056.1 | 773.8 | 467.5 | 414.7 | 241.8 |
| 0.1.13 sp=0 | 1072.0 | 811.4 | 830.7 | 473.5 | 319.0 |
| 0.1.13 sp=1 | 1073.3 | 809.4 | 856.8 | 478.9 | 327.8 |
| 0.1.13 default (2) | 1074.3 | 815.9 | 830.9 | 494.0 | 320.0 |
| 0.1.13 sp=4 | 1070.0 | 815.5 | 823.3 | 484.3 | 318.3 |
| 0.1.13 sp=16 | 1068.2 | 810.0 | 844.1 | 469.9 | 327.2 |

The spinner count moves none of the three (0.1.13 rows within 5% of each other, their order is noise). Scaling 1 to 16 workers: 1.9x for `pmap` (memory bound), 7.4x for `preduce`, 3.3x for `pfor-alloc`. A finding unrelated to spinners: `pfor-alloc` on the 0.1.13 pool is slower than on 0.1.12's threads at W >= 4 (320 against 242 ms at 16) and has a bump at W=4 (about 830 ms, no faster than W=2) in all five 0.1.13 rows. Not investigated.

## 2. Cases 8645 and 8642 (wall ms of one run of the built case, exit code 0 required; 0.1.12 cannot build them)

Case 8645 (a million tiny tasks forked and joined by one pool task, then 20 000 forked before any join), the DARWIN-3 case:

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| sp=0 | 83 | 390 | 12503 | 14387 | 14549 |
| sp=1 | 82 | 467 | 1424 | 3656 | 3725 |
| **default (2)** | 82 | 401 | 770 | 2298 | 2443 |
| sp=4 | 81 | 387 | 628 | 1262 | 1327 |
| sp=16 (before DARWIN-3) | 81 | 400 | 642 | 640 | 743 |

Case 8642 (nested pmap and forks keep the thread count at the pool size):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| sp=0 | 150 | 97 | 87 | 180 | 259 |
| sp=1 | 146 | 97 | 86 | 177 | 238 |
| default (2) | 146 | 96 | 86 | 180 | 234 |
| sp=4 | 145 | 97 | 88 | 176 | 265 |
| sp=16 | 145 | 96 | 86 | 181 | 264 |

8645: the cap of 2 recovers 6x from parking at once (sp=0: 14.5 s) but stays 3.3x slower than no cap at 16 workers and 3.6x at 8. From 2 workers up every configuration costs about 5x the one-worker 82 ms: a storm of tiny tasks does not scale here even uncapped (the steal cost, not the spinners). 8642 is indifferent to the cap (spread 234 to 265 ms at W=16 is noise).

## 3. Many small tasks (docs/shootout/parallel/sched-fork.fib; ms per run, internal timer, one warm-up run discarded)

`sfork ROW N 1`. `storm`: 300 000 fork+join rounds inside one pool task; `storm-main`: the same from the main thread (through the injector); `pfib`: fib(40) with a sequential cut at 20; `wide`: 100 000 tasks forked before any join. (0.1.12 has no `fork-task`.)

| row | config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|---|
| storm | sp=0 | 21.8 | 117.9 | 3723.7 | 4256.6 | 4298.6 |
| | sp=1 | 21.5 | 113.5 | 506.9 | 951.3 | 987.7 |
| | **default (2)** | 22.1 | 143.8 | 225.6 | 669.0 | 741.2 |
| | sp=4 | 21.6 | 123.9 | 181.4 | 360.5 | 357.2 |
| | sp=16 | 21.3 | 107.4 | 182.8 | 204.3 | 213.1 |
| storm-main | sp=0 | 15042.4 | 19385.8 | 25776.7 | 41066.3 | 69498.0 |
| | sp=1 | 289.3 | 303.4 | 349.8 | 482.8 | 882.7 |
| | **default (2)** | 302.2 | 259.1 | 304.6 | 308.8 | 442.0 |
| | sp=4 | 291.0 | 256.9 | 250.6 | 266.4 | 276.2 |
| | sp=16 | 287.4 | 256.7 | 242.2 | 259.0 | 310.6 |
| pfib 40 | all five configs | 575 to 579 | 292 to 293 | 153 | 81 to 84 | 51 to 52 |
| wide | sp=0 | 13.9 | 40.7 | 1222.2 | 1476.5 | 1483.8 |
| | sp=1 | 13.8 | 43.2 | 116.4 | 300.1 | 309.7 |
| | **default (2)** | 13.7 | 52.2 | 66.5 | 269.6 | 223.1 |
| | sp=4 | 13.9 | 44.2 | 67.6 | 105.7 | 114.7 |
| | sp=16 | 13.7 | 41.0 | 50.5 | 59.3 | 67.2 |

`pfib` (coarse tasks) is unaffected and scales 11x at 16. For fine-grained work the default costs 3.5x (`storm` 741 against 213 at 16, `wide` 223 against 67). `storm-main` is the one row where the default is nearly as good as the best (442 against 276 to 311 at 16); `sp=0` is catastrophic everywhere (a worker that parks at once: 69 s at 16 for 300 000 forks).

## 4. Parallel LZ4 (scripts/lz4-bench.fib, 211 MB concatenated Silesia, level 1, 1 MiB blocks, MB/s of input; median of 5 inside `lz4b`, median of 5 launches; W = FIB_THREADS = `Options.threads`)

Compress (`f`):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| sp=0 | 667 | 1270 | 2300 | 3844 | 5353 |
| sp=1 | 669 | 1270 | 2307 | 3824 | 5339 |
| default (2) | 669 | 1272 | 2299 | 3843 | 5320 |
| sp=4 | 667 | 1270 | 2296 | 3839 | 5362 |
| sp=16 | 665 | 1269 | 2299 | 3828 | 5304 |

Decompress (`fd`, content checksum verified):

| config | W=1 | 2 | 4 | 8 | 16 |
|---|---|---|---|---|---|
| sp=0 | 3209 | 5217 | 8037 | 6175 | 5891 |
| sp=1 | 3217 | 5187 | 8315 | 6391 | 6024 |
| default (2) | 3200 | 5190 | 8372 | 6071 | 6012 |
| sp=4 | 3335 | 5029 | 8045 | 6128 | 5975 |
| sp=16 | 3339 | 5213 | 8016 | 6070 | 5843 |

No effect (all within 4%). Decompress peaks at 4 workers (8.0 to 8.4 GB/s) and falls at 8 and 16, as `docs/shootout/lz4.md` found (memory bound). 0.1.12 has no `fib.compress`.

## 5. Sweep of the cap at 8 and 16 workers (case 8645 wall ms; sfork ms; median of 5, same harness)

| FIB_SPINNERS | 8645 W=8 | 8645 W=16 | storm W=8 | storm W=16 | wide W=8 | wide W=16 |
|---|---|---|---|---|---|---|
| 2 (default) | 2243 | 2355 | 677.3 | 617.5 | 205.5 | 219.5 |
| 4 | 1256 | 1209 | 373.6 | 356.3 | 105.7 | 116.6 |
| 6 | 767 | 933 | 247.2 | 267.8 | 77.5 | 75.7 |
| 8 | 711 | 797 | 207.7 | 239.3 | 58.0 | 70.8 |
| 12 | 745 | 773 | 211.9 | 211.8 | 62.1 | 68.8 |
| 15 | 656 | 747 | 213.0 | 208.5 | 61.9 | 68.6 |
| 16 | 654 | 725 | 223.2 | 215.2 | 61.2 | 68.9 |

At W=8 a cap of 8 is no cap. At W=16 the half-cap (8) is 797 against 725, 239 against 215 and 71 against 69: within 10%. Below about a third of the pool it degrades roughly as 1 over the cap.

## Conclusion

1. On the Ryzen 5800X the default of 2 regresses fine-grained fork workloads by 3.3x (8645 at 16 workers 2443 against 743 ms; `storm` 741 against 213; `wide` 223 against 67) relative to the behaviour before DARWIN-3, and improves none of the measured rows. Chunked `pmap`/`preduce`/`pfor`, `pfib`, parallel LZ4 and case 8642 are unaffected.
2. A cap proportional to the pool wins clearly here: `max(2, workers / 2)` is within 10% of uncapped; a constant 4 is 1.7x off, 2 is 3.2x off. Whether that also keeps the Mac win (cap 2 of 20 workers: 8.8 s to 4.1 s per 10 runs) is not known: the Mac table has only the values 0, 1 and 2. The data to choose one rule for both machines needs a Mac run with `FIB_SPINNERS` of 5 and 10. A per-platform default (cap on Darwin, half or none on Linux) fits both measurements today.
3. Limits: WSL2 guest; bare-metal Linux and Windows-native may differ. The 5800X has one CCD, so this does not test a two-CCD part (the task's "2 CCDs" does not describe this CPU).
