# First numbers on aarch64 (package A64-1)

The shootout programs, the SIMD kernels and the tensor `mmul`, built with the stage 2 `fibc` of this tree and run on two machines. This is a first look, not a benchmark report:
three runs each, the machines differ, and nothing here was tuned for NEON.

## Machines

| | x86 | aarch64 |
|---|---|---|
| CPU | Intel Core i7-14700KF (8 P cores up to 5.6 GHz, 12 E cores; 28 threads) | Apple M1 Ultra (20 cores: 16 performance, 4 efficiency; up to 3.2 GHz) |
| OS | Linux 7.0 | macOS 27.0.1 |
| `fibc` | stage 2 built from this tree by the v0.1.5 seed, host CPU | the same tree built on the Mac by the cross-built seed, host CPU (`apple-m1`) |
| C twin | gcc `-O3 -march=native` | Apple clang `-O3` |

Clock rate, core design and compiler differ, so a ratio below is "this machine over that machine" and says nothing about the instruction sets. Every program runs on one thread
(the cores do not matter, the frequency and the width of one core do). The x86 box was shared with other jobs (load average 4 to 5 during the run, the benchmark lock held), so its
medians are noisier than the Mac's, which was idle: `dot-mem` on x86 has median 1.276 s and minimum 0.797 s. The Mac's medians and minima agree to under 1% everywhere but `saxpy-mem`.
Java and Clojure were not run (no JDK on the Mac). Every output matched the md5 of the shootout's `sizes.txt` on both machines (the last column).

## Results

Wall seconds of the whole process, median of 3 (`scripts/shootout/a64-bench.sh -n 3`; rows of both machines in the same order):

| program | variant | x86 median s | M1 Ultra median s | M1 / x86 | md5 ok (x86, M1) |
|---|---|---:|---:|---:|---|
| n-body | fib-scalar | 6.661 | 8.875 | 1.33 | ok, ok |
| n-body | fib-simd | 1.994 | 2.636 | 1.32 | ok, ok |
| n-body | c-cc-O3 | 1.763 | 1.897 | 1.08 | ok, ok |
| spectral-norm | fib-scalar | 1.369 | 1.545 | 1.13 | ok, ok |
| spectral-norm | fib-simd | 0.557 | 0.647 | 1.16 | ok, ok |
| spectral-norm | c-cc-O3 | 0.604 | 1.556 | 2.58 | ok, ok |
| mandelbrot | fib-scalar | 11.554 | 14.852 | 1.29 | ok, ok |
| mandelbrot | fib-simd | 2.347 | 3.660 | 1.56 | ok, ok |
| mandelbrot | c-cc-O3 | 11.247 | 14.782 | 1.31 | ok, ok |
| fannkuch-redux | fib-scalar | 24.782 | 28.372 | 1.14 | ok, ok |
| fannkuch-redux | c-cc-O3 | 23.217 | 27.076 | 1.17 | ok, ok |
| binary-trees | fib-scalar | 7.424 | 7.426 | 1.00 | ok, ok |
| binary-trees | c-cc-O3 | 9.767 | 8.989 | 0.92 | ok, ok |
| dot-mem | fib-scalar | 1.900 | 0.409 | 0.22 | ok, ok |
| dot-mem | fib-simd | 1.276 | 0.326 | 0.26 | ok, ok |
| dot-mem | c-cc-O3 | 0.629 | 0.385 | 0.61 | ok, ok |
| dot-cache | fib-scalar | 0.887 | 1.914 | 2.16 | ok, ok |
| dot-cache | fib-simd | 0.310 | 0.411 | 1.33 | ok, ok |
| dot-cache | c-cc-O3 | 0.844 | 1.919 | 2.27 | ok, ok |
| saxpy-mem | fib-scalar | 0.533 | 0.432 | 0.81 | ok, ok |
| saxpy-mem | fib-simd | 0.428 | 0.367 | 0.86 | ok, ok |
| saxpy-mem | c-cc-O3 | 0.664 | 0.421 | 0.63 | ok, ok |
| saxpy-cache | fib-scalar | 0.322 | 0.535 | 1.66 | ok, ok |
| saxpy-cache | fib-simd | 0.338 | 0.529 | 1.57 | ok, ok |
| saxpy-cache | c-cc-O3 | 0.165 | 0.471 | 2.85 | ok, ok |
| matmul | fib-scalar | 6.882 | 6.963 | 1.01 | ok, ok |
| matmul | fib-simd | 6.393 | 10.036 | 1.57 | ok, ok |
| matmul | c-cc-O3 | 2.252 | 1.419 | 0.63 | ok, ok |
| tensor-mmul-f64 | fib-scalar | 0.179 | 0.225 | 1.26 | ok, ok |
| tensor-mmul-f32 | fib-scalar | 0.118 | 0.145 | 1.23 | ok, ok |

`tensor-mmul-*` is `scripts/shootout/tensor-mmul/tensor-mmul.fib`: three 1024 x 1024 products through `fib.tensor`'s `mmul`, 6.4 GFLOP in all, so about 36 GFLOPS (f64) on the x86 box and
28 GFLOPS on the M1 (one NEON core's f64 peak is 16 flops per cycle, 51 GFLOPS at 3.2 GHz; the x86 core's is 16 flops per cycle at up to 5.6 GHz). The two machines print the same
checksum for both element types (the md5 of the output, f64 `28aedb71...`, f32 `2acff896...`): the fma kernel's ordered chain is bit-identical across the two architectures, as the cases 7077 and 7078 say it should be.

## What the numbers say

* Scalar code is within 1.0 to 1.33 of the x86 box on the compute-bound programs (n-body, spectral-norm, mandelbrot, fannkuch-redux, binary-trees, matmul scalar), which at 3.2 GHz against
  about 5 GHz means the M1 core does more per cycle; the compiler's generated code and the C twin move together (n-body 1.33 and 1.08, mandelbrot 1.29 and 1.31).
* The SIMD builds (`f64x4` and friends, two NEON registers each) keep most of the scalar-to-SIMD gain: n-body 8.9 s to 2.6 s, mandelbrot 14.9 s to 3.7 s, spectral-norm 1.5 s to 0.65 s,
  dot-cache 1.9 s to 0.41 s. On x86 the same kernels go 6.7 to 2.0, 11.6 to 2.3, 1.4 to 0.56, 0.89 to 0.31.
* Memory-bound kernels (dot-mem, saxpy-mem) are faster on the M1 (0.2 to 0.9 of the x86 time): the Ultra's memory system. The in-cache ones (dot-cache, saxpy-cache) are slower (1.3 to 2.9).
  `saxpy-cache` and `dot-cache` on the M1 are the C twin's weakness too (2.3 and 2.9 against the x86 C), so part of this is the clock, part the loop overhead of a short vector.
* `matmul-simd` is the one SIMD program slower than its scalar twin on the M1 (10.0 s against 6.96 s; x86 6.4 s against 6.9 s). The C twin, which the compiler vectorizes, takes 1.4 s on the M1 and 2.3 s on x86, so
  both machines leave most of the gain unclaimed: this is a kernel question (below), not a NEON one.
* binary-trees (allocation and reference counting) is the same on both machines (7.4 s) and faster than C's malloc/free twin on both.

## The GEMM tile and NEON (observed, not changed)

`lib/fib/tensor/gemm-fma-f64.fib` and `gemm-fma-f32.fib` run a 6 x 8 f64 (6 x 16 f32) micro-tile sized for AVX2: twelve `f64x4` accumulators, "the sixteen ymm registers less two B vectors and one broadcast".
On NEON the register file is 32 registers of 128 bits, and an `f64x4` is two of them:

* The twelve accumulators are 24 of the 32 registers; the two B vectors are 4 more and each broadcast A value is at least 1: 24 + 4 + 1 = 29 live, leaving 3 for the broadcasts the 6 rows need
  in flight. The tile fits, but with no room: a compiler that keeps more than three splats alive at once spills. The assembly was not read; the measured 28 GFLOPS (55% of a core's 51) against the x86 box's 36 (about 40% of
  90 at 5.6 GHz) says it does not spill badly.
* The by-element form `fmla v.2d, v.2d, v.d[i]` (one load of two A values serves two rows, no broadcast register) frees the broadcast registers for accumulators, so a wider tile than 6 x 8 can
  hold its accumulators: that is the shape NEON kernels usually take. This is reasoning from the register count, not a measurement; the tile that is best on an Apple core is for A64-9 to find by trying.
* The f32 tile, 6 x 16 (twelve `f32x8` = 24 registers), has the same budget.
* The block sizes (mc 96, kc 256, nc 1024) were chosen for the x86 caches; the M1's cache sizes were not looked up or measured here.

Nothing of this was tried. A NEON tile is A64-9's work (`docs/design/aarch64.md` 8).

## Reproduce

```
FIBC=/path/to/fibc scripts/shootout/a64-bench.sh -n 3 --out rows.tsv        # either machine; bash 3.2 and macOS tools are enough
```
