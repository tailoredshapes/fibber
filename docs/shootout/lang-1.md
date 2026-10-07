# LANG-1: the field move, the in-place optimiser step, the scaling of `pmap`, and the shootout outliers

Measured 2026-10-07. Numbers marked **Ryzen** are from the quiet AMD Ryzen 7 5800X (WSL2, 16 threads, 15 GB), binaries built with `FIB_TARGET_CPU=x86-64-v3`
(`~/.cache/fibber-scratch/LANG1/build-ry.sh`, `ry-run.sh`); "old" is the lib of commit 68e4cf6 built by the same compiler, "new" is this change. Numbers marked **dev**
are from the shared development machine (i7-14700KF, loaded: a difference under 10% is not established there). Every checksum or md5 of old and new agreed.

## 1. Field moves and the in-place Adam step

The rule is `spec/types.md` 6.3 "Field moves". Before it, `(. p buf)` at the last use of an owned `p` was retained (count 2) and every write copied; the constructor pattern
(`(match p ((P buf n) ..))`) already stole. Cases `cases/ownership/379` to `383`; mutants `scripts/mutant-fieldmove.sh` (7, all killed: no-later-read by 381, always-steal by 264/266/380,
borrowed-shell by 383, part-of-shell by 379/382, no-rule4 by 379, no-own by 8200/8202, take-always by 8203).

| program | before | after |
|---|---|---|
| `cases/ownership/379` (100 calls of `bump`, objects allocated, `fibc run --trace`) | 202 | 102 |
| 100 chained `t/adam-step-owned` on 64 `f64` (case 8201, objects) | 7058 (`adam-step`) | 1955 |
| `scripts/bench/autodiff/adam-chain.fib` (the 4 MLP tensors, chained state), ms per step, **Ryzen** | 0.214 | **0.074** (2.9x) |
| the same, **dev**, median of 3 to 5, pinned | 0.147 to 0.156 | 0.108 to 0.116 |
| `scripts/bench/autodiff/mlp.fib tape adam`, objects per step (**dev**, `FIB_TRACE`) | 2152 | 1824 |
| the same, ms per step, median of 5 (**dev**) | 1.16 | 1.08 |

The loss curve and checksum of `mlp.fib` (40 steps) are bit-identical before and after. The remaining 1824 objects per step are the tape and the kernels' descriptors; the parameters and
moments are no longer copied (the buffers' counts are 1 at the kernel: probed with `raw`). Two things had to be right and were not at first: a protocol method's parameter is borrowed unless it
says `:owned` (so the shell reached the kernel with count 2), and a kernel that reads and writes one array through cells (`@c` peeks, `&c` writes) was 40% slower at 1e6 elements than the raw-pointer
loop the other `fib.tensor` kernels use, which is what `fib.tensor.optim-owned` uses after one unique-write test of each buffer.

Not done: a field read at a `let` or `loop` initialiser (they retain), a field of an owned temporary, a path `(. (. o in) buf)` (cases 382), `with` on a unique record whose new value reads the
old one, and rule 4 for the parameters of protocol methods. A candidate finding, not investigated further: `(let [vn (if (= s 0) b (copy b))] ..)` with `b` a borrowed `(Array i64)` parameter
double-freed in `fib.bigint.magdiv` (the cases 6001, 6002 and 6016 abort with `free(): double free`); the bigint change below keeps the copy.

## 2. Why `pmap` does not scale at 1e8 elements

`scripts/bench/parallel-scale.fib MODE W N` prints the phases of a call. At 1e8 elements the `pmap` of the AWS round was 1.2x to 1.4x at best. The measured cause is **serial assembly**, not the
allocator: the workers make chunk arrays in parallel, then the caller copies all 1e8 elements into one array (`concat-chunks`, an `array-set!` per element) and builds the `Vec` of it
(`vec-from-array`, a leaf per 32 elements), and `parallel-closure.fib` adds a serial `reduce` over the result. **Ryzen**, 1e8, `(pmap-each [x xs] (* x 2))`, `call` is the pmap, `sum` the serial
checksum, ms:

| arena | variant | W=1 | 2 | 4 | 8 | 16 | best speedup | sum |
|---|---|---|---|---|---|---|---|---|
| default | old | 989 | 1547 | 1232 | 1195 | 904 | 1.1x | 421 |
| default | new | 1093 | 1000 | 782 | 567 | 465 | **2.35x** | 504 |
| `MALLOC_ARENA_MAX=2` | old | 980 | 1388 | 830 | 1016 | 950 | 1.2x | 413 |
| `MALLOC_ARENA_MAX=2` | new | 1072 | 1320 | 2081 | 1417 | 1153 | 0.93x | 440 to 810 |

**Not shipped; a measured prototype.** The prototype (`vec-chunk-leaves` and `vec-join-leaves` in the prelude, `pmap-chunks`; commit 2be99b2, reverted in the commit after it): from 8192 results the grain is a multiple of 32, each worker cuts its chunk into the leaves of the result,
and the caller joins one pointer per 32 elements. It scales with the default glibc arenas (2.35x at 16 workers; the rest of the call is thread start, the join, and the 3M objects freed later).
**With `MALLOC_ARENA_MAX=2`, which the AWS round used because a 32-core machine under `ulimit -v 16000000` cannot afford 8 arenas a core, it is worse than the old path from W=2**: 6 million small
`malloc` calls from 4 to 16 threads contend on two arenas. That is the allocator finding: in multithreaded mode every small allocation is a `malloc` (rt/core.lir: `fib.mt` turns the free lists
off), and a program that is bounded to two arenas serialises on them. The fix is a per-thread cache of small blocks (thread-local free lists with a flush when a task ends); it is **not** in this
change (the runtime fast path, the task entry and ADR 0018's decay would all change, and it could not be tested to the standard of the rest in the time of this package). Allocation-heavy
parallel work that does not assemble a result scales with either setting (`pfor-alloc`, 1e7 elements, each a Vec of 6: 1036 ms at W=1, 247 at 16, default; 238 at 16 with 2 arenas), and
`alloc` (the same per element, assembled by `pmap-range`) 1073 ms at W=1, 427 at 16 (old) and 301 (new). `preduce-range` 196, 101, 63 to 88, 45 to 50, 38 to 46 ms at W=1, 2, 4, 8, 16 (no result to assemble: unchanged).

It was reverted for two reasons. The `MALLOC_ARENA_MAX=2` regression above is a regression for the configuration the benchmarks and `ulimit -v` users run in. And ADR 0006 holds `lib/prelude.fib` to 721
lines (the allow-list only shrinks), and the two functions need the prelude's private `VNode` and `vec-trie`: they could not stay. Case 8205 (every size around the switch and a leaf boundary, W 1 to 8, grains
that are and are not multiples of 32) passed with the prototype and was removed with it; commit 2be99b2 has both. The shipped part of this section is the diagnosis and `scripts/bench/parallel-scale.fib`.
The order of work: the per-thread small-block cache first (it removes the arena effect for every parallel program), then the leaf assembly, which then needs no flag.

## 3. Shootout outliers

| benchmark | before | after | note |
|---|---|---|---|
| pidigits 10000, **dev**, pinned, 3 runs | 4.06, 4.16 s | 3.46, 3.50 s (1.19x) | md5 `5b185f9a..` as in `sizes.txt` |
| pidigits 10000, **Ryzen** | 6.15 to 6.17 s | 5.66 to 6.30 s | within the noise of that machine (the kernel is memory-bound there) |

pidigits: the limb loops of `fib.bigint.mag` and the multiply-subtract of `magdiv` read and write by raw pointer with unchecked adds (limbs are below 2^31, so no sum or product can overflow
an `i64`): no bounds check, no unique-write test and no overflow branch per limb. The limbs are 31 bits in 64-bit words, twice the limbs and twice the memory of GMP's, so the remaining gap
to C (which uses GMP) is mostly that; a 62-bit limb in a 128-bit multiply is the next step and was not attempted. The bigint cases 6000 to 6016 pass.

k-nucleotide was profiled by timing the operations (`~/.cache/fibber-scratch/LANG1/p/mapb.fib`, 140 000 keys, **dev**, ns per operation): `get` with a default 22, `contains?` 24, `assoc` of an
existing key 37 to 39 on a fresh trie, and 147 to 182 after the trie's nodes have been reallocated in a different order. So the cost is the pointer chasing of a 4-level HAMT whose nodes
are scattered (an update allocates 1.7 objects), not the hash and not the `Option` of `get` (it does not allocate: the counts per update are equal with and without it). The C twin is a flat
open-addressing table of 2 MB. A cheap general fix was not found; the candidates are flatter nodes (a leaf stored in its parent's array) and a Map that rebuilds into allocation order, both
changes to `fib.coll` that need their own package. The ratio (16x C on SPR) is unchanged.
