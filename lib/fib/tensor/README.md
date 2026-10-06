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
`map-as`, `zip-with`, `copy`, `to-array`, `to-vec` and `mean-axis` read a dense
tensor straight from its buffer (logical element `i` is buffer element
`offset + i`) and any other layout row by row: outer coordinates are decoded
once per row and the inner loop advances by a constant stride. Reads stay
bounds-checked, so a forged descriptor still traps at the first bad read.

`sum`, `prod`, and `fold f initial tensor` are ordered scalar reductions in
logical row-major order. The order is the contract (a floating-point sum is
reproducible and equals the loop `acc = acc + x[i]`), so the sum is one chain of
dependent adds: about one add latency (4 cycles) per element, which is what makes
it several times slower than `sum-fast` on large inputs. `mean` is that ordered sum
over the count and stays so; `mean-fast` (`f64`) is `sum-fast` over the count, which
reassociates like `sum-fast` and can differ from `mean` in the last bits or more
under cancellation. Empty sums/products return typed zero/one.
`maximum`, `minimum`, and `mean` trap on empty input; `mean` currently accepts
`f64`. Integer arithmetic retains Fibber's checked-overflow behavior.

`sum-axis`, `prod-axis`, `maximum-axis`, `minimum-axis`, and `mean-axis` take
`axis keepdims tensor` and visit each slice in increasing axis index order.
With `keepdims = true`, the reduced axis has extent 1; otherwise it is removed.
Empty-axis sum/product produce zero/one; extrema and mean trap on an empty
reduction axis. `mean-axis` accepts `f64`. `fold-axis f initial axis keepdims
tensor` supports a different accumulator dtype with an `Element` instance.

**Lane kernels for the axis reductions** (`fib.tensor.axis-lanes`). `sum-axis`, `maximum-axis` and `minimum-axis` of a dense `f32` or `f64` tensor over an
axis that has elements after it (`axis` is not the last, or the last of extent... precisely: the product of the later extents is above 1; `sum-axis 0` of a matrix,
every axis but the last of a rank-3 tensor) view the tensor as `outer x extent x inner` and add row by row into a vector of `inner` accumulators, four vectors at
a time, so a 784x256 sum reads each element once at memory speed. **The order contract is unchanged**: every output element still adds its slice in increasing
axis index starting from `+0.0` (`acc = 0 + x0 + x1 + ...`), so the results are bit-for-bit those of the generic fold (case 7742 compares both over random data
spanning 1e-3 to 1e6, negative zeros and NaNs, every column count around the 8 and 32 column blocks, every axis of rank 3). `maximum-axis` and `minimum-axis`
replicate `(max acc x)` and `(min acc x)` exactly (a compare and blend, not `simd/max`): a NaN accumulator stays NaN, a NaN element makes the accumulator NaN,
a tie takes the later element, so `max(+0.0, -0.0)` is `-0.0` and `max(-0.0, +0.0)` is `+0.0`. The generic walk is kept for the other cases: a tensor that is not
dense (views, broadcasts), the last axis (`inner = 1`: one chain of dependent adds per output, which is the ordered contract), an empty axis, and the integer and
boolean types (checked overflow). `mean-axis` (`f64`) takes the lane kernel through `sum-axis`. No `sum-axis-fast` was added: the ordered kernel already runs at the speed
the reassociated one would have, since the vectors are across outputs, not along the axis.

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

**Lane kernels** (`fib.tensor.select`). The five comparisons and `where` walk operands that one linear pass covers (a dense layout, or a broadcast
scalar such as `(t/full [] 0.0)` on either side: step 0) with no closure per element: for `f32` and `f64` a comparison compares eight or four lanes at
once and stores the lanes into the boolean tensor, and `where` builds the lane mask from the mask's bytes and blends the two value vectors (`simd/blend`),
with a scalar tail; `i32`, `i64` and `bool` tensors run the same walk as scalar loops. Any other layout (a transposed view, a row broadcast) takes the earlier
element-by-element path with the same results. Floating comparisons are IEEE: a NaN compares false in all five, `-0.0` equals `+0.0`. `where` copies the
selected element bit for bit (a NaN or a signed zero is kept). The comparisons and `where` now also require the element type to have the internal
`Linear` instance, which the five standard element types (`i32 i64 f32 f64 bool`) have.

`relu-grad g x` is the relu backward in one pass: `g` where `x > 0`, else `+0.0` (`f32` and `f64`; operands broadcast, any layout). It is a **select**, as
PyTorch's `threshold_backward`: at `x = 0`, `x = -0.0` and `x = NaN` the gradient is 0 (PyTorch's convention: the relu gradient at 0 is 0), and a NaN or infinite
`g` under a zero mask gives 0 where `g * (x > 0)` would give NaN; under `x > 0` a NaN `g` stays NaN. `relu-mask x` is the `1` or `0` of `x > 0` in `x`'s dtype.
`relu y` of NaN or `-0.0` is `+0.0` as before (`:relu` means `(where (greater x 0) x 0)`), so `(relu-mask (relu x))` equals `(relu-mask x)` for every `x`,
`+inf` included (the earlier workaround `(relu (div y y))` gave 0 at `+inf`). Case 7741 compares everything against scalar oracles over every pair of 19
special values (both zeros, infinities, NaN, subnormals) and every tail length; `scripts/mutant-tensor-gaps.sh` plants 13 faults in these kernels.

## Linear algebra and SIMD

`dot` accepts equal-length rank-one tensors, `outer` accepts rank-one inputs,
and `mmul` accepts compatible rank-two inputs. They support strided and reversed
views. Empty inner dimensions produce typed zeros. `mmul-scalar` exposes the
checked, ordered reference implementation for comparisons.

Floating `mmul` on a target with a vector of at least four `f64` lanes (AVX2 and
FMA; `(native-lanes f64)`) runs a blocked kernel in the GotoBLAS shape
(`gemm-fma-f64`, `gemm-fma-f32`): B is packed per (column block, inner block)
into panels of NR columns, A per (row block, inner block)
into panels of 6 rows, both zero padded to whole panels and read from their
original strides, so any view is accepted without a copy. The kernel is one source over a witness vector,
so the tile follows the register file. At 256 bits (AVX2, sixteen ymm) it is a
6x8 (`f64`) or 6x16 (`f32`) tile of twelve vector accumulators updated with
`simd/fma`, a broadcast of A and two vector loads of B per inner step. Where the
target's native width is 512 bits (`x86-64-v4`, Sapphire Rapids, Zen 4 and later: types.targets, docs/design/avx512.md; thirty-two zmm) it is a 6x32 (`f64`)
or 6x64 (`f32`) tile of 24 accumulators, four vector loads of B and one broadcast: 29 of 32 registers, no spill. (A target without FMA, `(has-fma)` false, takes the multiply-then-add tiles of `gemm-f64`/`gemm-f32` instead; aarch64 would want its own tile.) An
incomplete tile at the right or bottom edge goes through a scratch tile and the
same kernel. Blocks are 256 (`f64`, narrow tile) or 128 (wide) and 512 (`f32` narrow) or 128 (wide) inner steps, 96 rows and
1024 columns. `multiply-at` (`gemm-fma-f64`, `gemm-fma-f32`) takes the width as a witness vector, `(splat f64x8 0.0)`, so a program can
run either shape on any target (the eight lanes are split into the registers an AVX2 CPU has) and compare them: cases 7960 and 7961 do, bit for bit. Pointers into the packed buffers are unchecked; shapes and the
reachable range of both inputs are validated once per call and every buffer is
sized from the loop bounds.
**Rounding:** each output is one chain of fused multiply-adds over the inner index
in increasing order, whatever its tile, block or vector width (a block continues the chain
from C), so `gemm-fma-f64/fma-reference` (a checked scalar loop) gives the same
bits. An fma rounds once, so the result can differ in the last bits from
`mmul-scalar` (multiply, then add), which is unchanged. A target without hardware
FMA (`(has-fma)` false: x86-64, x86-64-v2; every aarch64 CPU has it, with 128-bit vectors) would call libm per lane, 30 times
slower; there `mmul` keeps the earlier multiply-then-add tiles
(4×8 for widths at least 32 and 4×4 below, inner blocks of 128, output blocks of 64
or 32 for packed `f64` panels, scalar edges), which the safe tuning interface
(`gemm-f64/multiply-mode`, 8×4 tiles, sizes 32, 64, 128) also exposes on every target. No cross-target
bitwise reproducibility is promised. Integer multiplication uses the checked
scalar reference. Batched multiplication, decompositions, and BLAS integration
remain future work.

`sum-fast` and `dot-fast` use four independent SIMD accumulators: four native vectors (32 `f32` elements or 16 `f64` elements at 256 bits, 64 or 32 at 512) per unrolled iteration, followed by vector and
scalar tails. Small `f32` sums use a shorter four-lane loop. Contiguous inputs
use native vector reads; strided inputs use raw gathers after validating their
complete reachable range. Addition order differs from ordered reductions, so
results can differ in the last bits or more on cancellation-sensitive inputs.
No cross-target bitwise reproducibility is promised. `sum` and `dot` preserve
ordered scalar semantics.

## Vector width (AVX-512)

The lane kernels are written over the target's native width: `(native-lanes f64)` is 4 at 256 bits and 8 at 512, `f32xn` 8 and 16. Which width a
CPU gets is its row in `compiler/types/targets.fib` (`fibc targets` lists the targets; `x86-64-v4`, `sapphirerapids`, `graniterapids`, `znver4` and
`znver5` prefer 512, every other AVX2 CPU 256, Skylake-X and the client AVX-512 cores stay at 256 because they throttle or have one 512-bit fma unit),
and `FIB_VECTOR_BITS=128|256|512` overrides it for one run (a width the CPU lacks is split into the registers it has: correct, at that CPU's speed;
this is how the 512-bit shape of every kernel is tested on an AVX2 machine, `compiler/tests/lanes/lanes.sh`). A program built for the host on an
AVX-512 machine therefore gets 512-bit kernels; a release binary built for `x86-64-v3` keeps the 256-bit ones (docs/design/avx512.md).
What does and does not depend on the width: the **ordered** results do not (matmul, the fused dense layer, every elementwise operation, exp, log, tanh, the
unary functions, the axis reductions and the optimiser steps are bit-identical at every width; cases 7960 and 7961, `compiler/tests/lanes/lanes.sh`).
The **reassociated** sums do: `sum-fast`, `dot-fast`, the f64 `softmax` and `layernorm` row sums add a lane at a time and then the lanes, so their last bits
depend on the lane count (the README already promised no cross-target bitwise reproducibility for them). The f32 `softmax` and `layernorm`
(eight-lane chunks at every width) and the mask kernels (`where`, `relu-grad`: byte-mask gathers of eight or four lanes) keep their 256-bit shape.

## Vector exp, log and tanh

`exp`, `log` and `tanh` map an `f64` or `f32` tensor to a fresh tensor of the same
shape, four lanes at a time, without a libm call (`fib.tensor.vmath`; the constants
come from `scripts/bench/tensor/gen-vmath.py`). Measured against libm over dense
sweeps (case 7080, which asserts these bounds): `exp` at most 1 ULP in
[-708.396, 709.78] (below, the result is flushed to +0.0, where libm returns
subnormals; above 709.7827 it is +inf), `log` at most 2 ULP over the whole positive
range including subnormal inputs, `tanh` at most 2 ULP. `f32` is computed in `f64`
and rounded back, within 1 ULP of `expf`/`logf`/`tanhf`. Special values follow libm:
NaN in gives NaN, `log 0 = -inf`, `log x<0 = NaN`, `tanh +-inf = +-1`,
`tanh -0.0 = -0.0`. The polynomials use `simd/muladd` (fused with FMA, multiply-then-add without: the bounds hold on both, case 7080 passes at
`FIB_TARGET_CPU=x86-64-v2` too) and `simd/bitcast` for 2^k and the exponent split. Results can differ from the scalar libm in the last place. A
strided input is first made contiguous.

## Vector sqrt, rsqrt, abs and sign

`sqrt`, `rsqrt`, `abs` and `sign` map an `f64` or `f32` tensor to a fresh tensor of the same shape in one pass and one allocation
(`fib.tensor.unary`): eight (`f32`) or four (`f64`) lanes at a time over `simd/sqrt` and `simd/abs`, a masked tail, no closure and no libm call.
A dense input (including an offset slice) is read from its buffer; any other layout (a transposed view, a broadcast) is first made contiguous, as the
vector `exp` does, so it costs one more pass. `neg` and `square` already existed (`neg` is a scalar map, `square` is `mul x x`).

| Function | Result |
|---|---|
| `sqrt x` | IEEE `sqrt`, correctly rounded, so **bit-identical to libm** `sqrt` (an `f32` result equals the `f64` sqrt rounded to `f32`). A negative gives NaN, `sqrt(-0.0)` is `-0.0`, `sqrt(+inf)` is `+inf`, NaN gives NaN |
| `rsqrt x` | `1 / sqrt(x)`: a correctly rounded sqrt then a correctly rounded division, two roundings (within 1 ULP of the true value, not always correctly rounded). `rsqrt(+0.0)` is `+inf`, `rsqrt(-0.0)` is `-inf`, a negative gives NaN |
| `abs x` | clears the sign bit: `abs(-0.0)` is `+0.0`, NaN stays NaN, exact |
| `sign x` | `-1.0` or `+1.0`; a zero gives itself (`sign(-0.0)` is `-0.0`) and NaN gives NaN |

Case 7740 compares all four with scalar oracles bit for bit over 4096 pseudo-random bit patterns of every exponent (subnormals, infinities and NaNs
included), the special values, every tail length 0 to 19, an offset slice, a transposed view and a broadcast; `scripts/mutant-tensor-gaps.sh` plants 9 faults in them.

## Fused dense layers and activations

A neural-network layer is `activation(x * w + b)`. Written as `mmul`, `add`, `where`, it makes three
passes over the output and two temporaries; `dense` does it in the GEMM's store phase:

```clojure
(t/dense x w b :relu)          ; x: [m k], w: [k n], b: broadcasts to [n]  ->  [m n]
(t/mmul-bias x w b)            ; the same with no activation
(t/activate :gelu y)           ; an activation alone, one pass, one allocation (also (t/relu y))
;; a two-layer network:
(t/dense (t/dense x w1 b1 :relu) w2 b2 :identity)
```

Activations are the keywords `:identity :relu :tanh :sigmoid :silu :gelu` (`:gelu` is the tanh form
GPT-2 and BERT use; an unknown name traps). `:relu` means `(where (greater x 0) x 0)` exactly, so NaN and
`-0.0` give `+0.0`; the others are `fib.tensor.vmath`'s vector `exp` and `tanh` (so within a few ULP of libm,
the bounds of `exp` and `tanh` above, and an `f32` result is the `f64` result rounded).

For `f32` on a target with FMA, `dense` finishes each 6x16 output tile of the last inner block right after
the micro-kernel stored it (the tile is in L1): `C = act(C + bias)`, so the output is written once and no
temporary exists. The bias is copied once into a buffer padded to whole 16-column panels, so the epilogue
is two full vector adds per row also on edge tiles (which finish in their scratch tile before the copy out).
The product is the same fma chain as `mmul`, so `dense` equals `act(mmul + bias)` computed with the same
bits; for `f64`, a target without FMA, and an empty inner dimension, `dense` is composed from `mmul`, `add`
and `activate` (same value, extra passes). Case 7083 checks it against the ordered fma reference plus a scalar
activation over every tail shape, inner blocks and strided views, and `scripts/mutant-tensor-fused.sh`
plants 14 faults (dropped bias on edge tiles, activation on every inner block, wrong column offset, ...).
On the benchmark MLP (batch 256, 784-512-10, `scripts/bench/tensor/micro-mlp.fib`) the three-pass pipeline
took 3.3 ms and `dense` twice 1.5 ms; the bias add alone was 0.03 ms, the `where`-based relu 1.8 ms.

### Softmax and layernorm over the last axis

```clojure
(t/softmax scores)                      ; exp(x - rowmax) / rowsum, any rank >= 1, f64 or f32
(t/layernorm x gamma beta 1.0e-5)       ; (x - mean) / sqrt(var + eps) * gamma + beta; gamma, beta broadcast to [n]
```

Each row is handled while it is in L1: softmax reads it for the maximum, for `exp` (written once, summed on the
way) and for the division; layernorm for the sum, the squared deviations and the output. The composition of
`maximum-axis`, `sub`, `exp`, `sum-axis`, `div` (or `mean-axis`, `sub`, `mul`, ... for layernorm) makes five to ten
passes over the whole tensor and as many temporaries. The row sums are **reassociated** (four `f64` or eight
`f32` lanes, then a horizontal add, as `sum-fast`), so they can differ from the ordered `sum-axis` in the last
bits; the variance is the population variance from the centred second pass, not a one-pass `E[x^2] - E[x]^2`
(which cancels for a large mean: case 7085 has a mean of 1e6 over a spread of 27). `exp` is the vector `exp` above
(`f32` computed in `f64` and rounded). A NaN in a row gives a NaN softmax row. A strided input is first made
contiguous. Case 7085 compares both against a scalar oracle with libm over every tail length, ranks one to three,
views, empty tensors and extreme values; `scripts/mutant-tensor-fused.sh` plants 11 faults in them.
Softmax 4096x4096 `f64`: 74.7 ms against 145.1 ms composed (NumPy 126.8); layernorm 4096x1024: 9.7 ms against
42.9 ms composed (NumPy 35.2): `docs/shootout/tensor.md`, section 9.

## Fused optimiser steps

```clojure
(t/adam-step lr b1 b2 eps bc1 bc2 p g m v)   ; -> [p' m' v']      bc1 = 1 - b1^t, bc2 = 1 - b2^t for step t (the caller computes them)
(t/sgd-momentum-step lr mu p g buf)          ; -> [p' buf']
```

One pass over the parameter, its gradient and its state (eight `f32` or four `f64` lanes at a time, a scalar tail), no temporary tensor: the composition it replaces
(`axpby`, `mul`, `axpby`, `sqrt`, `scale`, `shift`, `div`, `axpby`) makes nine passes and eight temporaries per parameter. PyTorch's Adam without amsgrad or weight decay:
`m' = b1 m + (1 - b1) g`, `v' = b2 v + (1 - b2) g^2`, `denom = sqrt(v') / sqrt(bc2) + eps` (**eps is added after the square root and the bias correction**, as PyTorch does, not inside
the root), `p' = p - (lr / bc1) m' / denom`. SGD with momentum is PyTorch's (`buf' = mu buf + g`, `p' = p - lr buf'`; a zero `buf` at the first step gives `buf' = g`; no dampening, no Nesterov).
`f32` and `f64` tensors of one shape (a different shape traps; no broadcasting); scalars are `f64` and are rounded to the element type; a tensor that is not dense is made
contiguous first. **Rounding contract:** the kernel does the same multiplies, adds, one `sqrt` and one division, in the same order, as the unfused composition (no fused multiply-add;
`1 / sqrt(bc2)` is one reciprocal multiplied in, where PyTorch divides by `sqrt(bc2)`: at most one rounding apart), so the results are **bit-identical to the composition** and
within a few ULP of PyTorch 2.14 (case 7743: `f64` to 1e-12, `f32` to 3e-6 relative, on three steps of ten parameters). **Memory:** the three results are fresh buffers written once each (uninitialised arrays: the kernel stores every element), and the inputs are only read, so another holder of a tensor (a tape, a caller's `Vec`) never sees a change and no input buffer is copied first. The steps do **not** update in place: moving a unique tensor's buffer into a cell (`(cell (. p buffer))`, also through a helper that returns the field) left the buffer shared in every variant probed, so an in-place `adam-step!` would have copied anyway; this is reported, not solved (`docs/shootout/tensor.md` 10.4).
Case 7743 compares bit for bit over lengths 0 to 40, a matrix, a dense slice with an offset and a transposed view; `scripts/mutant-tensor-gaps.sh` plants 11 faults in them.

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
