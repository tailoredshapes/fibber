# fib.tensor against NumPy (package TP1)

How far `lib/fib/tensor` is from NumPy on the kernels of AI and scientific code, what limits each kernel in the emitted code, and what
the compiler-side fixes of this package bought. The programs are `scripts/bench/tensor/` (the fibber program `bench.fib`, the NumPy
program `bench.py`, the driver `run.py`); the owner's earlier driver `scripts/bench/tensor.py` (60 small and medium workloads) is
untouched.

## 1. Method

```
# stage 2 of this tree, built by the v0.1.5 seed (scripts/fetch-seed.sh)
ulimit -v 16000000
FIB_LIB=$PWD/lib ~/.cache/fibber-scratch/seed-main/fibc-0.1.5-linux-x86_64/bin/fibc build compiler/fibc.fib -I compiler -I lib \
    -L /usr/lib/llvm-21/lib -l LLVM-21 -o F
# the table: builds bench.fib with F, runs every kernel in both programs
python3 scripts/bench/tensor/run.py --fibc ./F --openblas ~/.cache/fibber-scratch/tp1/ob --tsv rows.tsv
# one kernel by hand (the same arguments for both programs: KERNEL N RUNS)
flock /tmp/fibsuite.lock ./bench matmul64 1024 6           # F build scripts/bench/tensor/bench.fib -I scripts/bench/tensor -I lib -o bench
flock /tmp/fibsuite.lock python3 scripts/bench/tensor/bench.py matmul64 1024 6
```

- One process per kernel and per program. It builds its inputs, runs the kernel 6 times (run 0 is the warmup and is dropped) and
  prints every time; the table is the **median of the 5 timed runs**, in milliseconds. Input construction, the checksum and the
  destruction of the result are outside the timed region; the allocation of the result is inside it (NumPy's too).
- Every process runs under `flock /tmp/fibsuite.lock` and `ulimit -v 16000000`. `OMP_NUM_THREADS`, `OPENBLAS_NUM_THREADS`,
  `MKL_NUM_THREADS` and `BLIS_NUM_THREADS` are 1 for both programs. Machine: i7-14700KF (AVX2 and FMA, no AVX-512), 28 cores,
  shared with other agents (the lock excludes other benchmarks, not other work), THP `madvise`.
- Same inputs: element `i` of an input is `((i mod 97) - 48)/8` (flavour 0), `((7i mod 89) - 44)/16` (1) or `((5i mod 83) - 41)/32` (2),
  computed in integers and converted, so every value is an exact dyadic fraction in f32 and f64 and both programs build the same bytes.
- **Checksums:** the sum of squares of every result element accumulated in f64 (a scalar kernel: the value). They are compared with a
  relative tolerance of **1e-9 for f64 results and 1e-4 for f32 results** (an f32 matmul sums in another order than BLAS). Every row
  of every table below passed; `run.py` exits 1 and prints `FAIL` otherwise. The data make most of them agree to all printed digits
  (matmul, a*b+c, sums, max); softmax's differ in the 13th digit (libm `exp` against NumPy's SIMD `exp`).
- **NumPy's BLAS, and which one this is.** The numpy on this machine (2.3.5, Debian) is linked to the *reference* BLAS
  (`/usr/lib/x86_64-linux-gnu/blas/libblas.so.3.12.1`, Netlib Fortran, not tuned, no threads). That is the column "numpy (system
  BLAS)". It is not what NumPy users get from a wheel. **OpenBLAS is hand-tuned assembly** (per-microarchitecture GEMM micro-kernels
  with FMA, prefetch, packing), so a second column runs the same NumPy against OpenBLAS 0.3.29 (single thread), put first on
  `LD_LIBRARY_PATH` as `libblas.so.3` from a copy found on this machine (a Steam runtime), in `~/.cache/fibber-scratch/tp1/ob`; it is
  the meaningful comparison for matmul and the MLP. Nothing else in these benchmarks goes through BLAS.
- **All threads:** not produced. `fib.tensor` has no tasks (`grep` of `lib/fib/tensor` for `spawn`, `async`: nothing), and this package
  does not add parallelism, so there is no second fibber column. For context only, NumPy+OpenBLAS with all threads is not measured either.

## 2. Results

Median of 5 timed runs, milliseconds, single thread, every checksum compared (section 1).

| kernel | numpy (system BLAS) | numpy + OpenBLAS | fibber before | fibber after | after / numpy | after / OpenBLAS | checksums |
|---|---:|---:|---:|---:|---:|---:|---|
| matmul f64 512 | 21.0 | 4.05 | 6.11 | 6.05 | 0.29 | 1.49 | ok |
| matmul f64 1024 | 193.3 | 32.0 | 55.0 | 46.0 | 0.24 | 1.43 | ok |
| matmul f64 2048 | 2884.5 | 254.3 | 483.2 | 453.9 | 0.16 | 1.78 | ok |
| matmul f32 512 | 11.6 | 2.01 | 3.00 | 3.36 | 0.29 | 1.67 | ok |
| matmul f32 1024 | 90.1 | 15.2 | 25.7 | 29.0 | 0.32 | 1.91 | ok |
| matmul f32 2048 | 1027.1 | 111.7 | 216.3 | 234.9 | 0.23 | 2.10 | ok |
| a*b+c f64 1e7 | 19.4 | - | 45.8 | 22.9 | 1.18 | - | ok |
| a*b+c f64 1e8 | 189.2 | - | 417.5 | 273.6 | 1.45 | - | ok |
| a*b+c f32 1e7 | 8.55 | - | 23.6 | 12.0 | 1.40 | - | ok |
| sum f64 1e8 (ordered t/sum) | 37.2 | - | 417.2 | 102.1 | 2.75 | - | ok |
| sum f64 1e8 (t/sum-fast) | 41.2 | - | 37.4 | 35.5 | 0.86 | - | ok |
| mean f64 1e8 | 42.6 | - | 451.3 | 106.1 | 2.49 | - | ok |
| max f64 1e8 | 41.5 | - | 468.2 | 154.5 | 3.72 | - | ok |
| sum axis 0, 4096x4096 | 6.75 | - | 107.9 | 21.9 | 3.25 | - | ok |
| sum axis 1, 4096x4096 | 7.11 | - | 19.3 | 20.9 | 2.94 | - | ok |
| max axis 0, 4096x4096 | 7.26 | - | 110.1 | 33.3 | 4.58 | - | ok |
| max axis 1, 4096x4096 | 6.88 | - | 32.8 | 30.5 | 4.43 | - | ok |
| broadcast add 4096x4096 + row | 21.7 | - | 36.6 | 23.3 | 1.07 | - | ok |
| softmax rows 4096x4096 f64 | 111.0 | - | 309.9 | 251.2 | 2.26 | - | ok |
| layernorm 4096x1024 f64 | 26.7 | - | 58.0 | 32.3 | 1.21 | - | ok |
| MLP f32 b256 784-512-10 | 8.26 | 1.63 | 5.68 | 5.52 | 0.67 | 3.38 | ok |

"fibber before" is stage 2 of main (commit 057a1d8) built by the v0.1.5 seed; "fibber after" is stage 2 of this branch (five commits, section 5)
running the same `bench.fib`. "numpy" columns are medians of the same 5 timed runs of `bench.py`. Ratios below 1 mean fibber is faster.
Sizes are the ones asked for: matmul 512/1024/2048 in f64 and f32, elementwise over 1e7 and 1e8, reductions over 1e8, axis reductions and
broadcast on 4096x4096, softmax over the rows of 4096x4096, layernorm over 4096 rows of 1024, an MLP forward pass (batch 256, 784-512-10).
Two kernels are in the table twice on purpose: `t/sum` is the ordered scalar sum (the library's default and contract), `t/sum-fast` is the
four-accumulator SIMD sum that NumPy's pairwise sum corresponds to.

## 3. How far from NumPy, in one paragraph per class

- **Matmul:** fibber is 3 to 5 times faster than the NumPy that is installed here (reference BLAS) and **1.4 to 2.1 times slower than
  OpenBLAS**, the gap growing with the size (f64 1.5, 1.4, 1.8; f32 1.7, 1.9, 2.1; matmul code is unchanged between before and after, so
  the +-10% differences between those two columns are the machine's noise). Against a tuned BLAS NumPy is still ahead; against
  an untuned one it is not.
- **Elementwise a*b+c and broadcast add:** 1.2 to 1.45 times NumPy for a*b+c (was 2.2 to 2.5 before the huge-page change), broadcast
  add 1.07.
- **Reductions over a vector:** the SIMD `sum-fast` is at NumPy's speed (0.9). The ordered `sum`, `mean`, `maximum` are 2.5 to 3.7 times
  slower (they were 10 to 12 times): an ordered floating-point sum is a serial chain of adds, which NumPy's pairwise sum is not.
- **Axis reductions:** over the last axis 2.8 to 4.5 times NumPy; over the first axis they were 18 and 14 times and are now 3.3 and 4.6. These are library loops with a closure call per element.
- **Softmax** 2.4 times (a scalar libm `exp` per element, through `map`), **layernorm** 1.2, **MLP** 0.66 of the reference-BLAS NumPy
  and 3.4 times the OpenBLAS one.

So, for the project goal: on matmul and on bandwidth-bound elementwise work fibber is within a small factor of NumPy and ahead of an
untuned BLAS; it is not yet at parity with OpenBLAS-backed NumPy; and the reduction and `map`-style kernels written as generic closures
are the furthest behind.

## 4. What limits each kernel, ranked

Evidence is marked: **[asm]** read from the emitted assembly (`objdump -d -M intel` of the built benchmark, everything inlined into the
kernels), **[measured]** an experiment with a number, **[inferred]** from code reading and the figures, not isolated.

1. **Fresh large outputs fault in 4 KiB pages (no transparent huge pages): elementwise, broadcast, layernorm.** [measured] a*b+c over 3e7
   doubles took 121 ms; the same binary with `GLIBC_TUNABLES=glibc.malloc.hugetlb=1` took 68 ms (NumPy 56 ms). NumPy `madvise`s its
   arrays of 4 MiB or more for huge pages; the fibber runtime's `malloc` blocks are not. Fixed in the runtime (commit 3).
2. **Per-element index decode in the generic library loops: ordered `sum`/`mean`/`maximum`/`minimum`, `map`, `map-as`, `zip-with`, `to-array`,
   `to-vec`, `copy`.** [asm] `fib.tensor.layout.flat-offset` is out of line and per call re-derives `meta-size` (a loop over the rank with
   a negative-extent check), checks the index range, then runs a loop with one `idiv` or `div` per axis and checked `imul`/`add`; [measured]
   `t/sum` over 1e8 took 417 ms, 4.2 ns an element, against 37 ms for `sum-fast`. `fold` now reads a dense tensor straight from its buffer
   (commit 2): 101 to 120 ms. **Still open:** `map-as`, `zip-with`, `copy`, `to-array`, `to-vec`, `mean-axis` use `flat-at` too (softmax's
   `map` over 16.7M elements pays this) [inferred for softmax: 16.7M x ~4 ns is 67 ms of its 245 ms].
3. **No fused multiply-add in the matmul tile.** [asm] `gemm-f64.tile` has 20 `vmulpd`, 20 `vaddpd` and no `vfmadd`: every update is two
   uops where OpenBLAS issues one; the hot loop I read keeps 4 accumulators, fewer than the 8 that hide FMA latency on two ports. The
   machine's FMA rate is twice the mul+add rate. This is P4b's `fma` work (not touched here); the library change to use it is the next step.
4. **The first axis was reduced one output at a time: a column walk** [measured] `sum` over axis 0 of 4096x4096: 108 ms (4 KiB pages),
   217 ms once huge pages made the 32 KiB stride land in one cache set, 19.7 ms after the fix (commits 4 and 5); `maximum` 110, 206, 31 ms.
5. **Ordered floating-point reductions are a latency chain.** [asm] the `sum` loop is one `vaddsd` per element into one register (as the
   contract demands); at 4 cycles of latency and 1e8 elements the floor is about 73 ms at 5.5 GHz. `sum-fast` has no such floor and equals NumPy.
6. **Last-axis reductions:** one closure call, one `flat-offset` per output, a strided `array-get` with a checked multiply per element, no SIMD.
   [inferred] 18 ms against NumPy's 6.5; not changed.
7. **Scalar `exp`, `sqrt` through `map`.** `fib.math` has `sqrt` only (the benchmark declares its own `extern exp`, `scripts/bench/tensor/tb/libm.fib`);
   a vector `exp` does not exist, and `map` callbacks are not vectorised (README). [inferred] the rest of softmax's gap.
8. **Tail handling:** [asm] the remainder of each `kernel-f64.compute` loop uses `vmaskmovpd` + `vblendvpd` with a `vpmovsxdq` mask rebuild
   per iteration. Not isolated; affects small and odd sizes (the owner's driver covers them).
9. **Bounds checks and checked index arithmetic.** [asm] contrary to the SIMD note (docs/shootout/simd.md item 1) a counted loop that
   only *reads* arrays is already free of both in the emitted code: the loop of `(+ acc (array-get a i))` over `(array-len a)` is eight
   `vaddsd` from memory per trip with no compare or `jo` (LLVM proves `i < len`). The cost is in loops that **store**: see item 10. What is
   still there is the checked affine index (`k*nb + j`) in the SIMD matmul micro-kernel (`jo` after each `imul`/`add`, a compare per
   element); removing it needs a loop-versioning pass on the index expression and is not done.
10. **`array-set!` in a loop: the unique check, the length reload and the copy path.** [asm] the fill loop `(dotimes (i n) (array-set! &buf i ..))`
    reloaded the length, tested flags and count, and kept a call to `fib.array-slice` on a cold path that changes the loop-carried
    array pointer, per element: not vectorised. Fixed (commit 1) for loops that only write one cell.

Not a limiter, checked: matmul packing and the `sum-fast` / `dot-fast` loops are at or near the machine's rate; `sum-fast` is 0.91 of NumPy.

## 5. What was changed, and the evidence

Five commits on the branch after 057a1d8, in this order: 1 the hoist and 3 the runtime change are compiler-side; 2, 4 and 5 are library.

| commit | change | evidence |
|---|---|---|
| hoist the unique check and length of `array-set!` out of loops that only write one cell | `compiler/emit/lower/hoist.fib` (new), `builtins.fib`, `call.fib`, `cx.fib`, `mod.fib` | in-cache `x[i] = 0.5*x[i] + y[i]`, 32768 elements x 40000 passes (`scripts/bench/tensor/micro-write-loop.fib`): **1.04 s to 0.20 s**, same output; SIMD saxpy (scripts/shootout/saxpy/saxpy-simd.fib 32768 61035): **0.444 s to 0.308 s**, same md5; SIMD matmul 1024 0.414 to 0.408 s and SIMD n-body 5e6 0.183 to 0.178 s: no effect (their loops are not of this shape). The asm of the fill loop goes from scalar to `ymm` (`vmulpd`). |
| tensor: ordered `fold` reads a dense tensor straight from its buffer | `lib/fib/tensor/reduce.fib` | `t/sum` 1e8: 417 to 101 ms (table); `mean`, `maximum` likewise |
| runtime: transparent huge pages for blocks of 4 MiB and more | `crates/fibc/rt/core.lir` (`fib.advise-huge`), `compiler/emit/runtime.fib` regenerated (`compiler/tests/emit/runtime.sh`: "regenerated runtime.fib equals the committed file; 18 checks ok") | a*b+c 3e7: 133 to 67 ms; table: a*b+c 1e7 45.8 to 23 ms, broadcast 36.6 to 17.4 ms |
| tensor: axis reductions off the last axis | `lib/fib/tensor/reduce.fib`, `reduce-extra.fib` | `sum` axis 0: 217 to 19.7 ms, `max` axis 0: 206 to 31 ms (with huge pages in; 108 and 110 ms before) |

The hoist, precisely (header of `hoist.fib`): a loop whose body writes elements of a local cell `&c` with `array-set!` and uses `c` in no
other way (`(array-get @c i)` and `(array-len @c)` are the only other uses allowed; nothing in a closure, `async`, `await`, `&c` passed on,
`@c` bound, `array-push!`/`pop!`/`take!`, `set!`) cannot make the array shared, because nothing takes a count on it. The array is made
unique once before the loop (`array-own`, the copy the first write would make), its length is loaded once (also the length of every
immutable array local the loop reads with `array-get`/`array-len`), and each write is the bounds check against that length and the store.
Loops in async bodies are not touched. One visible difference: a zero-trip loop over a shared array still makes the copy.

**Tests** (all new, in this branch):

- `cases/ownership/330` trap at the computed index 6 of 5 (`array index 6 out of range 0..5`) after three good writes;
  `331` a snapshot holding the cell's array stays zero; `332` a snapshot taken inside the loop makes the loop not hoisted and the
  snapshot still reads the old value; `333` two cells sharing one array, only one written, plus hoisted length reads of `@a` and a plain
  array; `334` string elements released, nested loops hoist once (memory audit clean); `335` arrays one below, at and above the 4 MiB
  huge-page size and a 24 MB one are written and read whole.
- `cases/stdlib/7075` an ordered `sum` of a forged descriptor traps at the fifth read with `array index 4 out of range 0..4`;
  `7076` the axis reductions off the last axis keep index order (1e16, 1, -1e16, 1 sums to 1).
- **Mutants, each built into its own compiler and run on 330-334 (each caught):** (1) skip the make-unique step: 331, 333 and 334 fail
  (`result: expected 0, got 1`); (2) hoist every loop whatever it does with the cell: 332 fails; (3) drop the bounds check of the hoisted
  write: 330 fails (`expected a trap with "array index 6 out of range 0..5", but the run finished`). Reversing the step order in the
  axis walk: 7076 fails (7002 still passes, which is why 7076 exists). Case 335 is a regression pin for the huge-page advice, which is
  advice only: it cannot fail on a wrong constant, and no claim is made that it can.
- The gate: see the last section.

## 6. Not done

- No change to `compiler/emit/lower/simd*.fib`, `lib/fib/simd.fib` (P4b) and no use of `fma` in the matmul (blocked on P4b, then a
  library change); no new tile shapes.
- No loop versioning for affine index expressions (`k*nb + j`) in loops over several arrays, which is what is left of the bounds-check
  cost in the SIMD matmul micro-kernel. That is the next compiler item and needs a design (the trap must still fire at the right index).
- `map-as`, `zip-with`, `copy`, `to-array`, `to-vec`, `mean-axis` still decode every index with `flat-at`; a dense fast path like `fold`'s
  would speed softmax's `map` and the checksum helpers. Last-axis reductions, a vector `exp`, tail kernels are unchanged.
- No all-threads table (the library has no tasks). No comparison with MKL or a wheel's bundled OpenBLAS (the OpenBLAS here is 0.3.29
  from a Steam runtime, via `LD_LIBRARY_PATH`; it is hand-tuned assembly but I did not check which kernel set it picked for this CPU).
- The machine is shared with other agents; runs hold the lock, but other work was running (differences of about 5% are noise; one
  a*b+c 1e8 run took 502 ms against 257-273 ms in repeats, a compaction stall, not reproduced).
- THP `madvise` is the system setting here; on a system with THP `never` the huge-page commit does nothing.

## 7. The gate

`GATE_FRESH=1 FIBC=~/.cache/fibber-scratch/seed-main/fibc-0.1.5-linux-x86_64/bin/fibc scripts/gate.sh --full -j 8` on the final tree of this
branch (all five commits), run once at the end:

```
fixed point: F and F3 emit the same lIR (71.2 s)
311 cases: 311 pass, 0 fail, 0 pending, 0 header error        (ownership)
27 cases: 27 pass, 0 fail, 0 pending, 0 header error          (modules)
1185 cases: 1163 pass, 1 fail, 0 pending, 0 header error, 21 open   (stdlib; the one failure is 1707, the expected atom leak)
ci-stage2: ok (1 expected non-passing)
GATE PASS (full)        total 331.9 s
```

An earlier gate run the same afternoon (also `GATE PASS`) overlapped an edit of `lib/fib/tensor/reduce.fib` and is not counted.
`scripts/bench/quick.sh` was not run (the lock was held by this package's benchmarks; the hoist changes the emitted code only for loops
of the shape in section 5).

## 8. Package TP2: matmul, index decode, vector exp (after TP1)

Four library commits on top of TP1 (`lib/fib/tensor`, `scripts/bench/tensor`; no compiler change; stage 2 built by the v0.1.5 seed
from this tree). Sections 1 to 7 above are the TP1 record and are not rewritten; this section replaces their "after" column.

### 8.1 Table

Same method as section 1 (`run.py`, median of 5, one thread, `flock /tmp/fibsuite.lock`, checksums compared; every row `ok`):

```
python3 scripts/bench/tensor/run.py --fibc <F> --openblas ~/.cache/fibber-scratch/tp1/ob --tsv rows.tsv
```

| kernel | numpy | numpy + OpenBLAS | TP1 after | TP2 after | TP2 / numpy | TP2 / OpenBLAS |
|---|---:|---:|---:|---:|---:|---:|
| matmul f64 512 | 20.3 | 3.83 | 6.05 | 4.30 | 0.21 | 1.12 |
| matmul f64 1024 | 191.3 | 30.8 | 46.0 | 31.7 | 0.17 | 1.03 |
| matmul f64 2048 | 3479 | 247.9 | 453.9 | 285.8 | 0.08 | 1.15 |
| matmul f32 512 | 10.9 | 2.06 | 3.36 | 1.99 | 0.18 | 0.97 |
| matmul f32 1024 | 100.6 | 16.4 | 29.0 | 17.8 | 0.18 | 1.09 |
| matmul f32 2048 | 820.3 | 120.9 | 234.9 | 135.6 | 0.17 | 1.12 |
| softmax rows 4096x4096 f64 (t/exp) | 117.0 | - | 251.2 | 138.9 | 1.19 | - |
| layernorm 4096x1024 f64 | 33.3 | - | 32.3 | 38.0 | 1.14 | - |
| MLP f32 b256 784-512-10 | 9.56 | 1.72 | 5.52 | 3.56 | 0.37 | 2.07 |
| a*b+c f64 1e7 / f32 1e7 | 21.8 / 10.7 | - | 22.9 / 12.0 | 29.4 / 14.1 | 1.35 / 1.31 | - |
| a*b+c f64 1e8 | 217.8 | - | 273.6 | 2054 (see below) | 9.4 | - |
| sum / sum-fast / mean / max 1e8, axis rows, broadcast | | | | within 10% of TP1 except as noted | | |

Reductions and axis rows are unchanged from TP1 (ordered sum 108.9, sum-fast 37.0, mean 114.5, max 138.0, axis 19 to 34 ms,
broadcast 21.0 ms). The a*b+c rows are slower in this run than in TP1 and are **not** a change of this package: those kernels are the
untouched SIMD ones, and on the same machine state the **pre-TP2 binary** (built from the TP1 tree before commit 1 of this package)
gives 2866 ms for a*b+c f64 1e8 against 2749 ms for the TP2 binary (`tm.sh` median of 5, same minute); free memory was 18 GB of 61 GB
with 17 GB cache, a state in which the 2.4 GB working set is slow (page and huge-page compaction), which TP1 also saw once (section 6).
Re-measure on an idle machine before reading anything into those rows. The matmul rows were stable across runs (1024 f64: 29.6-33.9 ms).

**Goal (1) met:** every matmul is within 1.15x of OpenBLAS single-thread (target 1.3x). **Not met:** the MLP, 2.07x OpenBLAS: its two products
are 1.9 ms of the 3.6 ms (micro-timed: 256x784x512 in 1.86 ms, 256x512x10 in 0.07 ms); the rest is the bias add, the relu `where` and a
broadcast add, which are still the generic elementwise kernels (a `where` and a comparison per element, three passes). Fusing those is not done.

### 8.2 Commits, evidence

1. **Blocked fma GEMM** (`gemm-fma-f64.fib`, `gemm-fma-f32.fib`, linalg dispatch). GotoBLAS loop nest; A packed to 6-row panels and B to 8-column (f64) or
   16-column (f32) panels, both zero padded, read from the original strides, so views need no copy; the micro-kernel is 6x8 / 6x16 with 12 vector accumulators and
   `simd/fma`; edge tiles use a scratch tile and the same kernel; blocks 256/512 x 96 x 1024 (tuned: f64 kc 256/384/512 gave 32.1-36.7 ms at 1024, f32 kc 256/512 gave
   15.1/14.4). **Asm:** the built benchmark's kernel loop is `vfmadd231pd ymm0..ymm11` x 4 unrolled with ymm12-15 for B and the broadcast, no `vmulpd`/`vaddpd` pair
   in it (`objdump -d -M intel`; the other 42 `vmulpd`/`vaddpd` in the binary are other kernels). Before/after, 1024 f64 49 -> 32 ms, 2048 f64 483 -> 268, 1024 f32 28 -> 14.4, 2048 f32 216 -> 115
   (`tm.sh`, one run each, mean noise 5%; `scripts/bench/tensor/bench.fib matmul64 1024 6`).
   **Rounding changes**: an fma rounds once, so f64/f32 `mmul` can differ in the last bits from `mmul-scalar` (README updated). Each output is still one ordered chain
   (inner index increasing), pinned bit for bit by cases 7077/7078 against `fma-reference`.
   **Blocker found and handled, not worked around in the compiler:** on a target without hardware FMA (`FIB_TARGET_CPU=x86-64-v2`, which `package.sh` sets for releases) `simd/fma`
   lowers to a libm `fma` call: the 512 f64 product took **203 ms instead of 6 ms**. `mmul` therefore took the fma kernels only when `(native-lanes f64) >= 4` (SC1 replaced that proxy by the `(has-fma)` constant of the Target record, which is also true on aarch64; vmath's exp/log/tanh use `simd/muladd` and `simd/bitcast`, 1e7 exp at x86-64-v2 270 ms -> 41 ms) and the earlier multiply-then-add tiles otherwise (measured at x86-64-v2: 13.7 ms / 7.1 ms for 512 f64 / f32). A target with AVX2 but no FMA does not exist
   in practice; if it did, it would hit the slow path. The proper fix is in the compiler (lower `simd/fma` to mul+add, or expose a `has-fma` constant); I did not touch it.
2. **Dense fast path and row walks** for `map-as`, `zip-with`, `copy`, `to-array`, `to-vec`, `mean-axis` (`flat-at` per element is gone from them; `argmax`/`argmin`, `where`
   and the mask paths still use it). Micro (`scripts/bench/tensor/micro-walk.fib`, 2048x2048 f64, best of 5, before -> after ms): map 28.2 -> 9.6, zip-with 48.5 -> 10.5,
   copy 23.9 -> 6.9, to-array 23.1 -> 6.0, transposed map 104.9 -> 49.6, transposed zip 140.1 -> 41.6, transposed copy 103.5 -> 48.8, to-vec 1e6 15.6 -> 12.7 (the `conj`
   dominates). a*b+c and layernorm did not move (they are not on these paths); softmax 255 -> 189 ms. Case 7079 (indexed reads as oracle, 19 views); 8 planted faults all fail it.
3. **Vector exp, log, tanh** (`fib.tensor.vmath`, `t/exp t/log t/tanh`, f64 and f32). Measured maxima against libm over 1e6 to 2e6 points: **exp 1 ULP, log 2 ULP, tanh 2 ULP (f64),
   1 ULP (f32)**; documented in the module header and README; case 7080 asserts them (so a worse implementation fails) plus special values and tails; 8 planted faults fail it.
   Speed, 1e7 f64 exp: map over libm 50.9 ms, `t/exp` 34.0 ms (about 1.5x; memory traffic is about 15 ms of that); softmax 187 -> 145 ms (`softmaxlibm` and `softmax` kernels of
   `bench.fib`). The gain is smaller than a SIMD library's because 2^k is built by moving each of the four lanes through a scalar `bits->f64`: the language has no vector bitcast
   (a candidate for the compiler side: `simd/bitcast`), which also costs `log`. Flush-to-zero below -708.396 (libm returns subnormals) is a documented difference.
4. **`mean-fast`**, ordered-sum contract. `mean` keeps its ordered semantics: using `sum-fast` in it would change results, which the README forbids ("stop and report"), so I did not;
   `mean-fast` is the offered alternative. Cases 7081/7082.

### 8.3 Not done

- The MLP elementwise stages (bias, relu) are not fused; `where`/`greater` still decode per element. `argmax`, masks, `where`: `flat-at`.
- No vector bitcast, so exp/log have lane-by-lane scalar steps; an f32-native (8 lane) exp/log/tanh does not exist (f32 goes through f64).
- Last-axis reductions (2.7-4.5x NumPy), the ordered sum's 4-cycle chain (floor, by contract) and a*b+c / max over 1e8 are as in TP1.
- The fma-less x86 targets run the old kernels (1.5x slower than the new on this CPU: 1024 f64 49 ms against 32 ms) until the compiler gives `simd/fma` a non-libm lowering there.
- No other kernel shapes tuned (tall/skinny products, small n, are padded to whole panels: 256x512x10 spends most of its work on zeros).
- Timing rows for a*b+c are not trustworthy in this run (see 8.1).

## 9. Package TP3: fused layers (after TP2)

Four library commits on top of TP2 (`lib/fib/tensor`, `scripts/bench/tensor`, cases 7083 to 7085; no compiler change). Stage 2 is the TP2 one (built from this
tree by the v0.1.5 seed): a library edit needs no new stage 2. The goal was the MLP within 1.2x of NumPy+OpenBLAS (1.72 ms against 3.56 ms); the owner's
guidance was fused, copy-free library code first, and to stop at diminishing returns rather than tune the kernel toward OpenBLAS.

### 9.1 Profile, before any change

`scripts/bench/tensor/micro-mlp.fib` (best of 15 per stage, same process, ms):

| stage | ms |
|---|---:|
| matmul 256x784x512 | 1.55 to 1.67 |
| bias add 256x512 (`add`, row broadcast) | 0.03 to 0.05 |
| relu, `where (greater x 0) x 0` | **1.76 to 1.94** |
| matmul 256x512x10 | 0.056 |
| bias add 256x10 | 0.007 |
| the whole pipeline | 3.30 to 3.75 |

The TP2 doc blamed "bias, relu and broadcast passes". Measured, the bias add is nothing (the broadcast kernel is fast); the cost was the relu: `greater` through a
closure per element and `where` decoding three indices per element (`flat-at`). The second matmul (10 columns, padded to one 16-wide panel) is 0.06 ms, so the
tall/skinny tuning of step 4 has nothing to buy. The allocations are two outputs (512 KiB and 10 KiB) and the packing buffers (about 1.2 MiB); the GEMM output was already
`array-uninit-f32` (the matmul writes every element before it is read: the first inner block stores, later blocks load then store; documented in `multiply-with`).

### 9.2 What changed

1. **GEMM epilogue, `t/dense x w b act`, `t/mmul-bias`** (`gemm-fma-f32.fib` `multiply-with`, `linalg.fib`). Each 6x16 output tile of the last inner block is finished
   in place right after the micro-kernel stored it: `C = act(C + bias)`, bias padded to whole panels so edge tiles run the same two-vector code in their scratch tile.
   The activation is a keyword (`:identity :relu :tanh :sigmoid :silu :gelu`); `t/activate` and `t/relu` do one activation over a tensor in one pass. Other activations
   are added in `fib.tensor.vmath` (`apply4`) with one more case in `activation-code`. `f64`, a target without FMA and an empty inner dimension compose `mmul`, `add`, `activate`.
   Case 7083 (261 checks: the ordered fma reference plus bias plus a scalar activation, over every tail shape, two row, column and inner blocks, six activations, views and strided
   bias) and 14 planted faults (`scripts/mutant-tensor-fused.sh`: epilogue skipped on edge or full tiles, applied on every inner block or only the first, bias at the wrong tile or
   column block, one half of the tile or the last bias element dropped, bias stride ignored, a tile row unfinished, wrong activation code, relu removed, activation dropped by `dense`), all killed.
2. **`where` walks dense operands and scalars linearly** (`masks.fib`): the unfused relu 1.76 to 0.45 ms. Case 7084 (20 checks against indexed reads), 5 planted faults, all killed.
3. **`t/softmax`, `t/layernorm`** (last axis, f64 and f32, in `vmath.fib`): each row is processed while it is in L1 (see the README for the contract: reassociated sums, centred variance).
   Case 7085 (61 checks against a scalar libm oracle) and 10 planted faults, all killed.
4. **Benchmark rows**: `mlpfused`, `softmaxfused`, `layernormfused`, `attn` (QK^T / 8, softmax by composition, times V; one head, n = 1024, d = 64, f32) and `attnfused`
   (`t/softmax` in the middle), in `bench.fib`, `bench.py` (same formulas) and `run.py`.

### 9.3 Numbers

The machine was busy with other agents' work (load 6 to 10): a `run.py` table row can move by 2x between two runs (the same MLP binary: 1.6, 3.1, 4.5 ms in three runs), so the
fibber columns below are an **A/B**: the TP2 binary (`bench_old`, built from commit 405eb5a) and this tree's binary alternate, seven processes each, each process the median of its
5 timed runs (after one warm-up), under `flock /tmp/fibsuite.lock`; the table shows the median over the processes (min in brackets). NumPy columns are `run.py` (median of 5, one
thread, same lock), the least noisy of three runs; checksums of every row compared (relative 1e-4 for f32, 1e-9 for f64), all `ok`.

```
python3 ~/.cache/fibber-scratch/tp3/ab.py mlp:256:mlp:mlpfused softmax:4096:softmax:softmaxfused layernorm:4096:layernorm:layernormfused matmul32:512:matmul32:matmul32 ...
python3 scripts/bench/tensor/run.py --fibc F --bin B --openblas ~/.cache/fibber-scratch/tp1/ob --only mlp softmax layernorm attn matmul32
```

| kernel | numpy | numpy + OpenBLAS | TP2 composed | TP3 composed | TP3 fused | fused / numpy | fused / OpenBLAS |
|---|---:|---:|---:|---:|---:|---:|---:|
| MLP f32 b256 784-512-10 (`dense` twice) | 9.0 | 1.69 | 3.60 (3.48) | 2.20 (2.12) | **1.73 (1.59)** | 0.19 | **1.02 (0.94)** |
| softmax rows 4096x4096 f64 | 127.6 | - | 148.5 (142.1) | 146.0 (141.0) | **73.1 (70.8)** | 0.57 | - |
| layernorm 4096x1024 f64 | 35.2 | - | 43.8 (39.4) | 40.9 (39.2) | **10.7 (9.8)** | 0.30 | - |
| attention f32 n=1024 d=64 (softmax fused, mmuls as before) | 26.2 | 6.8 to 7.7 | 11.5 | 11.5 | 9.1 | 0.35 | 1.2 to 1.3 |
| matmul f32 512 / 1024 (unchanged path) | 11.0 / 98.4 | 2.0 / 15.1 | 2.24 / 17.05 | 2.18 / 17.30 | - | - | - |
| matmul f64 1024 (unchanged path) | - | - | 35.0 | 35.1 | - | - | - |

("TP3 composed" is the same `add` / `where` / `mmul` code as TP2, with the `where` fix; the attention rows are single `run.py` runs and noisy: the TP2 and TP3 composed
columns are the same code there, since `where` is not on that path.)

**The goal is met on this run:** the MLP is 1.02x NumPy+OpenBLAS at the median of the A/B and 0.94x at its best (target 1.2x; TP2 was 2.07x); fibber's MLP went from 3.6 to 1.7 ms. The
matmul path is unchanged (`multiply` calls `multiply-with` with code -1; 512 and 1024 f32 and 1024 f64 within noise of TP2). The remaining time is the two products themselves
(1.55 ms of 1.6), which is the kernel the owner chose not to tune further.

### 9.4 Not done, and why

- **Pack-once weights (`t/pack-weights`, step 5):** not done. Packing B for the 784x512 product is one read and one write of 1.6 MB (about 0.1 ms of 1.6): an estimate, not a
  measurement, and a handle type would put a second representation of a weight matrix into the public API for at most 6%. If inference with fixed weights becomes the main use, it is the next lever.
- **Prefetch / further kernel work:** not done; stopped at diminishing returns as asked (the MLP is at OpenBLAS parity).
- **Tall/skinny tuning:** nothing to tune: the 10-column product is 0.056 ms.
- **Last-axis `sum-axis`, `maximum-axis` (2.9 and 4.4x NumPy):** not changed. `sum-axis` is ordered by contract (the chain of dependent adds is the floor); a vectorised
  `maximum-axis` would differ from `(max a b)` folding in the sign of a zero (`simd/max` orders -0.0 below +0.0, the fold is order dependent), which is a behaviour change. The fused
  `softmax` and `layernorm` take their reductions (reassociated, documented) inside the row kernels instead. A `sum-axis-fast` would be new API; not added.
- **Activations on f32 use the f64 vector math** (two f64x4 halves per f32x8): `:gelu`, `:silu`, `:tanh`, `:sigmoid` are correct to a few ULP but not fast; relu and identity stay in f32. There
  is no f32-native `exp`. Attention's `t/exp`-based composed row and the fused row therefore stay 1.2 to 1.3x slower than NumPy+OpenBLAS (not tuned: the task said a benchmark row, not an optimised kernel).
- **`dense` for integer tensors** is not provided (the GEMM is f32/f64); `dense` on an empty inner dimension or without hardware FMA composes the unfused ops.
- Cases 7083 and the older fma-reference cases (7077, 7078) compare against `fma-reference` bit for bit, so they pass on the host (FMA) target only: at `FIB_TARGET_CPU=x86-64-v2`
  7078 fails already at the base commit and 7083 fails the same way (7080, 7084, 7085 pass there).
