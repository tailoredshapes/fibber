# Numerical kernel performance

Current status: historical October 4 implementation and measurement record. The current API is in [fib.tensor](../../lib/fib/tensor/README.md); rerun the recorded commands for current performance.

Original record (dated statements and unmarked code fences below are historical sketches):

Measured and implemented 2026-10-04. The public API remains eager and typed;
ordered reductions keep their existing evaluation order.

## Changes

- Fast reductions use four independent accumulators, with contiguous/strided
  dispatch outside the reduction loop. `f32` processes 32 elements per unrolled
  iteration; `f64` processes 16. Short `f32` sums use four lanes.
- Traversal plans remove singleton axes and coalesce adjacent axes only when
  both operands permit it. Outer coordinates are decoded once per row.
  Specialized operation loops use eight `f32` lanes or four `f64` lanes,
  constant-stride gathers, cached broadcasts, and bounded tails.
- `axpby alpha x beta y` computes `alpha*x + beta*y` in one traversal and one
  result buffer. Integer overflow remains checked. This is explicit fusion;
  arbitrary callbacks and nested eager calls are not automatically fused.
- Native unsafe `array-uninit-f32/f64` avoid initializing fresh output payloads
  twice. Negative/oversized lengths trap before allocation. Numeric kernels
  validate public descriptors before extracting pointers, retain their owners,
  and publish outputs only after complete writes. Zero-inner matrix products
  explicitly fill zeros. Uninitialized float arrays contain no object references.
- Matrix copies traverse 32×32 tiles. Reusable B-panel packing reads original
  strides directly into one fixed-pitch scratch array per multiply and reuses
  each panel across row tiles. Tail kernels read original B strides; unused
  panel padding is never read. The A input is materialized only when needed.
- Matrix kernels offer 4×4, 4×8, and 8×4 register tiles. The default selects
  4×4 when output width is below 32, otherwise 4×8. Cache blocks are 64 and
  inner blocks 128. Panels activate when m≥256, k≥128, n≥128; packed `f64`
  uses cache blocks of 32. No explicit FMA or fast-math is introduced.

These changes preserve immutable inputs and shared views. Pointer helpers are
private; safe kernel entry points validate forged descriptors. `sum-fast` and
`dot-fast` have a different addition order and no cross-target bitwise promise.

## Measurements

Intel Core i7-14700KF, CPU affinity `[0]`, host targeting, LLVM `-O2`, NumPy
2.3.5, BLAS restricted to one thread. Both native versions were compiled with
the same compiler, using a preserved copy of the library for the before run.
Inputs vary in sign and value and use exactly representable dyadic fractions.
One warmup precedes five timed samples; values below are medians in milliseconds.
Input construction, checksums, and final result destruction are excluded;
result allocation and matrix packing are included.

The before affine operation composes two scales and an add. The after operation
uses `axpby`. The before packing operation uses general `contiguous`; the after
operation uses tiled matrix packing. These are comparisons of complete operations,
not isolated instruction throughput. NumPy's affine comparison also composes
multiplications and addition with temporaries.

| dtype | Workload | Before ms | After ms | Speedup | NumPy ms |
|---|---|---:|---:|---:|---:|
| f32 | Add, 1M | 0.3920 | 0.2698 | 1.45× | 0.2725 |
| f32 | Fast sum, 1M | 0.4093 | 0.0778 | 5.26× | 0.1433 |
| f32 | Fast dot, 1M | 0.4223 | 0.1598 | 2.64× | 0.4451 |
| f32 | Affine, 1M | 2.8383 | 0.2924 | 9.71× | 0.5338 |
| f32 | Matmul 128³ | 0.2695 | 0.0576 | 4.68× | 0.1375 |
| f32 | Transposed matmul 128³ | 0.3514 | 0.0657 | 5.35× | 0.1381 |
| f32 | Transpose packing 128³ | 0.0871 | 0.0088 | 9.87× | 0.0061 |
| f32 | Matmul 512³ | 17.7963 | 3.1640 | 5.62× | 10.7725 |
| f32 | Transposed matmul 512³ | 20.0986 | 3.6524 | 5.50× | 10.5702 |
| f32 | Transpose packing 512³ | 1.4656 | 0.1562 | 9.39× | 0.1917 |
| f32 | Row broadcast, 129×257 | 0.3343 | 0.0129 | 25.91× | 0.0103 |
| f64 | Add, 1M | 0.7804 | 0.7458 | 1.05× | 0.5873 |
| f64 | Fast sum, 1M | 0.4724 | 0.1528 | 3.09× | 0.1879 |
| f64 | Fast dot, 1M | 0.5468 | 0.3088 | 1.77× | 0.5258 |
| f64 | Affine, 1M | 7.6211 | 0.6628 | 11.50× | 1.6606 |
| f64 | Matmul 128³ | 0.2832 | 0.0948 | 2.99× | 0.3443 |
| f64 | Transposed matmul 128³ | 0.3685 | 0.1030 | 3.58× | 0.3447 |
| f64 | Transpose packing 128³ | 0.0950 | 0.0081 | 11.70× | 0.0082 |
| f64 | Matmul 512³ | 18.9266 | 6.0176 | 3.15× | 19.7170 |
| f64 | Transposed matmul 512³ | 21.8426 | 7.1198 | 3.07× | 20.7965 |
| f64 | Transpose packing 512³ | 1.5659 | 0.2139 | 7.32× | 0.3511 |
| f64 | Row broadcast, 129×257 | 0.3600 | 0.0152 | 23.73× | 0.0141 |

The local NumPy configuration reports generic system BLAS 3.12.1, not an
optimized OpenBLAS or MKL build. The matrix ratios do not establish performance
against those implementations. These are warm repeated runs on one host;
cache state, CPU frequency, layout, and BLAS configuration affect results.
NumPy still wins some operations, including this run's large `f64` addition.

Three tiny workloads regressed by more than 10% against the preserved library:
31-element `f32` sum, 0.148→0.171 µs;
31-element `f64` sum, 0.130→0.149 µs;
31-element `f64` dot, 0.277→0.316 µs. These sub-microsecond timings are noisy and remain
limitations; large-array gains do not imply every size is faster.

The 192-configuration tuning suite checks both float widths, four matrix shapes,
three register tiles, two cache blocks, two inner blocks, and panel/direct modes.
At 512³, `f32` 4×8/cache64/inner128 measured 3.8758 ms direct versus
3.0349 ms packed; `f64` 4×8/cache32/inner128 measured 5.9055 ms packed.
At 31×17×29, direct 4×4 won. This supports conservative shape-based dispatch,
not an exhaustive hardware-independent crossover model. The raw assembly
inspection of the earlier direct `f64` kernel found packed arithmetic and no
vector stack stores; panel-mode assembly is not claimed to have that property.

## Reproduction and answer checks

```sh
python3 scripts/bench/tensor.py --compiler /path/to/rebuilt/F
python3 scripts/bench/tensor.py --compiler /path/to/rebuilt/F --suite tune
# Compare a preserved older library using equivalent composed operations:
python3 scripts/bench/tensor.py --compiler /path/to/rebuilt/F \
  --library /path/to/older/lib --reference --output /tmp/tensor-before
```

NumPy is required, never installed by the driver. Default output is
`/tmp/fibber-tensor-bench/results.json`. All six native checksums per job must
match independently computed NumPy results: f32 relative tolerance 5e-5,
absolute tolerance 0.02; f64 relative 1e-10, absolute 1e-8. Checksums alone
cannot establish elementwise correctness; the executable cases compare every
output against scalar models, including forced panel configurations and automatic
panels crossing both cache and inner boundaries. Exact dyadic fixtures make
those checks stronger than aggregate tolerances.

The expanded suite covers 60 workloads, including 31-element vectors,
4K/1M/8M vectors, awkward matrix dimensions, transposed inputs, broadcasts,
reductions, packing, and fused expressions. The 8M vectors exceed this CPU's
last-level cache even at f32. JSON records affinity, CPU target, compiler version and binary/library hashes,
NumPy/BLAS configuration, every sample, answer, and tolerance. Gate and
benchmark runs serialize through `/tmp/fibsuite.lock`.

Recorded artifacts for this session:

- Before: `/tmp/fibber-tensor-baseline-pinned/results.json`
- After: `/tmp/fibber-tensor-performance-verified/results.json`
- Tuning: `/tmp/fibber-tensor-panels-tune/results.json`

## Remaining performance work

Tune dispatch on other CPUs and optimized BLAS builds before broad performance
claims. General expression fusion, vectorized arbitrary callbacks, axis-reduction
kernels, multithreading, batched multiplication, FMA policies, and BLAS interop
remain future work. Small-call overhead and scalar matrix edges remain visible;
a wider SIMD type does not guarantee a speedup on every target.


## Validation

The final `scripts/gate.sh --full -j 4` run reported `GATE PASS (full)`:
F and F3 emit identical compiler lIR; ownership 305/305 pass; modules 27/27
pass; stdlib 1,117 pass, the existing expected atom leak failure 1707, and
21 explicitly open cases. All 71 numerical cases pass, with no pending cases.
The zero-row matrix regression case uses the maximum i64 column extent and
completes without a shape-sized kernel loop. Floating fused arithmetic and all
matrix configurations also compile and pass with `FIB_TARGET_CPU=x86-64-v2`;
the runnable numerical example includes `axpby`.

The final expanded benchmark checks all 60 answers and stores six native and
six NumPy timing samples per workload, including warmup, plus compiler and
tensor-source hashes. The 192-configuration tuning run checked every answer.

The repository quick benchmark preserved all ten checksums but exited 3 for
timing flags against the checked-in baseline: `num-f64` +13.0%, `num-nbody`
+10.1%, `vec-sort` +11.6%, `set-conj` +17.3%, and `lazy-fused` +14.7%.
All five emit identical lIR with the previous 0.1.5 compiler and the final
compiler. This rules out changed emitted lIR for those workloads, but does
not resolve the timing differences. The baseline was not rewritten.
