# fib.tensor

A dense numerical library with typed storage, strided views, broadcasting,
eager arithmetic, masks, ordered/axis reductions, and blocked matrix kernels.
Native `f32x8`/`f64x4` loads and stores accelerate arithmetic and explicit
fast reductions; row traversal supports broadcasts and arbitrary strides. This is a usable foundation, not NumPy feature or
performance parity.

Use the native compiler rebuilt from this checkout; these kernels require
the new floating memory primitives. A release seed can build that compiler:

```sh
fibc build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F
./F run examples/tensor.fib
```

Import the facade explicitly; it does not change implicit collection functions:

```clojure
(ns main (:require [fib.tensor :as t]))

(defun main () -> i64
  (let [a (t/reshape [2 3] (t/arange 0 6 1))
        b (t/tensor-i64 [10 20 30])
        c (t/add a b)]
    (do (println (t/to-vec c))  ; [10 21 32 13 24 35]
        (println (t/sum c))    ; 135
        0)))
```

For a runnable example with row normalization and matrix multiplication, see
[examples/tensor.fib](../../../examples/tensor.fib).

## Storage and construction

`(Tensor T)` stores an `(Array T)`, an `(Array i64)` containing offset,
rank, shape, and strides, and a scalar zero witness. The witness supplies
dtype and empty-reduction identities without reading an empty buffer.
Standard `Element` instances are `i32`, `i64`, `f32`, `f64`, and `bool`.
Boolean tensors store masks; numeric operations require numeric elements.

Rank is dynamic, from 0 through 8. Shape `[]` is a scalar with one element;
shape `[0]` is empty. Dimensions must be nonnegative. Shape and address
arithmetic is checked; overflow traps. Fresh allocations also check payload
bytes plus the native Array header before allocating. Strides and offsets are
in elements.

| Operation | Contract |
|---|---|
| `full dims value` | Fill a tensor; the value fixes its element type |
| `zeros dims`, `ones dims` | Default `f64` constructors |
| `zeros-f32`, `zeros-i32`, `zeros-i64` | Typed zero constructors |
| `tensor values` | Flat `(Vec f64)` to a rank-one tensor |
| `tensor-i64 values` | Flat `(Vec i64)` to a rank-one tensor |
| `from-vec dims values seed` | Explicit shape and typed witness, including empty inputs |
| `from-array dims buffer seed` | Share an exactly sized typed array; validate shape |
| `from-parts buffer offset dims strides seed` | Checked strided descriptor, including negative/zero strides |
| `arange start stop step` | Half-open `i64` range; nonzero step, either direction |
| `linspace start stop n` | `f64`, inclusive endpoints for `n > 1`; `n = 1` returns start |
| `shape`, `rank`, `size`, `strides`, `dtype`, `contiguous?` | Inspect a tensor |
| `at tensor indices`, `flat-at tensor index` | Checked element access; negative indices are rejected |
| `item tensor` | Read a tensor containing exactly one element |
| `to-vec tensor` | Logical row-major values |
| `to-array tensor` | Materialize exactly the logical values as a typed array |

The seed is a type witness, not an additional element. Generic arrays require
a fill value even when empty; this explicit API avoids guessing a dtype.
Nested vector construction and implicit dtype promotion are not provided.

## Views and value updates

```clojure
(t/transpose matrix)
(t/permute [2 0 1] cube)
(t/flip -1 matrix)
(t/slice 1 1 5 2 matrix)
(t/broadcast-to [10 3] (t/tensor [1.0 2.0 3.0]))
```

`transpose` reverses axis order. `permute` specifies a complete permutation.
Negative axes count from the end. `slice axis start stop step tensor` uses
strict half-open bounds, retains the axis, and currently requires a positive
step. Use `flip` for reversal. Views share the data buffer but allocate new
small descriptors; they copy no elements.

`reshape dims tensor` is view-only and requires a row-major contiguous layout
with the same element count. To reshape a transpose in logical order, first
call `copy` or `contiguous`. Singleton strides do not prevent contiguity.
Empty layouts are treated as contiguous.

`assoc tensor indices value` returns an updated value. Other holders and views
keep their old elements. A noncontiguous or broadcast input is materialized
before updating so one logical coordinate does not change its repeated peers.
A contiguous slice can still copy its parent backing array on update; minimizing
that cost is future work. There is no user-facing shared mutable view API yet.

## Arithmetic and reductions

Binary operations are **tensor-first, left operand then right operand**:
`add`, `sub`, `mul`, and float-only `div`. Both operands must have the same
element type. Wrap a scalar in `full [] value` for binary broadcasting, or use
`scale factor tensor` and `shift amount tensor`. Integer division into ratios
is deliberately unavailable as a tensor operation.

Broadcasting aligns trailing axes. Equal extents or an extent of 1 are
compatible; `[0 3]` and `[1 3]` produce `[0 3]`. Broadcasts use zero strides.
Invalid shapes trap before callbacks run. Outputs are eagerly materialized.
Floating operations dispatch once to specialized kernels: eight lanes for `f32`,
four for `f64`, with bounded tails. Compatible axes coalesce into rows; outer
coordinates are decoded once per row, and inner loops use constant strides.
Zero-stride operands reuse one scalar; arbitrary strides use vector gathers.
Integer operations retain checked overflow.

`axpby alpha x beta y` computes `alpha*x + beta*y` in one traversal and one
output allocation, with ordinary broadcasting. Coefficients and operands share
one numeric dtype. It does not introduce FMA or fuse arbitrary callbacks:

```clojure
(t/axpby 0.5 x -0.25 y)
```

`map f tensor` preserves dtype. `map-as f seed tensor` and
`zip-with f seed left right` permit a different output dtype. Callbacks run
once per logical output element, in row-major order, and never run on an
empty output. They may have effects. These kernels are not fused and arbitrary
callbacks are not automatically SIMD-vectorized.

`sum`, `prod`, and `fold f initial tensor` are ordered scalar reductions in
logical row-major order. Empty sums/products return typed zero/one.
`maximum`, `minimum`, and `mean` trap on empty input; `mean` currently accepts
`f64`. Integer arithmetic retains Fibber's checked-overflow behavior.

`sum-axis`, `prod-axis`, `maximum-axis`, `minimum-axis`, and `mean-axis` take
`axis keepdims tensor` and visit each slice in increasing axis index order.
With `keepdims = true`, the reduced axis has extent 1; otherwise it is removed.
Empty-axis sum/product produce zero/one; extrema and mean trap on an empty
reduction axis. `mean-axis` accepts `f64`. `fold-axis f initial axis keepdims
tensor` supports a different accumulator dtype with an `Element` instance.

`argmax` and `argmin` return the first flat logical index on a tie and trap
on empty input. They use ordinary comparisons: an initial NaN remains the
winner, and a later NaN does not replace it. NumPy NaN behavior is not promised.

```clojure
(t/sum-axis -1 true matrix)  ; [rows 1], suitable for broadcasting
(t/sub matrix (t/sum-axis -1 true matrix))
```

## Masks and selection

`equal`, `less`, `less-equal`, `greater`, and `greater-equal` broadcast two
same-dtype tensors and return `(Tensor bool)`. Floating comparisons use the
language's IEEE comparison semantics. Masks support views and value updates.

`where mask left right` broadcasts all three tensors and materializes the
selected values. Both value operands must have the same dtype. `any?` and
`all?` short circuit; empty masks return false and true, respectively.

```clojure
(t/where (t/greater matrix (t/full [] 0.0)) matrix (t/full [] 0.0))
```

## Linear algebra and SIMD

`dot` accepts equal-length rank-one tensors, `outer` accepts rank-one inputs,
and `mmul` accepts compatible rank-two inputs. They support strided and reversed
views. Empty inner dimensions produce typed zeros. `mmul-scalar` exposes the
checked, ordered reference implementation for comparisons.

Floating `mmul` uses 4×8 register tiles for widths at least 32 and 4×4
for smaller widths, with inner blocks of 128 and output blocks of 64
(or 32 for packed `f64` panels).
Strided input materialization copies 32×32 tiles. Matrices with at least 256 rows, 128 inner elements, and 128 columns pack B
panels directly from original strides into one reusable scratch buffer per call;
scalar edges cover incomplete tiles. The safe internal tuning interface also
exposes 8×4 tiles and cache/inner sizes 32, 64, and 128. Integer multiplication
uses the checked scalar reference. Each output traverses the inner dimension
in order; no FMA is introduced explicitly. Batched multiplication,
decompositions, and BLAS integration remain future work.

`sum-fast` and `dot-fast` use four independent SIMD accumulators: 32 `f32`
elements or 16 `f64` elements per unrolled iteration, followed by vector and
scalar tails. Small `f32` sums use a shorter four-lane loop. Contiguous inputs
use native vector reads; strided inputs use raw gathers after validating their
complete reachable range. Addition order differs from ordered reductions, so
results can differ in the last bits or more on cancellation-sensitive inputs.
No cross-target bitwise reproducibility is promised. `sum` and `dot` preserve
ordered scalar semantics.

## Safety and validation

The `Tensor` descriptor is public so callers can name the generic type.
Fibber's existing privacy does not provide opaque public structs. Constructors
validate descriptors, but callers can bypass them using the raw constructor.
Every raw kernel therefore validates metadata, shape, and the complete reachable
buffer range before deriving pointers. Forged and malformed descriptors have
explicit trap cases.

Raw kernels retain their input owners for the entire call and write only fresh
output arrays, so shared snapshots cannot be mutated. Internal memory helpers
use the native Array data offset (24 bytes); that representation dependency is
documented by the shared layout accessor in `fib.tensor.memory`. Unsafe float loads/stores use byte alignment
and access only complete scalar or vector blocks. Fresh floating outputs use
internal uninitialized arrays and are published only after every logical element
has been written; empty-inner matrix products explicitly initialize zeros. Checked scalar access
remains available for arbitrary layouts. Pointer extraction and raw block helpers are private to each kernel module;
the shared memory module exposes no raw access capability.

Cases 7000–7019 cover storage, snapshots, numeric/boolean types, layouts,
SIMD tails, ordered and fast reductions, fused broadcasting, all three matrix
tiles, forced panel packing, empty inner dimensions, and unsafe memory round
trips. Cases 7020–7049, 7060–7068, and 7070–7074 pin trap boundaries;
7050–7056 pin dtype, unsafe, and helper privacy rejection. Together these are
71 numerical cases. Allocation checks retain the 55-allocation bound for a
4096-element SIMD add plus reduction and zero per-element scalar-read allocations.

```sh
fibc cases cases/stdlib --only 700 701 702 703 704 705 706 707 -j 4
```

## Reproducible performance comparison

```sh
python3 scripts/bench/tensor.py --compiler /path/to/current/fibc
```

The driver requires local NumPy for independent answer checks and installs no
dependencies. It compiles varied signed inputs with host targeting and LLVM
`-O2`, runs one warmup and five measured samples, and reports medians. Input
construction, checksums, and result destruction are outside timing; output
allocation and packing are included. BLAS uses one thread, and the process is
pinned to its first permitted CPU (`--cpu` overrides this). The suite holds
`/tmp/fibsuite.lock` to exclude concurrent gate and benchmark runs.

The default suite covers 60 workloads across `f32` and `f64`: tiny vectors,
cache-sized and larger-than-cache vectors, reductions, fused arithmetic,
row broadcasting, dense/transposed matrices, and packing. `--suite tune`
compares 192 combinations of register tiles, cache blocks, inner blocks, and
panel packing. JSON records all samples, tolerances, checksums, CPU affinity,
compiler path, target, NumPy version, and BLAS configuration.

See [performance results and implementation notes](../../../docs/design/numerical-performance.md)
for before/after measurements and limitations, and
[the numerical-library proposal](../../../docs/design/numerical-library.md)
for the larger direction. General fusion, writable shared views, parallel
kernels, gather/scatter, and implicit dtype promotion remain proposals.
