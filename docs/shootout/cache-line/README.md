# Where false sharing stops (CACHE-0)

`false-sharing.lir`: two threads each increment their own `i64` (volatile load and store, 100 million times), the two words `stride` bytes apart in one
4096-aligned block; stride 0 is one thread alone. The least stride at which the time falls back to the one-thread time is the **destructive interference size** of
the pair of cores (docs/design/inline-lir.md, section 4).

    lairf build docs/shootout/cache-line/false-sharing.lir -o fs      (lairf from compiler/lairf.fib)
    taskset -c 0,2 ./fs                                               (cores differ in the answer: pin them)

Measured 2026-10-20 on an Intel Core i7-14700KF (Raptor Lake: P-cores are cpus 0 to 15, two threads each, so 0 and 2 are two cores; E-cores 16 to 27), Linux 7.0, LLVM 21,
three runs of each pair, ns per increment:

| pair | alone | 8 | 16 | 32 | 64 | 128 | 192 | 256 |
|---|---|---|---|---|---|---|---|---|
| P-cores 0,2 (3 runs) | 0.39 to 0.46 | 1.35 to 2.06 | 1.36 to 1.86 | 1.48 to 2.03 | 0.41 to 0.47 | 0.39 to 0.43 | 0.39 to 0.42 | 0.39 to 0.43 |
| P-cores 4,6 (3 runs) | 0.40 to 0.43 | 1.02 to 1.36 | 1.65 to 1.97 | 1.36 to 1.44 | 0.39 to 0.54 | 0.42 to 0.47 | 0.41 to 0.45 | 0.41 to 0.45 |
| P and E, 0,16 (3 runs) | 0.37 to 0.48 | 1.62 to 1.80 | 1.62 to 1.72 | 1.60 to 2.17 | 0.59 to 0.62 | 0.56 to 0.58 | 0.55 to 0.62 | 0.57 to 0.60 |
| E-cores 16,17 and 16,20 (6 runs) | 0.70 to 0.72 | 2.19 to 3.89 | 2.19 to 3.89 | 1.97 to 2.62 | 0.70 to 1.63 | 0.70 to 1.63 | 0.70 to 1.63 | 0.70 to 1.63 |

**Reading.** Between P-cores, and between a P and an E core, the knee is at **64 bytes** in every run: 64 is as good as 128 or 256. Below 64 it is 3 to 5 times slower.
Between E-cores the time from 64 up is either 0.70 (the one-thread time) or 1.63 ns, from run to run and **not by stride** (the same stride gives both): so the 1.63 is not
false sharing (for which a larger stride would help) but something else on the E-core pair (which pair of the cluster, the placement of the page); this test cannot say what.

**What this does not show.** One CPU. A store-and-load test: the adjacent-line prefetcher's effect on **reads** of the neighbouring line (a consumer polling a cursor next to a line a
producer writes) is not measured here, and is the case the 128-byte folklore is about. Until a read-polling variant and the other targets (Zen, Apple M, a Neoverse) are measured the
table in `compiler/types/targets.fib` keeps its conservative values: 128 for x86-64 and AArch64, 64 for the rest.
