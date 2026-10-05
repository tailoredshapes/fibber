# Atoms under contention (P-count-a)

Package P-count-a of docs/design/parallelism.md (2.6, 3.6). Machine: i7-14700KF, 8 P + 12 E cores (28 CPUs), shared with other agents (load average 4 to 6 during these runs: the 28-thread rows move by 10 to 20%).
Commands (each row under `flock /tmp/fibsuite.lock`, `ulimit -v 16000000`, `MALLOC_ARENA_MAX=2`, median of the three rounds the program runs):

```
scripts/bench/atoms.sh                       # the table below; FIBC = a stage 2 built from this tree
docs/design/parallelism/proto/p5.fib         # the BEFORE rows: built with a stage 2 of main 7fa774b, run as ./p5 MODE T 200000
```

T tasks each do N = 200 000 operations on one shared `(atom 0)` (or one `fib.adder`); ms.

| mode | T=1 | 2 | 4 | 8 | 16 | 28 |
|---|---:|---:|---:|---:|---:|---:|
| `@a`, shared, before (spinlock) | 1.14 | 2.75 | 11.3 | 66.5 | 415 | **1019** |
| `@a`, shared, after (atomic load) | 0.12 | 0.13 | 0.16 | 0.24 | 0.57 | **4.07** |
| `swap!`, shared, before | 2.22 | 6.00 | 22.3 | 129 | 1369 | **2987** |
| `swap!`, shared, after (cmpxchg loop) | 2.07 | 9.63 | 22.2 | 44.5 | 134 | **326** |
| `compare-and-set!` loop, shared, after | 1.45 | 11.0 | 19.2 | 46.8 | 90.7 | **229** |
| `inc!` of one `fib.adder`, after | 3.26 | 4.19 | 4.34 | 4.86 | 7.17 | **41.4** |
| `swap!`, own atom, before / after | 2.50 / 1.53 | 2.17 / 1.60 | 2.55 / 1.59 | 2.59 / 2.06 | 4.04 / 2.95 | 5.73 / 5.10 |
| `@a`, own atom, before / after | 1.05 / 0.20 | 1.10 / 0.12 | 1.14 / 0.19 | 1.13 / 0.28 | 2.17 / 0.35 | 3.96 / 0.49 |

(The after rows of the scalar atoms were measured with the lock of the object atoms still a test-and-test-and-set one; the scalar paths do not use the lock, and the code of those paths is the code shipped.)

What it shows:

- **Reads**: 250 times faster at 28 tasks. At N = 200 000 a task's whole run is 0.1 ms, so the 28-task row (4 ms) is mostly the cost of starting 28 threads; the readers do not slow each other (T = 1 to 8: 0.12 to 0.24 ms).
  The design's bound "28 readers < 20 x one reader" holds only if the thread start-up is counted out; I did not measure a longer run (N = 2e7) because the benchmark lock was held by other jobs for the time I had.
- **`swap!` on ONE atom** is 9 times faster at 28 tasks and still collapses with the task count: all compare-and-swaps hit one cache line (326 ms for 5.6 million swaps, 58 ns each), and it is slower than the old lock at T = 2 (9.6 against 6.0 ms; one run, noisy: not investigated, and no exponential back-off was added: a failed swap retries at once). The way out is not to share one counter: `fib.adder` is 8 times faster than one atom at T = 28 and flat to T = 16.
- `swap!` and `@a` on an atom of one's own are not slower (the fast paths are shorter).
- No update was lost in any of these runs: the program checks the final value (`v`).

## Object atoms: the lock

An atom of an object (a `Vec`) keeps the lock: the retain of the loaded pointer must be atomic with the load, and a lock-free design needs hazard pointers or epoch reclamation, which are not built. `@a` of a `(Atom (Vec i64))` followed by `count` (T tasks, 100 000 each; ms; same program, only `fib.lock` replaced in the emitted lIR, `p5o`):

| lock | T=2 | 4 | 8 | 28 |
|---|---:|---:|---:|---:|
| shipped: `cmpxchg`, `sched_yield` after each failure | 5.0 | 22 | 105 | 1928 |
| test-and-test-and-set, K 16 doubling to 1024, then yield | 19 | 62 | 175 | 1800 |
| same, K 16 to 64 | 14 | 71 | 190 | 2258 |
| 4 loads, then yield, each round | 6.2 | 34 | 165 | 2122 |
| 32 loads, then yield, each round | 12 | 46 | 166 | 1947 |
| K 4 doubling to 16 | 14 | 47 | 172 | 2381 |

None is better at 28 tasks (within the noise) and the spinners are 1.5 to 4 times slower at 2 to 8. What a waiter waits for is the holder's retain of the Vec's shared count, a contended atomic. So the lock was left as it was (comment in rt/atom.lir), and the cost of object atoms is the count: P-count-b (`freeze`, borrowed reads). A `swap!` of an object atom (`conj` then `pop` on a Vec, 20 000 each) at T = 2, 4, 8: 12, 49, 179 ms with the shipped lock, similar with the others.

## Checks

- `cases/stdlib/7620` to `7625` (stage 2): every scalar type; object `compare-and-set!` and its counts (audit clean); 120 000 `swap!` increments from six tasks (i64, f64, a toggled bool) with a monotonic-reads watcher; 200 one-winner races and an 18 000-ticket dispenser; object atoms under `swap!` and `compare-and-set!`; the adder.
- `scripts/mutant-atoms.sh MODE` plants one fault per run, builds a stage 2 from the copy and requires its cases to fail: `cas-blind`, `cas-says-true`, `swap-no-retry`, `read-first`, `cas-obj-leak`, `cas-obj-keep-old`, `cas-obj-bits`, `lock-none`, `unlock-none`, `adder-lost`, `adder-one`: all killed (some by wrong results, some by the audit, `unlock-none` and `read-first` by the timeout).
- `scripts/tsan-atoms.sh`: cases 7620 to 7625 under ThreadSanitizer: no reports; the planted race (`TSAN_SED='s/\(atomic-store seq_cst /(store /'`, plain stores of scalar atoms) is reported (8 reports, exit 66). TSAN cannot see a weakened ordering among atomics: the orderings are argued in `compiler/emit/lower/atoms.fib` (every access `seq_cst`).
