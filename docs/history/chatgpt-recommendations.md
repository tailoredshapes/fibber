# Recommendations for Fibber's next implementation milestones

Date: 2026-10-04. Status: **proposal**, based on a review of this checkout
and the preceding discussion. This document does not approve new language
rules, replace existing owner decisions, or claim that proposed features
are implemented. Example APIs below are illustrative unless identified
as existing.

The objective is to make ordinary expressive programs efficient, with
predictable memory costs, and to extend that property to numerical kernels.
The recommended work is: generalized value representations, vector access
to scalar buffers, scoped exclusive views, and compiler-backed diagnostics.
Floating-point contracts and validation are part of those changes.

## 1. Starting point and evidence

New implementation work belongs in `compiler/` and `lib/`. The Rust tools
are frozen as seed and legacy comparison tools. The compiler's native
backend is in `compiler/native/`, over `compiler/llvm/`; changing only the
Rust emitter would not change the active language.

Several earlier recommendations are already addressed:

- Scalar options have an inline tag/payload representation in stage 2,
  including in fields, arrays, captures, and task frames. Ordinary
  object options remain nullable pointers. Nested options and other
  payloads can still require boxes.
- Last-use moves, field moves, array update primitives, and shell reuse
  allow persistent collection updates to reuse unique storage.
- Eligible single-consumer sequences bound through `let` can fuse;
  reused sequences retain memoization.
- SIMD values have typing and native lowering, with arithmetic, masks,
  lane operations, reductions, and target-dependent widths.
- `run -O 0` uses FastISel. Compiler-server and incremental-development
  work have a design, rather than being assumed complete.

During this review, the available stage-2 binary reported `fibc 0.1.5`.
Fifteen selected cases passed: ownership 267, 268, 269, 279; standard-library
4010, 4011, 4015, 4050, 4051, 4200–4203, 6200, and 6290. These are focused
checks, not a fresh full gate or bootstrap verification. Case 1707 reproduced
the documented failure: `clean=false`, two leaks, zero reported errors.

The benchmark evidence in this document is recorded evidence, not new
measurement. Consult [the quick baseline](../../scripts/bench/baseline.tsv),
[the development-loop baseline](../../scripts/bench/dev-loop.tsv), and
[the SIMD shootout](../shootout/simd.md) for tree identities, workloads,
checksums, timing methods, and comparison limitations.

Relevant designs and rules:

- [Scalar options](../design/unboxed-option.md).
- [Collection updates](../design/in-place-update.md) and
  [standard-library rules](../../spec/stdlib.md).
- [SIMD and tensors](../design/simd-and-tensors.md).
- [Exclusive views](../design/exclusive-views.md), with draft syntax and types.
- [Development loop](../design/dev-loop.md).
- [October 4 decisions](../design/decisions-2026-10-04.md).

The dated decisions supersede the independent interpreter-oracle requirement.
The interpreter is a development tool. Existing case verdicts, runtime
audits, compiled reference programs, and the bootstrap fixed point remain
useful evidence; a fixed point alone does not prove semantic correctness.

## 2. Generalized value representations

### 2.1 Problem

Removing scalar-option boxes eliminates allocation at common lookup
boundaries. Other small values can still allocate: pairs produced while
walking collections, scalar records, and control/result enums. Allocating
such a value for every element can dominate an otherwise simple loop.

The next step should be a consistent representation mechanism, rather than
another collection of special cases for individual type names.

### 2.2 Representation descriptor

Introduce an explicit descriptor for each concrete, specialized type:

| Property | Purpose |
|---|---|
| Value and memory types | Describe register/SSA values separately from storage where necessary |
| Size and alignment | Lay out fields, array elements, captures, and frames consistently |
| Representation kind | Distinguish scalars, inline aggregates, counted objects, nullable objects, and dynamic values |
| Identity policy | Determine whether weak references or other operations require stable object identity |
| Child ownership operations | Generate retain, release, trace, and share behavior for reference-bearing fields |
| Constructor and pattern layout | Locate tags and payloads without duplicating representation decisions |
| Specialization identity | Ensure generic bodies and their representatives agree on representation |

Begin at [emit/layout.fib](../../compiler/emit/layout.fib) and
[emit/value.fib](../../compiler/emit/value.fib). Inventory their consumers in
object construction, pattern lowering, generated walkers, monomorphization,
compile-time constants, and native calls before changing their contracts.

Refactor existing scalar, option, and dynamic layouts through the descriptor
first. Keep emitted behavior unchanged in that preparatory step. Do not
assume a value type's LLVM alignment equals its allocated storage alignment:
the existing SIMD memory representation already handles this distinction.

### 2.3 Initial eligibility rule

Start with small, nonrecursive records whose fields are scalars. A concrete
`Pair i64 i64` is a useful first candidate because collection iteration can
exercise it heavily. Extend next to small scalar-payload enums such as
appropriate `Step` and `Result` instantiations.

Choose a measured size limit; a tentative 16-byte starting limit is a
proposal, not an ABI guarantee. Larger aggregates can increase register
pressure, copying, spills, and array footprint. Eligibility should account
for padding and the largest enum payload, not merely source field count.

Keep recursive types and types requiring observable identity boxed
initially. Establish a language policy before automatically unboxing
ordinary structs: existing weak-reference behavior must not disappear as
an accidental consequence of optimization. Possible policies include an
explicit value classification or conservative exclusion of identity-bearing
types. This decision needs approval before changing the specification.

### 2.4 One representation across storage positions

For a concrete type, use one representation in arguments, returns, locals,
joins, loops, fields, arrays, closure captures, task captures/results, and
suspended frames. Avoid opportunistic boxing at each boundary in the first
implementation; missed conversions create layout and ownership errors.

For enums, preserve a distinct tag for every semantically distinct variant.
Never collapse absence into a payload bit pattern unless that pattern is
provably unavailable to the payload. In particular, nested option states
must remain distinguishable. Initialize inactive storage deterministically
where it can be copied or inspected; do not let padding or inactive payloads
define language equality.

Verify calling conventions through native, protocol, and compile-time JIT
paths. LLVM may return small aggregates in registers, but that is target
and ABI dependent; it is not a portable language guarantee. Keep foreign
ABI changes separate and explicit.

### 2.5 Aggregates containing references

Treat this as a later package. An inline aggregate has no count header of
its own, but its children can still be counted objects. Copying retains
the active children, consuming moves ownership, and dropping releases
the active children. Enum walkers must follow only the selected variant.
Raw pointers remain uncounted.

Update the ownership model deliberately. The scalar-option implementation
can safely use a conservative object plan with no-op count operations;
that does not automatically justify the same treatment for an aggregate
containing references. Sharing across tasks must recursively mark the
relevant children, and `Send` must remain structural.

### 2.6 Validation and completion criteria

- Exercise every storage position listed above, including generic and
  protocol boundaries, constants, and values live across `await`.
- Test nested patterns, inactive enum payloads, equality, printing, and
  replacement of aggregate array/cell elements.
- For reference-bearing aggregates, keep children alive through copies
  and verify their release after the final owner disappears.
- Add allocation bounds for a scalar-pair pipeline and a scalar-result
  loop; the bounds must fail when boxing is deliberately restored.
- Measure allocation count, bytes, peak resident memory, and throughput.
  Include materialized arrays so reduced allocation is not confused with
  worse element density or excessive copying.

Complete the initial package when selected scalar aggregates allocate no
wrapper objects across supported storage positions, correctness cases pass,
and targeted benchmarks show no material regression from copying or spills.

## 3. Vector memory operations over scalar arrays

### 3.1 Problem and API scope

The SIMD measurements identify bounds/index overhead and the absence of
vector access to `(Array f64)` as major gaps. Requiring `(Array f64x4)`
changes storage layout and makes overlapping or differently sized vector
access awkward.

Proposed operations, with final names and type-inference rules to decide:

```clojure
(load-vector xs offset)            ; result's Simd type fixes the lane count
(store-vector! &ys offset lanes)   ; existing uniqueness policy initially
```

Start with contiguous, unaligned access. Define offsets and bounds in
elements, not bytes. A checked load produces a lane value; a checked store
writes a complete lane value. Shared snapshots retain value semantics.
Do not require stronger alignment than the array actually guarantees.

### 3.2 Bounds and lowering

For array length `length`, lane count `lanes`, and offset `offset`, require:

1. `offset >= 0`.
2. `lanes <= length`.
3. `offset <= length - lanes`.

This avoids overflow in `offset + lanes`. Only after validation compute the
address, with a proof that element-to-byte addressing is valid for the
allocation. Lower to one vector load/store rather than assembling lanes
from separately checked scalar accesses.

Before exclusive views exist, mutable stores follow the current cell and
copy-on-write rules. This makes the feature useful early, but a per-store
uniqueness check is still overhead; do not present it as the final hot-loop
interface.

Extend the Fibber front end, emitter, lIR checker, and native lowering as
needed. Do not add the feature to the frozen Rust seed. Prefer existing
lIR operations where their alignment and memory semantics suffice.

### 3.3 Masked tails

Add separate masked load/store operations after full-block operations are
stable. Specify inactive load lanes through an explicit fill value. Inactive
store lanes leave memory unchanged. An inactive lane must not access an
out-of-range address or fault.

Check active addresses, including sparse masks. Define negative offsets
and all-inactive masks explicitly rather than inheriting LLVM behavior.
Provide a scalar-tail fallback on targets lacking suitable instructions.
Do not treat a masked instruction as permission to create invalid pointers.

### 3.4 Validation and completion criteria

- Test lengths 0, 1, `lanes - 1`, `lanes`, `lanes + 1`, and several blocks.
- Test first/last valid offsets, negative offsets, large offsets, and
  offsets not aligned to a vector-width boundary.
- Compare active results to scalar reference loops. Check shared owners,
  preserved snapshots, tail stores, and inactive lanes.
- Inspect generated LLVM and machine code: establish that a block has one
  range check and a vector access rather than per-lane checks.
- Measure cache-resident and memory-sized workloads separately, including
  initialization separately from the timed kernel.

Completion means scalar-backed arrays support useful full-block SIMD access
without repacking, with defined failure behavior and demonstrated lowering.
Do not claim a speedup for memory-bandwidth-bound workloads without measuring.

## 4. Scoped exclusive views

### 4.1 Relationship to the existing design

Build on [exclusive-views.md](../design/exclusive-views.md) and its October 4
amendments. The earlier design text defers some parallel support; the later
decisions include disjoint parallel windows and read-only lending in the
first design. Sequential implementation can be a development checkpoint,
but should not silently redefine the approved scope.

The useful invariant is: a window has temporary access to a validated
buffer range, the owner remains alive, and conflicting access is unavailable
until that window's scope ends. Copy a shared backing buffer once when
establishing writable uniqueness, rather than once per element.

Illustrative surface, based on the design:

```clojure
(with-view [v (slice! &buffer lo hi)]
  (fill! &v 0.0)
  (vset! &v 3 1.0))
```

Keep persistent `Vec` updates separate from contiguous buffer windows.
Start the implementation with arrays and explicitly defined backing-buffer
ownership; tensor windows follow their layout and stride rules.

### 4.2 Checker responsibilities

The checker must track the identity of the lent place, not only its spelling.
Before lending, establish the existing private-cell conditions. Account for
aliases, closures created before the lend, helper calls, and enclosing
`&` forwarding. A syntactic prohibition on the owner's name is sufficient
only when privacy and effect rules prove that indirect access is impossible.

During a writable lend:

- Freeze conflicting owner reads, writes, replacement, and resizing.
- Keep the view scoped: no return, ordinary heap storage, or escape through
  generic wrappers, dynamic values, or closure captures.
- Reject suspension across `await` unless a separate lifetime rule supports it.
- Reject overlapping mutable subwindows; construct disjoint windows through
  checked splitting or tiling operations.
- End the lend on every scope-exit path and restore access to the owner.
- Do not admit tail calls that discard the frame needed by a live lend.

The latest design forbids closure captures of view cells in v1. Parallel
work uses the approved tile combinator, rather than making arbitrary scoped
values generally `Send`.

### 4.3 Runtime protocol and optimization promises

At lend entry, establish backing-buffer uniqueness, validate the range,
and build the window without an extra strong buffer reference. The owner
keeps the buffer alive for the entire extent. A shared source is copied
before creating writable windows; retained snapshots remain unchanged.

Uniqueness must cover the backing allocation, not just a tensor wrapper.
Subwindows and strided views can share the same allocation even when their
wrappers are distinct. Prevent reallocation while raw window addresses live.

Hoist checks only where the window and loop prove all accesses valid.
Apply LLVM alias metadata conservatively: disjoint byte ranges and restricted
access can justify optimization, but a general `noalias` annotation requires
the exact LLVM contract to hold. Read-only views may alias one another.

### 4.4 Parallel disjoint windows

`split!` or a tile combinator must prove or check that writable ranges do
not overlap. The parent scope must wait for all workers before restoring
owner access or releasing the backing storage. Worker traps and cancellation
must also honor that lifetime. No worker may resize the underlying buffer.

Define whether splitting occurs before or after the single copy needed for
a shared source; all children must refer to the same resulting unique
allocation. Test nonuniform final tiles and empty partitions. False sharing
is a performance concern, not evidence that the disjointness rule is wrong.

### 4.5 Validation and completion criteria

Write reject cases for aliased-owner access, prior closure access, escaped
views in generic containers, overlapping writers, captured view cells,
suspension, and premature restoration of owner access. Add accepted cases
for multiple read-only readers, nested disjoint windows, and joins of tiled
parallel work.

Compare a blocked matrix multiplication and an in-place row operation to
scalar reference implementations. Test edge tiles, shared inputs, and
snapshots. Allocation bounds should prove one copy at lend entry when
required and no copy per write for a unique source.

Complete the feature only when the stated sequential and approved parallel
rules are enforced, adversarial cases fail for the intended reasons, and
real kernels use the interface without reaching for unchecked raw pointers.

## 5. Diagnostics and development loop

### 5.1 Compiler-backed checking

Implement the proposed `fibc check --json` before a long-lived server.
Separate structured diagnostic production from text rendering; errors should
not be reconstructed by parsing human-readable messages.

Recommended diagnostic fields:

| Field | Contract |
|---|---|
| Schema version | Allows clients to recognize compatible output |
| Stable code | Identifies the rule independently of wording |
| Severity and message | Human-facing explanation |
| Primary span | File, start, end, and a documented coordinate convention |
| Related spans | Binding, lend, declaration, or capture that caused the restriction |
| Notes | Useful context and suggested correction |

Choose a JSON framing format and define exit statuses. Keep stdout machine
readable; use stderr for timing/debug output. Handle malformed input and
unreadable files as structured errors where possible. Preserve macro call
positions and relevant input-form positions; distinguish them in related
locations when needed. Define conversion to the editor's position encoding.

For an invalid view access, point to both the access and the lend that froze
the owner. For numeric-width or SIMD broadcast errors, identify the inferred
types and suggest a suffix, conversion, or `splat` only when applicable.

### 5.2 Editor integration

Use diagnostics from the active compiler for current-language checking.
The legacy `fibref lsp` should not silently diagnose new syntax under an
older language model. Completion and hover should eventually use the same
resolved names and schemes, with source locations for definitions.

Tests should compare `check` and `run` front-end verdicts, verify diagnostic
codes and spans, and show that a compiler rejection appears at the correct
editor location. Include Unicode and macro-generated input. Avoid goldens
that unnecessarily freeze every explanatory word.

### 5.3 Cache before server before incrementality

First separate immutable checked-library state from per-program state.
Cache it within one process, keyed by actual inputs: compiler version/build,
source contents, ordered module roots, relevant flags, target configuration,
and embedded-prelude identity. Cache failed results carefully or not at all
initially. Confirm that repeated checking cannot mutate cached state.

Next implement the proposed opt-in server. Preserve execution isolation,
explicit session ownership, and fallback when the server is unavailable.
Follow the existing child-process design and specify behavior when a request
is interrupted, a program traps, or the client disconnects.

Only then add per-definition incrementality, starting with type checking.
Invalidate dependent components for changed declarations, protocol instances,
macros, ownership summaries, and relevant compiler configuration. Keep a
from-scratch path as a comparison for emitted output and verdicts.

Measure cold startup, warm checks, a one-function edit, a library edit,
memory growth across repeated requests, and recovery after server failure.
Do not hide a slower cold path behind attractive warm measurements.

## 6. Floating-point contracts

SIMD speedups can come from reassociation, fused operations, and different
data layouts. These must not silently change ordinary arithmetic semantics.

Provide an explicit fused multiply-add operation with its single-rounding
contract. Preserve ordinary multiply followed by add unless contraction is
already explicitly allowed by the language. Define reduction order through
separate operations or an explicit policy: ordered, tree, and any relaxed
mode must be distinguishable. Names remain a design decision.

Document NaNs, infinities, signed zero, subnormals, and target differences.
Decide what reproducibility means: a fixed expression order is not by itself
a promise of identical behavior across all targets and floating-point modes.
Do not add a broad fast-math switch as an undocumented shortcut.

Use exact or bitwise reference cases where the contract requires them;
otherwise specify numerical tolerances and justify their bounds. Include
cancellation-sensitive and exceptional inputs. Benchmark FMA separately
from changes in layout, reduction order, and algorithm.

## 7. Delivery plan

| Package | Scope | Depends on | Reviewable completion evidence |
|---|---|---|---|
| A0 | Inventory and representation descriptor refactor | Existing option implementation | Preserved behavior and fixed-point/case validation |
| A1 | Inline scalar pair and small scalar records | A0, identity-policy decision | Storage-position cases and allocation bounds |
| A2 | Selected scalar-payload enums | A1 | Tag/payload cases, control-loop allocation checks |
| A3 | Reference-bearing inline aggregates | A2, ownership design | Child lifetime, sharing, and audit cases |
| M0 | Full-block vector access to scalar arrays | Current SIMD lowering | Boundary tests and verified vector code generation |
| M1 | Masked tails and target fallbacks | M0, mask semantics decision | No inactive-lane memory access; edge-length cases |
| V0 | Scoped contiguous views and owner privacy | View rules reconciled with latest decisions | Alias/escape rejections and shared-source preservation |
| V1 | Checked-once kernel access | V0, M0 | No per-write copying; loop/code inspection |
| V2 | Disjoint parallel tiles and read-only lending | V0, task lifetime rules | Join/trap lifetime tests and scalar model comparison |
| D0 | Structured diagnostics and check command | Diagnostic schema decision | Correct codes/spans and front-end verdict agreement |
| D1 | Editor consumption | D0 | Current-language diagnostics in the editor |
| D2 | Immutable library-state reuse | State separation | Warm timing improvement and unchanged results |
| D3 | Server and execution isolation | D2 | Fallback, crash/trap recovery, bounded state growth |
| D4 | Per-definition incrementality | D3 | From-scratch equivalence and dependency invalidation |
| F0 | Explicit FMA and reduction contracts | Numeric semantics decision | Exceptional-value cases and documented reproducibility |

Prioritize M0 and D0 as early user-visible improvements. Develop A0/A1
independently. V0 must follow a review of privacy and escape rules; V2 is
part of the approved design scope, not an optional omission. A3 and D4
are larger follow-ups and should not delay the first useful milestones.

This table describes dependencies, not an instruction to spawn agents or
create branches. No new release, implementation, or specification approval
is implied by creating this document.

## 8. Validation and measurement discipline

For each implementation package:

1. Write meaningful accepted, rejected, and trap cases before finalizing the
   behavior. Add allocation bounds where allocation is the claimed benefit.
2. Run targeted cases against the active stage-2 compiler.
3. Inspect emitted lIR/LLVM or machine code for the optimization being claimed.
4. Run the relevant quick benchmarks with verified answers; distinguish
   allocation reduction, indexing improvement, and numerical changes.
5. Before integration, run the full stage-2 gate and bootstrap fixed point.
   Check the known non-passing set deliberately rather than expanding it to
   absorb unexpected failures.

Useful existing entrypoints are `scripts/gate.sh`,
`scripts/bench/quick.sh`, `scripts/bench/dev-loop.sh`, and the shootout scripts.
Honor their suite lock; do not benchmark concurrently with heavy builds or
case runs. Record compiler identity, source tree, CPU target, optimization
level, workload, checksum, run count, median, and memory/allocation metrics.

For optimization validation, a deliberate rollback or planted fault should
make the relevant case fail: restore pair boxing, disable shell reuse,
ignore a view overlap, access an inactive masked lane, or skip cache
invalidation. This provides stronger evidence than comparing outputs from
two paths that share the same bug.

Do not require a frozen interpreter to recognize new features. Use compiled
scalar references, persistent snapshots, runtime auditing, and independently
derived model results where appropriate. Retain the existing audit checks
and investigate the known atom leak separately; neither a successful fixed
point nor a fast benchmark establishes memory safety.

## 9. Decisions to resolve before implementation

- Which types may lose object identity, and is value classification explicit?
- What size/shape limits govern inline aggregates, and are they ABI-stable?
- What are the full-block and masked vector access names and inference rules?
- What do masked operations do with negative offsets and all-inactive masks?
- Which private-cell proofs prevent indirect owner access during a lend?
- How do parallel tile scopes complete after traps or cancellation?
- What diagnostic schema and source-position encoding do clients consume?
- What does each floating-point reduction promise across targets?

Resolve these through the existing decision process, then update the live
specification and executable cases together. The intended outcome is a
language where persistent data, numerical buffers, and development tools
all benefit from the recent compiler progress without unpredictable semantic
changes caused by minor refactoring.
