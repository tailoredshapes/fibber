# The large-block cache (ALLOC-1)

Status: design of the runtime package ALLOC-1, written before the code. The owner's line: "Decay first and then scope arena": this
package is the decaying cache; the scope arena is a later, separate package (section 7 says what it needs from this one).
Where this file and `rt/core.lir` disagree, the code is the truth and this file is to be fixed.

## 1. The problem

An object of more than 512 bytes is a plain `malloc` block (`fib.alloc-slow`, branch `big`) and goes back to `free` when it dies.
glibc serves a request of 128 KiB or more (its `M_MMAP_THRESHOLD`, which it slides upwards after a free) with `mmap`, and its
`free` is an `munmap`. A tensor program makes and drops blocks of one size again and again: each 800 KB result of the Adam step
(784x256 f32) is a fresh mapping, a page fault for every 4 KiB page that is written, and an `munmap` (docs/shootout/autodiff.md
section 8.3: with glibc's thresholds raised by environment the Adam step went from 1.20 to 1.00 ms; an experiment, not a profile).

## 2. The cache

- **Which blocks.** A request of `fib.lc-floor` bytes or more (header included), up to 2^47 bytes. The floor is 128 KiB: below it glibc's
  own bins already reuse blocks without a system call, so a second cache there would only duplicate them. In a static (musl) executable
  the floor is 4 KiB, because musl's malloc does not (section 9).
- **Size classes.** Four per power of two: a class size is `(k+1) * 2^(e-2)` for k in 4..7 (128 KiB, 160, 192, 224, 256 KiB, ...),
  148 classes in all (from 1.25 KiB: the table reaches below the 4 KiB floor). A request is rounded **up** to its class and that many bytes are asked of `malloc`, so every block of a class
  can serve every request of it. The waste is under 25% of the request, and for a mapped block the pages past the end that are never
  written are never faulted in. A freed block is filed **down**: its class is the largest class size not above
  `malloc_usable_size` (Darwin: `malloc_size`), so a block that came from anywhere (an older runtime, a JIT module, a block made
  before the cache was enabled) is never filed in a class bigger than itself.
- **The blocks stay `malloc` blocks.** Nothing is added in front of the object and no flag bit is used: an object made through the
  cache can be freed by any runtime with `free` (the compiler's macro modules run the runtime of the tree while the compiler itself
  runs the seed's, and objects pass between them), and the cache can take any object's block. The cache is a list per class through
  the first two words of each dead block (the next block, the time it was filed).
- **One global cache, one short lock.** `fib.lock` (the atoms' `cmpxchg` spinlock with `sched_yield`) around a pop or a push: a
  few loads and stores, never a call. Per-thread caches were not built: a block made in one task is often freed in another
  (parallelism.md: a pool worker makes a chunk, the joiner drops it), and a per-thread cache then needs stealing to be any use. A
  large block costs microseconds to fill, so a lock of tens of nanoseconds per large allocation is not the cost to remove.
- **Trace mode** (`FIB_TRACE`, the memory audit) uses the cache too, for the block in front of which it keeps the ordinal: the `A`
  and `F` lines are the program's objects, which are made and dropped exactly as before; a cached block is runtime memory and is in
  no trace line, so the audit cannot count it as a leak.
- **Zeroing.** None. No object path relies on memory from `malloc` being zero (`malloc` never promised it): `(array n x)` stores
  `x` in every element (`array-fill` in `compiler/emit/lower/builtins.fib`), `array-uninit-*` leaves the elements to the caller, and
  the program's `(alloc n)` is `calloc` and is not an object. A case keeps this true (a dirty reused block read through
  `(array n 0.0)` reads zeros).
- **Huge pages.** A block of 4 MiB or more is still advised (`fib.advise-huge`) after it is obtained; a cached block keeps its
  huge pages, and decay returns them with the block.

## 3. Decay and the cap

A block filed more than `FIB_ALLOC_DECAY_MS` milliseconds ago (default **1000**) is given back with `free`. With glibc's threshold
pinned at 128 KiB by `mallopt(M_MMAP_THRESHOLD)` when the cache is on (so glibc no longer slides it up), every cached block is its
own mapping and its `free` is an `munmap`: the memory goes back to the system at once. On Darwin there is no `mallopt` (the call is
dropped for that target, emit.os) and the system allocator returns large blocks itself.

**Why 1 s.** A loop that reuses a size does so in microseconds to milliseconds (an Adam step is 1 ms), so any decay above about
100 ms catches it; a server that sees a large request now and then should be back to its baseline soon after the spike, and a second
is the scale at which a person or a monitor looks at RSS. jemalloc's default dirty-page decay is 10 s; we are more eager because
our blocks are whole mappings (no fragmentation to wait out) and a miss costs only what every allocation costs today.

**No background thread.** The clock is read (`CLOCK_MONOTONIC_COARSE` on Linux, `CLOCK_MONOTONIC_RAW_APPROX` on Darwin: a few
nanoseconds, no system call) only while the cache holds something, at these points: every large allocation and free, every
allocation and free of a block above 512 bytes (already a `malloc` or `free` call), every small block that comes from `malloc`
(`fib.alloc-slow`'s `fresh`: every small allocation once the program has threads, a free-list miss before), every 256th small free
while the program has one thread (a plain countdown in `fib.free-block`'s push), and the return of `sys-sleep`. While the cache is empty each point costs one atomic load. When the time passed is past the next deadline (the
oldest block's filing time plus the decay), one sweep under the lock takes every expired block out and frees them after the lock is
released. The inlined fast path (a pop from a free list) does not read it. The `malloc` of a small block was added after the first
measurement: the threaded HTTP server's small requests allocate nothing above 512 bytes, so without it a spike's blocks stayed
cached for good (section 8). A program that does nothing at all keeps its cache until its next allocator event, as jemalloc without background threads does;
`fib.os.memory/trim!` gives everything back at once. A thread was rejected: it would make the program multi-threaded (`fork-run` refuses then) and
a lock held at a `fork` would be inherited by the child.

**The cap.** `FIB_ALLOC_CACHE_MAX` bytes (default **1 GiB**): a block that would take the cache above it is freed at once. The
cache never holds more than the program freed within the last decay period, so in steady state it is bounded by the program's own
churn; the cap is for a phase change (one size class freed by the gigabyte while the next phase allocates another) and for a
machine with little memory. 1 GiB is far above the tensor workloads measured (tens of MB) and small next to the 16 GB the tools
run under; a program for which it is wrong sets the variable.

`FIB_ALLOC_DECAY_MS=0` turns the cache off: every block is `malloc` and `free` as before, and glibc's threshold is left alone.

## 4. The interface

- Environment, read at start-up (`fib.init`): `FIB_ALLOC_DECAY_MS` (milliseconds; 0 = no cache), `FIB_ALLOC_CACHE_MAX` (bytes),
  `FIB_ALLOC_STATS` (set: one summary line on standard error when `main` returns). A value that is not a decimal number is ignored.
- `fib.os.memory` (`lib/fib/os/memory.fib`): `(trim!)` gives every cached block back and returns the bytes; `(alloc-stats)` is an
  `AllocStats` (cached and peak cached bytes, hits, misses, bytes and blocks returned, blocks refused by the cap, the decay and the
  cap in force); `(configure! decay-ms cap)` sets both (0 ms empties and disables the cache); `(configure-from-env!)` reads the two
  variables again, as start-up does; `(rss-bytes)` is `VmRSS` from `/proc/self/status`, `nil` where there is none (Darwin).
- How the library reaches the runtime: the runtime marks one function, `fib_alloc_ctl (op a b) -> i64`, with a comment line
  `;; extern: (declare ...)`; `emit.rt`'s `runtime-declaration` reads that line, so the library's `extern` of the name binds to the
  runtime's definition and is not declared again (no new builtin or core name, nothing in the language server's lists).

## 5. Tests (cases/stdlib 7790-7809, stage 2)

Reuse (hits in a loop of same-size allocations); decay (RSS back down after the decay time, Linux; elsewhere the RSS part is
skipped and the stats part still runs); `configure!` and `FIB_ALLOC_DECAY_MS=0` (through `configure-from-env!`) turn caching off;
a dirty reused block read through `(array n 0.0)`; every class boundary written whole and checked (a too-small block would be
caught); a block freed on another thread and reused (also under ThreadSanitizer, scripts/tsan-suite.txt); the audit stays clean.
Planted faults are listed with their outputs in the commit messages.

## 6. Not done

Per-thread caches; a class larger than the request served from a neighbouring class; a background decay thread; `madvise`-based
partial return of a cached block.

## 7. What the scope arena will need from this package

The arena (a region whose objects all die at its end, freed as one) will take its chunks from `fib.lc-get` and give them back with
`fib.lc-put`, so that an arena opened and closed in a loop reuses the same chunks with no system call, and its chunks decay like any
other block. It needs: (1) the get/put pair on raw blocks, not objects (here); (2) a way to tell an arena object from a heap one at
its drop: a flags bit (bit 5, beside ROOMY) or a class value, because `fib.free-block` must not `free` an interior pointer; (3) its
chunk size to be a cache class (a multiple of 128 KiB in this scheme) so chunks are never rounded; (4) the stats split by owner if
the arena's chunks are to be told apart from ordinary blocks in a measurement.

## 8. Measured (2026-10-06, x86-64 Linux, glibc; shared machine, load average 2 to 4)

Binaries: `F0` = stage 2 of main (700238b) built by the v0.1.7 seed (the runtime without the cache); `F1` = stage 2 of this branch
built by the seed (its programs get the cache); `F3` = this branch built by `F1` (the compiler itself runs with the cache). Every
timing is `ulimit -v 16000000`, `flock /tmp/fibsuite.lock taskset -c 3`, the median of 5 runs (each run's own number is a median of
its steps).

```sh
F build scripts/bench/autodiff/adam-chain.fib -I lib -o probe;  F build scripts/bench/autodiff/mlp.fib -I lib -o mlp
flock /tmp/fibsuite.lock taskset -c 3 ./probe;  flock /tmp/fibsuite.lock taskset -c 3 ./mlp tape adam 300   # and `tape sgd 300`
```

| ms per step | F0 | F0 with glibc's thresholds raised (the experiment of autodiff.md 8.3) | F1, cache on | F1, `FIB_ALLOC_DECAY_MS=0` |
|---|---|---|---|---|
| `adam-chain` (chained Adam update) | 0.293 (an earlier run: 0.337) | 0.113 | 0.114 (first build: 0.122) | 0.289 |
| MLP step, Adam (tape) | 1.202 (earlier: 1.206) | 1.001 | 1.010 (first build: 1.019) | 1.209 |
| MLP step, SGD (tape) | 0.910 | 0.912 | 0.921 (first build: 0.912) | 0.906 |

The cache gives what the glibc experiment gave: the chained Adam update 2.6x faster, the Adam step 16% faster (1.20 to 1.01 ms),
SGD unchanged (it allocates no large block per step). `FIB_ALLOC_STATS=1` on the MLP Adam run: hits 2990, misses 15, held 24 MB
at the end, nothing returned (the loop never pauses for a second).

**A server's resident memory after a spike** (`examples/http-server.fib`, 4 workers; a client sends small `GET /` at about 50 per
second for 12 s and, at 3 s, eight `POST /echo` of 6 MB one after the other; VmRSS sampled every 50 ms; script
`~/.cache/fibber-scratch/alloc1/httpload.py`):

| server built by | baseline | peak | 1 s after the spike | 2 s | 4 s | 8 s |
|---|---|---|---|---|---|---|
| F0 (glibc) | 2.2 MB | 38.2 MB | 2.5 | 2.5 | 2.5 | 2.5 |
| F1, first build (checks only at large and medium events) | 2.2 | 20.3 | 14.3 | 14.3 | 14.3 | 14.3 |
| F1 (check also at a small malloc) | 2.2 | 14.3 | 14.3 | 2.2 | 2.2 | 2.2 |

glibc returns 6 MB blocks at once (they are its own mappings), so the cache cannot beat F0's curve, only match it after the decay
time: it does, within 1.3 s of the spike's end, with a lower peak. The first build never returned the 12 MB: the threaded server's
small requests reached no check, which is why `fresh` (and, for one thread, the free countdown) check now.

**The language server** (`F3 lsp -I compiler`, a 2 MB buffer opened, then a hover every 100 ms for 8 s; script
`~/.cache/fibber-scratch/alloc1/lspload.js`): RSS 52 MB before the open, 102 MB after it and flat with the cache off, 105 MB and flat
with it on. The 50 MB are the analysis the server keeps for the open document, not freed blocks; `FIB_ALLOC_STATS` at exit: hits 342,
misses 8, 8 MB held (freed as the server shut down). There is no spike for the cache to return here; it costs 3 MB while the decay runs.

**The small-object path** (`BENCH_FIBC=F scripts/bench/quick.sh -n 5`, F0 then F1 back to back): every row of F1 within 7% of F0's
(num-f64 1.49/1.46, nbody 0.62/0.61, vec-conj-pop 0.89/0.85, vec-index 0.37/0.38, vec-sort 0.41/0.43, map-assoc-get 0.26/0.28,
set-conj 0.24/0.22, lazy-fused 0.84/0.77, strings 0.63/0.66 s); lazy-fused is flagged against the checked-in baseline for both.

**macOS** (the Mac Studio, arm64, macOS 27): cases 7790-7796 cross-built by F1 (`--target arm64-apple-macosx13.0.0 --emit obj`),
linked with `cc` on the Mac and run natively: all exit 0; 7791 checks the cache's own counts there (no /proc), and its stats line shows
the 32 blocks returned after the sleep.

## 9. The musl floor (SMALL-1)

STATIC-1 measured the `strings` benchmark 94% slower in a static musl executable (0.73 s against 1.42 s). `strace -c` of the two
builds (the same code, x86-64-v3):

| | glibc | musl |
|-|-:|-:|
| mmap | 17 | 24,339 |
| munmap | 5 | 14,304 |
| brk | 2,070 | 2,417 |

The cause: musl's malloc (mallocng) serves a block of about 2 KiB and more from a group it maps for it (the sizes mapped: 4, 8, 32 and
64 KiB for the blocks from 2 KiB to 128 KiB), and unmaps the group when its last block is freed. A string built by repeated
concatenation (`grow`: 20,000 pieces, a new block of a larger size each time, each dropped at once) maps and unmaps at every step.
glibc serves the same blocks from its heap (`brk`). The cache's floor of 128 KiB did not reach them: with the cache's counts on
(`FIB_ALLOC_STATS=1`) the musl run had 3 hits and 5 misses in all, and mmap sizes under 128 KiB were 24,333 of the 24,339 calls.

The fix: the floor is a global (`fib.lc-floor`, 128 KiB), which `emit.os` sets to 4 KiB when the target is musl (the same place that drops
`mallopt`); `fib.os.memory/cache-floor` reads it. The classes start at 1.25 KiB (index 0), so the table is 148 entries.
Measured with a floor knob (a temporary environment variable, removed), mmap/munmap of the musl `strings` run by floor:

| floor | mmap | munmap |
|-:|-:|-:|
| 128 KiB (before) | 24,339 | 14,304 |
| 64 KiB | 24,338 | 14,302 |
| 32 KiB | 15,258 | 5,220 |
| 16 KiB | 12,769 | 2,730 |
| 8 KiB | 11,572 | 1,533 |
| 4 KiB | 10,650 | 610 |
| 2 KiB | 10,168 | 128 |

The mmaps that remain are growth of live data (a million small strings: groups that stay), which glibc serves with `brk`. 4 KiB is
the floor taken: 2 KiB saves 480 more munmap calls and caches smaller blocks, a lock and a rounding for each. glibc keeps 128 KiB:
a floor of 8 KiB or 16 KiB there changed no system call count (brk 2,067 to 2,070) and no time (median of 5, `strings`: 0.657 s at 128 KiB,
0.664 at 16 KiB, 0.666 at 8 KiB, 0.656 at 4 KiB).

Time, `scripts/bench/static-compare.sh` (medians of 3 or 7, x86-64-v3, the same machine, before and after; the dynamic column is glibc):

| benchmark | glibc before | glibc after | musl before | musl after |
|-|-:|-:|-:|-:|
| strings | 0.652 to 0.685 | 0.657 to 0.672 | 1.200 to 1.237 | 1.091 to 1.120 |
| set-conj | 0.225 to 0.249 | 0.222 to 0.244 | 0.228 to 0.249 | 0.230 to 0.273 |
| binary-trees | 1.315 to 1.318 | 1.332 to 1.382 | 1.385 to 1.430 | 1.390 to 1.472 |
| vec-conj-pop | 0.838 to 0.847 | 0.842 to 0.854 | 0.946 to 0.975 | 0.951 to 0.960 |

(ranges over the runs of the script; the order of the rows of one benchmark between runs is noise, 5% to 10%.) The musl `strings` run
is 8 to 9% faster, `sys` time 0.195 s to 0.086 s; the gap to glibc went from +80% to +65%. The rest is user time (1.00 s against 0.57 s):
musl's malloc and `memcpy` themselves, which no cache in the runtime changes; not profiled (`perf` is not allowed on this machine).

Tests: case 8060 (blocks of 104 KB are hits where the floor says they are cached, and under `FIB_STATIC_CASES=1`, which the gate's
static stage sets, the floor must be at most 100 KiB), case 8061 (every class from 5 KiB to 128 KiB written whole and read back), and
`scripts/mutant-alloc-floor.sh` (the floor back at 128 KiB; a freed block filed in the class above its size; a `cache-floor` that lies).
