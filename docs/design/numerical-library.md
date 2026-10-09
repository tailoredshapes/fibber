# A Lispy numerical library for Fibber

Current status: the dense core and SIMD kernels are described by the [tensor API](../../lib/fib/tensor/README.md). The October 4 implementation brief below also contains future proposals.

Original record (dated statements and unmarked code fences below are historical sketches):

Date: 2026-10-04. Status: proposal and implementation brief. The checked dense
core and native SIMD kernels now exist in [fib.tensor](../../lib/fib/tensor/README.md);
that README
defines the implemented API and distinguishes it from the proposals below.
This extends [the tensor design](simd-and-tensors.md) and follows
[the latest view decisions](decisions-2026-10-04.md). It does not replace
approved language rules. Examples below describe proposed interfaces and
are not all executable with the initial core (in particular, `arange` is
currently integer-only).

## Product direction

Build `fib.tensor` as a dense numerical library with ordinary Lisp functions,
typed storage, strided views, broadcasting, reductions, and matrix operations.
The first goal is a coherent array foundation that can support competitive
kernels. Matching all of NumPy's breadth is a longer program involving
indexing, statistics, linear algebra, random sampling, files, and interop.

Use Lisp syntax and function composition rather than importing Python's
indexing syntax. Keep numerical data distinct from persistent general-purpose
collections: a `Vec` is useful for API arguments, records, and irregular data;
a tensor stores homogeneous scalars contiguously in its backing array.

## Data structures

| Structure | Responsibility |
|---|---|
| `Tensor T` | Own a reference to a typed buffer and validated layout metadata |
| Layout metadata | Offset, dynamic rank, extents, and element strides |
| Slice specification | All, integer index, or bounded range with a step |
| Broadcast plan | Compatible result shape and normalized operand strides |
| Kernel plan | Coalesced axes and contiguous/strided dispatch information |
| Scoped writable window | Temporary exclusive access to a validated region; depends on the view feature |

Retain the existing proposal's single metadata array: offset, rank, extents,
then strides. Expose shape as a small persistent vector, but do not allocate
shape vectors inside element loops. Avoid a persistent vector of boxed
numbers as the actual numerical buffer.

Start with `f32`, `f64`, `i32`, and `i64`, with dtype fixed by `T` rather than
a runtime string. Use explicit dtype conversion. Boolean masks are important
for a NumPy competitor, but their storage and SIMD conversion should be a
separate decided package rather than an implicit object array fallback.

Rank is dynamic, initially 0–8 as proposed by the existing tensor design.
Rank zero has shape `[]` and one element; shape `[0]` is empty. Reject ragged
input at construction. General heterogeneous data remains a `Vec` or record.

### Layout invariants

- Extents are nonnegative and rank matches metadata lengths.
- Shape products, strides, offsets, and addressing use checked arithmetic.
- Nonempty layouts prove every reachable element lies inside the buffer.
- Negative strides represent reversal; zero strides represent broadcasting.
- Empty tensors never dereference their buffer and have an explicit offset policy.
- Fresh allocation checks element count, payload byte count, and native header
  size before allocation; broadcasting a tiny buffer cannot bypass this check.
- Layout validity and write eligibility are different properties: a valid
  broadcast view can have overlapping logical elements and cannot be an
  unrestricted writable window.
- Singleton-axis strides do not determine contiguity; zero-sized and rank-zero
  cases have deliberate rules.

The proposed private representation needs a concrete enforcement check.
Current module privacy must actually prevent clients from constructing or
accessing invalid layouts before kernels rely on unchecked accesses. A comment
that fields are private is not an encapsulation mechanism.

## Surface API

Use a qualified namespace in examples to keep tensor operations clear beside
the implicit sequence library:

```clojure
(ns main (:require [fib.tensor :as t]))

(let [a (t/reshape [2 3] (t/arange 0.0 6.0 1.0))
      b (t/tensor [10.0 20.0 30.0])
      c (t/add a b)]
  (t/sum c))

(->> samples
     (t/sub mean)
     (t/div scale)
     (t/square)
     (t/sum))
```

Function argument order is a design decision: the second example uses
collection-last transforms, following the project's Clojure direction.
The prior tensor design uses tensor-first forms for some operations. Reconcile
the two before implementation and record the selected signatures. Noncommutative
operators must have unmistakable operand order; introduce pipeline helpers if
necessary rather than silently reversing subtraction or division.

Initial public operations:

| Area | Functions |
|---|---|
| Construction | `tensor`, `full`, `zeros`, `ones`, `arange`, `linspace` |
| Inspection | `shape`, `rank`, `size`, `contiguous?`, `item` |
| Layout | `reshape`, `transpose`, `slice`, `flip`, `broadcast-to`, `copy`, `contiguous` |
| Elementwise | `add`, `sub`, `mul`, `div`, `neg`, `square`, comparisons, selection |
| Reduction | `sum`, `prod`, `minimum`, `maximum`, `mean`, `argmin`, `argmax`, ordered fold |
| Linear algebra | `dot`, `outer`, rank-two matrix multiplication |
| Conversion | Flat vector, typed array, explicit dtype conversion |

Avoid user variadics and keyword-option machinery until the compiler supports
them reliably. Use fixed signatures or a small options record. Axis reductions
need an explicit `keepdims` choice. Multiple axes, duplicate axes, negative axis
normalization, and output order must be specified.

## Views, indexing, and updates

Borrow NumPy's useful shape/stride model, not its mutable alias semantics.
NumPy's ordinary views share writable data, and reshape can copy when a view
is impossible ([official documentation](https://numpy.org/doc/stable/user/basics.copies.html)).
Fibber's ordinary views retain a buffer reference and preserve value semantics.
An update to an owner with a live view copies, leaving the view unchanged.

Specify reshape's cost explicitly: a view-only operation that rejects
incompatible layouts, and an operation allowed to materialize a copy. Do not
promise that every reshape is metadata-only. A transpose or flip is normally
a metadata operation; gathering arbitrary indices materializes output.

Use tagged slice specifications instead of loose maps with magic keys.
An integer index drops an axis; a range retains it; `:all` retains the axis.
Start with positive-step slices, then add negative steps with pinned endpoint
rules. Later packages add gather, boolean selection, and scatter with explicit
duplicate-index behavior.

Ordinary operations return values. Scoped exclusive windows are the explicit
bulk-write API, following the approved ownership design. Broadcast views with
zero strides are read-only unless materialized; overlapping strided windows
must not receive independent mutable access. Shared buffers become unique once
before a writable kernel, not once per element.

## Broadcasting and kernel execution

Align dimensions from the trailing axis. Two extents are compatible if equal
or one is 1. Choose the other extent when one is 1; this makes `[0]` combined
with `[1]` produce `[0]`, not `[1]`. Missing axes behave as singleton axes.
Represent expanded axes with zero strides, without replicating input data.
See [NumPy's broadcasting rules](https://github.com/numpy/numpy/blob/main/doc/source/user/basics.broadcasting.rst).

Compute a broadcast/kernel plan once per operation. Coalesce adjacent axes
when every operand's layout permits it. Dispatch among:

1. Equal-shape contiguous operands and scalar broadcasts: flat SIMD loop.
2. Unit-stride inner dimension: vector inner loop and strided outer traversal.
3. Arbitrarily strided inputs: constant-stride vector gathers for native numeric
   kernels, checked scalar callbacks, and packing where useful.
4. Empty output: no kernel execution or input element read.

No shape checks, metadata allocations, or rank-sized work inside the element
loop. SIMD is an implementation choice; users do not store lane vectors to
use ordinary tensors. Native floating kernels now use eight lanes for `f32`
and four for `f64`, coalesce compatible axes, traverse rows with constant
strides, and validate public descriptors before raw access. Arbitrary callbacks
still use checked scalar traversal. Explicit `axpby` fuses a common affine
expression without changing the eager semantics of other calls.

Keep the public API eager. Establish eager semantics before adding compiler
fusion for nested calls and eligible single-use bindings. Fusion must preserve
evaluation order, traps, dtype rules, and reuse behavior. Arbitrary user
functions cannot automatically become lane-polymorphic kernels. Support known
numeric operations first and retain a correct scalar tensor-map fallback.

## Reductions and matrix multiplication

Separate ordered fold from fast reduction. Document pairwise/SIMD ordering,
NaNs, empty input behavior, accumulator dtype, integer overflow, and target
dependence. Sum and product have identities; minimum/maximum and arg reductions
need an explicit failure/optional result policy for empty input. Mean of an
empty tensor needs a defined policy too.

Provide all-element reduction and single-axis reduction first. Keep dimensions
when requested so centering and normalization compose with broadcasting.

Start matrix multiplication with rank two and a scalar reference kernel.
Then add tiled packing and a register-blocked SIMD microkernel. Rank-one and
batched multiplication are later semantic extensions with explicit shape rules.
Do not infer batched behavior from a generic `dot` name.

Explicit FMA may be used where its numerical contract is documented. Compare
floating matrix results with justified tolerances, not identical-output claims
after reordering. For performance comparisons, record layout, algorithm,
thread count, CPU target, dtype, and whether allocation/packing is included.
Keep optional optimized BLAS interop available for later linear algebra.

## Modules and delivery

Suggested internal responsibilities, subject to privacy enforcement:

```text
fib.tensor                 public facade
fib.tensor.layout          shape, strides, bounds, axis normalization
fib.tensor.construct       constructors and dtype conversion
fib.tensor.views           reshape, transpose, slicing, broadcasting
fib.tensor.plan            normalized multi-operand traversal
fib.tensor.elementwise     scalar baseline and SIMD dispatch
fib.tensor.reduce          all-element and axis reductions
fib.tensor.linalg          dot and matrix multiplication
```

First milestone: validated storage/layout, constructors, scalar reading,
read-only views, broadcasting, eager elementwise operations, reductions, and
rank-two multiplication. This is a usable dense-tensor foundation, not a claim
of NumPy parity or speed parity.

Native vector buffer access, contiguous SIMD arithmetic, bounded allocation
cases, boolean masks/selection, and blocked floating matrix multiplication are
also implemented. Tiled input copies, reusable B-panel packing, and measured
register-tile dispatch are implemented; see [performance notes](numerical-performance.md). Third: scoped writable windows and
disjoint parallel tiles, integrated with the existing language design. Later:
gather/scatter, random generators with explicit state, statistics,
decompositions, serialization, and foreign array interoperability.

## Validation

Write scalar model cases before optimized kernels. Exercise rank zero, empty
axes, singleton axes, overflow, invalid coordinates, negative strides,
broadcast conflicts, shared snapshots, view lifetime, and incompatible reshape.
Add metamorphic cases: transpose twice, reshape preserving logical order,
slice matching scalar indexing, broadcasting matching explicit repetition,
and matrix multiplication matching a small triple loop.

Separate semantic tests from cost tests. Views may allocate descriptors but
must not copy data. SIMD tails must not touch inactive out-of-range lanes.
Benchmark tiny, cache-resident, and memory-sized workloads, contiguous and
strided inputs, and fused versus materialized expressions. Verify numerical
answers and record comparison methodology before describing the library as
competitive with NumPy.
