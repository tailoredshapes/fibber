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

- **Which blocks.** A request of `LC-MIN` = 128 KiB or more (header included), up to 2^47 bytes. Below 128 KiB glibc's own
  bins already reuse blocks without a system call, so a second cache there would only duplicate them.
- **Size classes.** Four per power of two: a class size is `(k+1) * 2^(e-2)` for k in 4..7 (128 KiB, 160, 192, 224, 256 KiB, ...),
  124 classes in all. A request is rounded **up** to its class and that many bytes are asked of `malloc`, so every block of a class
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
allocation and free of a block above 512 bytes (already a `malloc` or `free` call), and the return of `sys-sleep`. When the
time passed is past the next deadline (the oldest block's filing time plus the decay), one sweep under the lock takes every
expired block out and frees them after the lock is released. The small-object fast and slow paths do not read it. A program that
does nothing at all keeps its cache until its next allocation, as jemalloc without background threads does; `fib.os.memory/trim!`
gives everything back at once. A thread was rejected: it would make the program multi-threaded (`fork-run` refuses then) and
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
