# Automatic differentiation for fibber: a tape on `fib.tensor`, and the road to a compiler pass

Status: **design with a working prototype**, 2026-10-06. **Measured** means a command was run in this session and its output is quoted
(sections 2, 6, 7; the benchmark is `docs/shootout/autodiff.md`). **Built** means it is in `lib/fib/autodiff/` and a spec in `specs/` exercises it.
Everything else is design and is marked "not built". The toolchain is `main` at a6d06ac plus this branch, stage 2 built by the v0.1.7 seed
(`F build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F`, 65 s); `FIB_LIB` is this tree's `lib`.

The owner's goal for the language: destroy Python at AI and scientific work (no GIL, no C extensions, deterministic memory without a GC, fast).
A training loop (forward, loss, backward, optimiser step, thousands of iterations) is where interpreter overhead and temporary allocations hurt
most, so end-to-end training is the benchmark that can show an advantage. This document says what to build, what was built, what it cost, and what
the numbers do and do not show.

Contents: 0 short answer; 1 requirements and non-goals; 2 the options, with measurements; 3 the recommended plan; 4 the prototype (`fib.autodiff`);
5 semantics decided (broadcast, reductions, ties, in-place, control flow, higher order, numerics, memory, threads); 6 tests, tolerances and mutants;
7 benchmark summary; 8 what NumPy and PyTorch users expect that is missing; 9 not done, risks, decisions for the owner.

## 0. The short answer

1. **Build the tape (option a) first, and it is already built for a useful slice.** `lib/fib/autodiff/` is about 470 lines of fibber over `fib.tensor`: a
   `Tracked` value, a tape of backward closures, `value-and-grad` / `grad` over a function of tensors, the ops of section 4, SGD and Adam,
   forward-mode JVPs and data-parallel gradients. It is generic in the element type, so one definition serves `f32` (the fused `dense` kernels)
   and `f64` (the gradient checks).
2. **Its overhead is small and measured.** On the 784-256-10 MLP, batch 128, `f32`, one thread, the tape step is 0.932 ms and the same step written out by hand with
   `fib.tensor` calls is 0.916 ms: the tape is 1.7% of a step (`docs/shootout/autodiff.md`). A chain of one-element ops costs 0.07 us per op to record and 0.6 us per op to walk back.
3. **The tape's ownership story is real, and testable.** The tape owns what the backward closures saved; `backward` pops an entry off the tape, runs
   it and drops it, so the saved tensors of op k are released before op k-1 runs. A spec proves it with a weak reference (section 5.8), a
   planted fault that holds the chain to the end of the pass fails it, and a case with the memory audit fails on a planted ownership cycle.
4. **Where the time goes is the kernels, not the tape.** The three matrix products are the same speed as OpenBLAS' (0.357 ms against 0.368 ms for
   128x784x256 `f32`, one thread). What separates fibber from NumPy+OpenBLAS in this benchmark is the elementwise glue around the products
   (`where`, `greater`, `sum-axis` are scalar loops in `fib.tensor`); the prototype works around two of them (section 2.1) and the table says
   what remains. **The benchmark does not show fibber faster than NumPy, PyTorch or JAX on this model; see `docs/shootout/autodiff.md` for the
   measured ratios.**
5. **Source-to-source as a macro (option b) is not viable today.** A stage-2 macro sees only the prelude: it cannot recurse by name, cannot call a
   helper from another module, and has no types (section 2.2, with the error texts). **An LLVM/lIR pass (option c) is the right long-term home** of
   differentiation of arbitrary fibber code, but it is a multi-month compiler project (2.3). **Forward mode (option d)** was cheap: 37 lines, and it
   agrees with the reverse mode (a spec checks `JVP = <grad, v>` for random nets).

## 1. Requirements and non-goals

What a scientist or ML user writes, in order of how often:

| Wish | Surface | State |
|---|---|---|
| gradient of a scalar function of tensors | `(ad/grad f params)`, `(ad/grad f)` returns the function | built |
| value and gradient together | `(ad/value-and-grad f params)` returns `(ValueGrad value grads)` | built |
| a training loop | forward, `value-and-grad`, `sgd-step` / `adam-step`, repeat; `no-grad` for evaluation | built (SGD, Adam) |
| `f32` and `f64` | the element type is a type variable of every op | built; checks in `f64`, benchmark in `f32` |
| broadcasting in the forward pass, correct backward | the backward of a broadcast is a reduction (section 5.1) | built for elementwise ops, `broadcast-to`, bias |
| control flow inside the differentiated function | `if`, `cond`, `loop/recur` of the host language: the tape records the executed path (5.5) | built (specs use `loop`) |
| `stop-gradient`, `no-grad` | `ad/stop-gradient x`, `(ad/no-grad ref (fn () ..))` | built |
| Jacobian-vector product | `fib.autodiff.forward`: `jvp` over `Dual` | built (few ops) |
| Jacobian (small outputs) | `(jacobian f xs)`: one reverse pass per output element, or one JVP per input element | **not built**; a 10-line loop over `grad` / `jvp`, section 5.6 |
| gradients through `where`, masks, indexing, slicing, max/min | section 5.2, 5.3 | design only |
| in-place ops | tensors are values; the rule is section 5.4 | design only (nothing to build for correctness) |
| higher-order derivatives (Hessian-vector products, `grad` of `grad`) | forward-over-reverse (5.7) | stretch, not built |
| layer library, conv, attention, batch norm, dropout, mixed precision, GPU | section 8 | roadmap |

**Non-goals** (of this work, and in some cases of the language): a dynamic-shape tracing JIT like JAX's `jit` (fibber is already compiled ahead of
time; fusing a whole step into one kernel is a different project and section 2.3 is how it would start); automatic vectorisation over a batch
(`vmap`: write the batched op); a graph optimiser; GPU backends; distributed training; differentiating through arbitrary pointer code or `unsafe`.

## 2. The options, with measurements

Machine and rules for every number below: single thread (`taskset -c 3`, `OMP/OPENBLAS/MKL_NUM_THREADS=1`), `i7-14700KF`, AVX2+FMA, fibber built with
`F build` (LLVM `-O2`), the median or the best of the runs stated, `ulimit -v 16000000` for fibber. Timings that need a quiet machine ran under
`/tmp/fibsuite.lock`.

### 2.1 (a) A dynamic tape (PyTorch-style): built

**Mechanism.** A forward op computes its value eagerly with `fib.tensor` and, if an input needs a gradient, pushes one `TapeEntry`
`(out-id, [input ids needing a gradient], back)` where `back` is a closure from the output gradient to those inputs' gradients. The closure
captures the tensors its formula needs. `value-and-grad` makes a fresh tape, wraps the parameters as leaves (`id` 0..k-1), calls `f`, seeds the
output with ones and runs the tape backward. Source: `lib/fib/autodiff/{tape,run,ops,nn,optim}.fib`.

**Why a persistent chain and not a vector.** The tape is `(Cell (Chain a))`, a singly linked list of entries, newest first. A first version used a
`(Cell (Vec ..))` pushed with `cell-update!`; the checker refused it (`value of type (fn :local ..) cannot be shared between threads: closure capture
e`): `cell-update!` takes a `:send` function and the entry holds a closure that is not sendable. The chain needs no uniqueness: push is a `Link`
allocation and a `set!`, pop is a `match` and a `set!`, and the order of the chain is exactly the order the backward pass wants. (A `Cell` is never
`Send`, so a tape cannot cross a thread; section 5.9 says what that means for data parallelism.)

**Moved and retained, precisely** (this is what "the tape owns, a backward pass consumes" means in fibber's counts; every row is covered by a
spec or a case, section 6):

| Thing | At record time | At backward time | Released when |
|---|---|---|---|
| a `TapeEntry` | allocated, linked into the tape's chain (the tape's cell holds the chain, the chain holds the entry) | `pop-entry` unlinks it: the cell now holds the rest, the entry is a local | at the end of its iteration (it is not stored anywhere) |
| the backward closure | owned by its entry | called with the output gradient (borrowed) | with the entry |
| a saved tensor (an input value, an output `y`, a mask) | one count held by the closure | read by the closure | with the closure, unless another holder remains: the caller's `let` scope, the parameter vector, the returned loss |
| the gradient slot of an op's output | empty until a consumer's backward writes into it (moved in, no copy; added to the old gradient if the tensor was used twice) | emptied before the closure runs (`assoc gs id nil`): the gradient is moved to the closure's argument | at the end of the iteration, unless it was stored into a parent's slot or returned |
| a parent's gradient | produced by the closure | stored with `(some g)` (a move) or `t/add`ed to the existing one (a new tensor; the two old ones are released) | when that parent's entry runs, or at the end for a leaf |
| leaf gradients | same | kept in the slot vector | returned to the caller as `(Vec Tensor)` (made dense with `t/contiguous` when a view such as a broadcast came out) |
| parameters | borrowed by `value-and-grad` (the caller's vector) | untouched | by the caller; the optimiser step returns fresh tensors, so the old ones go when the caller rebinds |

Two measured facts about the memory model. (1) **Locals live to the end of their scope, not to their last use.** The first version of the
release-timing spec built the whole forward in one function and asked whether `exp`'s saved output was gone before the next backward step: it was
not (`expected: [true false] actual: [true true]`), because the forward local `c` of the test function still held it. The fix was in the test: the
forward is a function that returns, and after it returns only the tape's closures hold the activations. A user who writes the forward inline in a long
`let` pays PyTorch's cost too (Python locals live to the end of the frame): the activations are freed by the backward only if the forward function has
returned. This is why `value-and-grad` takes a function, and why the recommended style is a small forward function. (2) A closure that captures a
`Tracked` value captures its tape (`Tracked` holds it): tape -> entry -> closure -> `Tracked` -> tape is a cycle that no count frees. Case 7710
plants exactly that and the audit reports `leaks=54`.

**Memory high-water against PyTorch.** What the pass holds at its peak is: the parameters, the saved tensors of every op whose backward has not yet
run, and the gradients of the frontier. PyTorch's autograd frees a saved tensor when the node that owns it has run (unless `retain_graph`), so the
two are comparable by design, and the question is only constant factors. Measured on a 12-layer, width-1024, batch-256 MLP, one step above the
parameters and data: section 7 and `docs/shootout/autodiff.md`. A transformer block was **not built or measured** (no attention, layernorm op or
embedding on the tape yet); what it would save per block is listed in section 5.8 as arithmetic, not as a measurement.

**Costs measured.** A tape step of the benchmark MLP (0.932 ms) against the same step written out with `fib.tensor` calls and no tape (0.916 ms): 1.7%. Per op,
on a chain of 1000 elementwise multiplies of one-element tensors (`scripts/bench/autodiff/tape-overhead.fib`, best of 20, one thread):

| | us per op |
|---|---|
| the chain with `fib.tensor` calls, no tape | 0.277 |
| the same with tape recording (forward only) | 0.351 |
| forward and backward | 0.957 |

so recording is 0.07 us an op and the walk back 0.6 us an op (the closure call, the gradient multiply, the accumulation).

Allocations are where a tape costs most in the Python world; here each `fib.tensor` call allocates 20 to 90 small objects of its own (descriptors,
shape vectors, packing buffers), measured with `FIB_TRACE` (`mmul` 71, `dense` 87, `softmax` 20, `sum-axis` 34, `t/relu (t/div y y)` 74, `sgd-step`
of one tensor 94, `ad/softmax-cross-entropy` forward and backward on 128x10: 612; 2145 per SGD step with the tape, 1255 for the hand-written step); the tape adds a closure, an entry and a chain link per op. They
are small objects and the step is dominated by three 0.37 ms products, but they are the reason the allocation count per step in the benchmark is in the
thousands.

**What the prototype does about the library's slow glue** (found by profiling, section 7): `t/where` and `t/greater` are scalar loops (0.135 ms for a
128x256 relu mask against 0.010 ms for NumPy), and `t/sum-axis 0` over 128x256 takes 0.040 ms against 0.003. The backward uses a mask made of
vector kernels only, `relu(y / y)` (1 for `y > 0`, NaN for `y = 0`, and relu sends NaN to +0.0; known limit: `y = +inf` gives 0), and a bias gradient
as a product with a row of ones (one fma chain per column over the rows in increasing order: as reproducible as `sum-axis`, 6x faster, 0.0066 ms). The
right fix is vector `where`/`greater` and an axis-0 reduction in `fib.tensor`; the workaround is two functions and a test each.

### 2.2 (b) Source-to-source as a macro (JAX-style `grad` of a function written with tensor ops): not viable now

A macro `(grad-of (fn [x] ..))` would walk the forms of `f` and emit, next to each tensor op, its VJP call: the forward and backward code with no tape
and no closures, `jit`-like. Experiments on this toolchain (`~/.cache/fibber-scratch/ad/t/m1.fib`, `m2.fib`; spec/syntax.md 3.16):

| Attempt | Result | Consequence |
|---|---|---|
| a macro that calls itself by name to recurse over the argument forms | `macro ddx failed: m1.fib:12:42: unbound name ddx` | a form walker must be one function body with an explicit stack (`loop/recur` over a work list), or the walk is done by a prelude macro |
| a macro calling a helper `defun` in a module it `:require`s (`df/diff-form`) | `macro ddx failed: m2.fib:3:22: unbound name df/diff-form` | the macro-time module is the prelude and the macro; **no library function is visible at expansion time**, not even from a required module |
| a helper that uses `count`, `map`, `conj` (the implicit library) | the same unbound-name error (spec/syntax.md 3.16: the implicit library is not in the macro-time module) | only the prelude's raw operations: `vec-count`, `vec-nth`, `vec-conj`, `match` on `Form`, `loop/recur` |

**Since MACRO-NS (2026-10-06, docs/design/macro-names.md)** the first two rows are lifted: a macro may call functions of the modules its
module requires or uses at expansion time (spec/syntax.md 3.16 "Macro-time helpers"), so the walker is a helper `defun` that recurses by name,
and a macro's template resolves names in the macro's module. Case `cases/stdlib/7933` is the `ddx` experiment, now passing: a derivative
macro over `+` and `*` that calls a recursive helper of a required module. The implicit library is still not in the macro-time module
unless the macro's module (or a module it requires) `:use`s it by name. The verdict below on the source-to-source design is unchanged: the
limits that remain are types, shapes and interprocedural differentiation, not names.

What the expander does offer: macros receive and return `Form` (`Sym Kw Int Flt Str Chr Bool Nil List Vec Map`); quasiquote and `gensym`; expansion is
outermost first and **before name resolution and typing**; `(call-pos)`. What that forbids, concretely:

- **No types and no shapes.** The macro cannot ask whether `(t/mul a b)` is a broadcast or what `a`'s shape is, so it cannot decide how to reduce
  a gradient; it must emit a call to a run-time function that does (so the backward is a call to the same `unbroadcast` the tape uses).
- **No interprocedural differentiation.** It sees the forms handed to it, not the bodies of the functions they call. A function the user wrote in
  another module cannot be differentiated through unless that module also marks it (`defgrad`) and the macro emits a call to its generated adjoint,
  which means every differentiable function is wrapped in the macro by its author.
- **No macro-expanded body.** The macro sees `let`, `cond`, `->`, `loop` unexpanded; it must handle each (the prelude's `cond`, `->>`, `if-some`
  expand to core forms only later). Differentiating a `loop/recur` needs the tape or checkpointing anyway.
- **No access to rules.** The table of VJP rules is a run-time library; the macro can only refer to rule functions by qualified name (as
  `fib.test`'s macros do for `fib.test.core/...`), which works.

Verdict: a macro can emit forward-plus-backward code for straight-line tensor code (`let` of ops) by naming run-time rule functions, and that would
remove the tape's closure and entry allocations (the 0.016 ms per step of section 7, 1.7%). It is a poor trade now: the tape already works for loops, branches and
calls; the macro handles none of them; the expected gain is the tape overhead, which is 1.7% on the benchmark. **Not built, not measured** (there is nothing
to measure that the 1.7% above does not bound). Revisit when the expander can call library functions at expansion time (the owner's open decision in
spec/syntax.md 3.16) or when option (c) exists.

### 2.3 (c) An LLVM / lIR level transform (Enzyme-style): the long-term home, not now

Differentiate the compiled code. fibber has the right substrate: lIR is SSA with `phi`, typed vectors (`f32x8`, `f64x4`), memory, atomics, `fma`
(`spec/lir.md` 6.1 to 6.6), with a reader, AST and whole-module checker in fibber (`compiler/lir/`), a lowering to LLVM through LLVM-C
bindings (`compiler/native/`), and an emitter (`compiler/emit/`) that already knows what every fibber value is. A pass `lir -> lir` that adds a
`f_grad` next to `f` would give `(grad f)` for any compiled fibber function, with no tape allocation for straight-line code, loops differentiated
by the usual caching of loop-carried values, and the whole step visible to LLVM for fusion.

What lair/lIR would need, and the realistic scope:

| Need | Why it is hard here |
|---|---|
| **activity analysis** (which values depend on the inputs and reach the output) | needs alias information for loads and stores; fibber's counted objects and `Array` buffers are all behind pointers, so the pass needs the type-based facts the emitter has (an `Array f32` is not a refcount header) |
| **adjoint (shadow) memory** | every active `Array`, `Vec` of tensors, `Cell` needs a shadow with the same lifetime: the counts of the shadow must mirror the primal's, so the pass must understand `fib.retain`/`fib.release`/`fib.alloc` (inactive calls with an adjoint of their own), the freeze and private-copy rules, and in-place unique writes |
| **control flow** | reverse order needs the forward block trace (a tape of branch decisions) or recomputation; loops need caching of loop-carried values: the same checkpointing problem JAX solves with `scan` |
| **vectors and fma** | the rules are lanewise and exact: `d fma(a,b,c) = (da*b + a*db + dc)` with fma again; reductions (`horizontal add`) become broadcasts. These are the easy part |
| **atomics and threads** | the adjoint of a shared read is an atomic add into the shadow; the adjoint of `spawn`/`join` needs the scope structure (a fork-join in reverse): Enzyme's hardest area |
| **calls to the runtime and libm** | `exp`, `log`, `tanh` need rules; `fib.tensor`'s own kernels (packing, GEMM micro-kernels) are 1000+ lines of lIR the pass must either differentiate (and then the adjoint GEMM is a poor GEMM) or be told the rule (a library-level `custom_vjp`: the same table section 4 has) |
| **types of the result** | a derivative function has a different signature; closures, protocols and generics are monomorphised by then, so it is a post-monomorphisation pass |

Enzyme itself is a C++ LLVM plugin; using it breaks "no C extensions" and would couple to one LLVM major version; fibber's owner already plans to
write lair in fibber (memory: `project_lair_in_fib`), which makes a fibber-written differentiation pass over lIR the consistent choice. **Realistic
scope: a quarter of work for the straight-line and counted-loop fragment with custom rules for the tensor kernels; a year for threads and the whole
language.** It is not measured because it does not exist; the tape (a) is also its test oracle: a gradient from the pass must equal the tape's, and the
gradient-check specs of section 6 are the executable form of that.

### 2.4 (d) Forward mode (Jacobian-vector products on duals): built, small

A `Dual` is a value tensor and a tangent tensor. Each op is the value kernel and the tangent kernel(s): `dmul` is `(a*b, da*b + a*db)`, `ddense` is the
fused `dense` for the value and `dx W + x dW + db` times the activation derivative for the tangent (the derivative function is the one the backward
pass uses). It is `lib/fib/autodiff/forward.fib`, 37 lines, with `dual`, `dadd dsub dmul dtanh dexp dmatmul ddense dsum dmean` and `jvp`.
It needs no tape, saves no activations and costs a small multiple of the forward pass per direction, which is the right trade for a Jacobian of
few inputs or many outputs and for the forward half of Hessian-vector products. "Dual numbers on the SIMD lanes" (packing a scalar's value and
tangent into adjacent lanes of one vector) is the scalar-function version of this; for tensors the whole-tensor Dual already runs every kernel at
vector width, so lane packing is not needed and **was not built**. Check: a spec compares the JVP of a two-layer net along a random direction with the
dot product of the reverse-mode gradient and the direction, over 100 random nets, tolerance `1e-10` relative (it can fail: mutant
`dual-product-rule-drops-a-term` in section 6). Cost of a JVP against a forward pass: one JVP of the benchmark network in one direction takes 1.17 ms, 3.1 times its forward pass (0.375 ms) and 1.35 times the full gradient through the tape (0.866 ms), so reverse mode is the right tool for a scalar loss and forward mode for few inputs or many outputs.

## 3. The recommended plan

| Stage | Content | State |
|---|---|---|
| **A1** | the tape done right: per-op VJPs for elementwise ops, matmul and the fused dense layer with every activation, reductions, broadcast, softmax, cross-entropy, reshape/transpose; accumulation, `stop-gradient`, `no-grad`; numerics policy; gradient-check and mutant suites; the end-to-end benchmark | **built** (this work), minus the items of section 5 marked design |
| **A2** | the rest of the tape: layernorm, `where`/masks, indexing and slicing, max/min with the ties rule, `concat`/`stack`, embedding gather; in-place optimiser updates through windows; vector `where`/`greater`/axis-0 reduction in `fib.tensor` (the glue the profile blames) | design (5.2, 5.3) |
| **A3** | a small `fib.nn` on top (Linear, MLP, LayerNorm, attention block, dropout, an `AdamW`), checkpointing (`remat` of a function) and `jacobian`/`hessian-vector-product` helpers | roadmap (section 8) |
| **B** | data-parallel training on `fib.parallel`: shard the batch, one tape per task, fixed-tree gradient sum (built as a prototype, 5.9); then `freeze`d weights shared by readers | prototype built, not benchmarked |
| **C** | the lIR pass (2.3), using the tape and the gradient-check specs as oracle | long term |
| macro (2.2) | only if the expander gains library access | not planned |

The order is by what a user needs, and each stage keeps the previous one's tests green. The tape stays after C: it is the dynamic path (data-dependent
control flow, debugging, the oracle) as PyTorch's eager mode is next to `torch.compile`.

Decisions that are the owner's (section 9 repeats them): whether `fib.tensor` gets vector `where`/`greater`/axis-0 reductions now (they decide the
elementwise half of the gap in section 7); whether the macro-time module should include the implicit library (spec/syntax.md 3.16, it would open
option b); whether `autodiff` stays a library or becomes part of `fib.tensor`'s facade (the prototype is explicit: `(:require [fib.autodiff :as ad])`).

## 4. The prototype: `fib.autodiff`

Files (all under 500 lines; functions under 50; the self-hosting trap was checked: types `Tape TapeEntry Tracked ValueGrad Chain Link End AdamState
Stepped Dual Real` and the function names were grepped against `compiler/` and `lib/` and none is defined there as a type or core form; the facade is
explicit, so the generic names `add`, `exp`, `sum` live behind the alias `ad/`):

| File | Lines | Content |
|---|---|---|
| `lib/fib/autodiff.fib` | 8 | the facade (`:export-from` the parts, except `parallel` and `forward`, which are required explicitly) |
| `autodiff/real.fib` | 22 | protocol `Real` (`lit` an `f64` constant of the element type, `wide` read as `f64`, `total` the sum accumulated in `f64`) for `f32` and `f64` |
| `autodiff/tape.fib` | 69 | `Tape`, `TapeEntry`, `Tracked`, `leaf`, `constant`, `const-like`, `record1/2/3`, `stop-gradient`, `no-grad` |
| `autodiff/run.fib` | 77 | `pop-entry`, `accumulate`, `backward`, `value-and-grad`, `grad` |
| `autodiff/ops.fib` | 120 | `add sub mul div neg scale exp log tanh sigmoid relu sum mean sum-axis reshape transpose broadcast-to`, `unbroadcast` |
| `autodiff/nn.fib` | 75 | `matmul`, `dense` (six activations), `softmax`, `softmax-cross-entropy` |
| `autodiff/optim.fib` | 36 | `sgd-step`, `adam-init`, `adam-step` |
| `autodiff/forward.fib` | 37 | `Dual`, `jvp` (2.4) |
| `autodiff/parallel.fib` | 25 | `shard-value-and-grad` (5.9) |

A user program:

```clojure
(ns main (:require [fib.tensor :as t] [fib.autodiff :as ad]))

(defun loss (ps: (Vec (ad/Tracked f32)) x: (t/Tensor f32) labels: (Vec i64)) -> (ad/Tracked f32)
  (let [h (ad/dense (ad/const-like (nth ps 0) x) (nth ps 0) (nth ps 1) :relu)
        o (ad/dense h (nth ps 2) (nth ps 3) :identity)]
    (ad/softmax-cross-entropy o labels)))

;; one step
(let [vg (ad/value-and-grad (fn (xs: (Vec (ad/Tracked f32))) (loss xs xb labels)) params)]
  (ad/sgd-step 0.1 params (. vg grads)))              ; a fresh Vec of tensors
```

Design decisions, each with its reason:

1. **Explicit parameters, explicit constants.** `f` takes `(Vec (Tracked a))` and the leaves are the parameters; data enters with `const-like`
   (it takes the tape from a tracked value). There is no global tape (fibber has no mutable globals, and a global would not survive tasks). The
   cost is one `const-like` per constant input; the benefit is that every dependence on the tape is visible in the code.
2. **`id < 0` means constant.** An op whose inputs are all constants (or recorded under `no-grad`) computes its value, pushes nothing, and returns a
   constant: inference and optimiser arithmetic cost nothing extra. `record1/2/3` ask `needs?` per input and **compute the gradient of the inputs that
   need one only** (the closure receives the mask): the first layer's `dX = gz W^T` (a third of the backward flops of the benchmark network) is never computed.
3. **Gradient formulas are closures over tensors, not over `Tracked`.** The backward therefore records nothing, which is why a second derivative is
   not available from this tape (5.7).
4. **One definition for `f32` and `f64`** through the library's own protocols (`Element`, `Num`, `Arithmetic`, `MatrixKernel`, ..) which the checker
   infers as bounds; `Real` supplies constants. Compiled generic code is monomorphised, so there is no dispatch at run time.
5. **`grad` has two arities** (`(grad f)` returns the function, `(grad f params)` applies it); `value-and-grad` likewise. A `:borrow` function parameter
   cannot be captured by the returned closure, so the helpers in the specs take `f` owned.
6. **Optimisers return fresh tensors.** `sgd-step` is `axpby` per parameter (one pass, one allocation each); `adam-step` is about ten passes per parameter
   (`fib.tensor` has no `sqrt`: the first version used the vector `exp(log(v)/2)`, which cost 0.94 ms for one 784x256 tensor because the vector `log` and `exp` take 2.3 ns an
   element; a scalar `map` with `math/sqrt` is exact and cut the Adam step from 1.96 to 1.65 ms; a vector `sqrt` over `simd/sqrt` is the fix, section 9; the spec compares with a scalar reference to `1e-9`). Updating in place through a window (`fib.view`) is stage A2: after `backward` consumed the tape, the parameters are held only by the
   caller, so an in-place update is legal by the unique-write rule; the prototype does not use it.

### Op table: forward, saved, backward

`g` is the output gradient, `y` the output, `s` a `[]` constant; `unb(g, S)` is the reduction of section 5.1. Every row has a gradient check (section 6).

| Op | Saved by the closure | Gradients |
|---|---|---|
| `add a b` | shapes | `unb(g, Sa)`, `unb(g, Sb)` |
| `sub a b` | shapes | `unb(g, Sa)`, `unb(-g, Sb)` |
| `mul a b` | `a`, `b` | `unb(g*b, Sa)`, `unb(g*a, Sb)` |
| `div a b` | `b`, `y` | `unb(g/b, Sa)`, `unb(-(g*y)/b, Sb)` |
| `neg a`, `scale k a` | `k` | `-g`, `k g` |
| `exp a` | `y` | `g*y` |
| `log a` | `a` | `g/a` |
| `tanh a` | `y` | `g*(1 - y*y)` |
| `sigmoid a` | `y` | `g*y*(1 - y)` |
| `relu a` | `y` | `g * relu(y/y)` (1 where `y > 0`) |
| `sum a`, `mean a` | the shape | `broadcast-to(shape, g)`, scaled by `1/n` for `mean` |
| `sum-axis k keep a` | the shape | `g` reshaped to the shape with 1 on the axis, broadcast |
| `reshape d a`, `transpose a`, `broadcast-to d a` | the old shape | reshape back, transpose back, `unb(g, old)` |
| `matmul a b` | `a`, `b` | `g B^T`, `A^T g` (products on transposed views; `fib.tensor` reads strides while packing) |
| `dense x w b act` | `x`, `w`, `y` (and the pre-activation `z` for `:silu`, `:gelu`) | `gz = act'(..) * g` with `gz` as below; `dx = gz W^T`, `dw = X^T gz`, `db = unb(gz, Sb)` |
| `softmax a` | `y` | `y * (g - sum(g*y, axis -1))` |
| `softmax-cross-entropy z labels` | `p = softmax(z)` | `(p - onehot) * (g / m)`; the forward is `mean_i (log sum_j exp(z_ij - max_i) - (z_i,l - max_i))` with the batch sum in `f64` |

`act'` for `dense`: identity `g`; relu `g * mask(y)`; tanh `g(1-y^2)`; sigmoid `g y (1-y)`; silu `g s (1 + z(1-s))` with `s = sigmoid z`; gelu (the tanh form
GPT-2 uses) `g [ (1+th)/2 + z/2 (1-th^2) c (1 + 3 k z^2) ]`, `th = tanh(c (z + k z^3))`, `c = sqrt(2/pi)`, `k = 0.044715`. The fused kernel does not return `z`, so
silu and gelu run as `mmul-bias` plus `activate` (two passes) and save `z`; the four others use the fused kernel and save only `y`.

## 5. Semantics decided

### 5.1 Broadcasting in the backward pass

A forward op that broadcasts an input of shape `S` to `B` makes the input's gradient a **sum over the stretched axes**: leading axes added by
alignment are summed away, axes where the input had extent 1 and `B` more are summed with `keepdims`. `unbroadcast` implements that, with the common
case (a bias, `[m n] -> [n]` or `[1 n]`) as a product with a row of ones (one fma chain per column, increasing row order). The planted fault
"`add` skips the reduction for `b`" and "`unbroadcast` skips axes of extent one" are both killed (section 6). A scalar `[]` input is the all-axes case.

### 5.2 Reductions, max/min and the ties rule (design)

`sum` and `mean` are built. For `max` and `min` over an axis (or all elements) the rule is **the gradient is divided equally among the tied
extrema**, the answer of `torch.amax` and of JAX's `max`, which is symmetric and does not depend on the order of the elements; a value taken by
`argmax` (a gather of one element) gets the whole gradient at the first index, which is what `t/argmax` already returns. The alternative (first index
only) makes `max(x, x)` have gradient `[1 0]` for one operand and `[0 1]` for the other depending on argument order. `relu` is `max(x, 0)` with the
**convention that the gradient at 0 is 0** (built; `x = 0` gives `y = 0`, mask 0). Needs a count of ties per slice, one extra pass over the saved
input; not built.

### 5.3 `where`, masks, indexing, slicing, `concat` (design)

`where(m, a, b)`: `ga = where(m, g, 0)`, `gb = where(m, 0, g)`, each reduced with `unb`; the mask is a constant. A **view** op (`slice`, `transpose`,
`reshape`, `flip`) has a view as its backward: `slice`'s is a zero tensor with `g` written into the window, which is where the exclusive-views
`with-view` helps (write the window of a fresh zero tensor in place; without it, `t/assoc` copies the whole tensor, quadratic over many slices:
a `concat`-style gather of the backward is better). Gather/embedding: the backward is a scatter-add; with repeated indices it must be done in a fixed
order (index order) for determinism. `concat`/`stack`: `record-n` style, one gradient per input, a slice of `g` each. None of these is built.

### 5.4 In-place ops

PyTorch has a per-tensor version counter and refuses the backward if a saved tensor was modified (`one of the variables needed for gradient
computation has been modified by an inplace operation`). **fibber cannot have that bug by construction**: a `Tensor` is a value, `assoc` returns a
new tensor, and a tensor the tape saved has a count of at least 2 (the closure and the caller), so the unique-write protocol of `array-set!` copies instead of
writing (spec/types.md 6.6; cases 4000 to 4007). The rule for the library: **an op never mutates an input; an in-place variant exists only as a
function of a unique argument** (`fib.view` windows), i.e. for optimiser updates after the tape is consumed and for fresh temporaries the backward
creates. No PyTorch-style `x.add_(y)` on a tracked value. Not built; nothing needs building for correctness.

### 5.5 Control flow

`if`, `cond`, `match`, `loop/recur` and calls are the host language's: the tape records the ops that **executed**, in order, so the gradient is that of the path
taken (what PyTorch eager and JAX-without-`jit` do). A data-dependent loop of 12 `tanh` steps is a spec (`ad-a-long-chain-with-a-shared-broadcast-bias`).
The tape grows with the trip count: a loop of N steps saves N sets of activations; the remedy is checkpointing (a `remat` of a function: run it under
`no-grad`, record one entry that re-runs it with a tape in the backward), stage A3, not built. A branch on a `Tracked` value needs `ad/value`
to read it (`(if (> (t/item (ad/value x)) 0.0) ..)`): the comparison is not differentiable and the gradient is that of the taken branch.

### 5.6 `jacobian`

`(jacobian f xs)` for `f` with `m` outputs is `m` reverse passes (the tape is consumed by each, so `f` is re-run) or `n` forward passes (JVP per input
element). Pick the smaller of `m` and `n`; both are 10-line loops over `grad` and `jvp`. **Not built** (the specs check the two against each other for the
scalar-output case, which is the `m = 1` row).

### 5.7 Higher-order derivatives (stretch)

The backward closures call `fib.tensor` kernels on plain tensors, so they record nothing and `grad (grad f)` does not work. Two routes: (1)
**forward over reverse** for a Hessian-vector product: run `jvp` of the function `x -> grad f x`, which needs the backward closures to be generic over
the tensor-or-dual type (the `Dual` ops of `forward.fib` already cover the ops; the VJP formulas would be written once, over a small protocol of ops that
both `Tensor` and `Dual` implement); (2) record the backward (the `create_graph` of PyTorch): the closures call the `Tracked` ops. Route (1) is the
cheaper one for a statically typed language and uses the code that exists. Not built.

### 5.8 Numerics policy

1. **Determinism.** For one worker count (and across worker counts, 5.9) every gradient is a pure function of the inputs: reductions in the backward are
   `t/sum-axis` (ordered, increasing index), the ones-product (an fma chain over increasing rows) and `t/mmul` (each output is one fma chain over the inner
   index in increasing order, whatever its tile: `lib/fib/tensor/README.md`). A spec runs the MLP gradient twice and compares **bits**
   (`ad-the-gradient-is-bit-identical-from-run-to-run`). `sum-fast`, `softmax`'s lane sums and the fused kernels' rounding are not bit-compatible with the scalar
   reference, so bits are not promised across targets with and without hardware fma (as `fib.tensor`).
2. **Accumulation type.** Scalar reductions of `f32` (`sum`, `mean`, the cross-entropy batch sum) are accumulated in `f64` and rounded once (`Real/total`):
   cheap (one pass over a scalar loop on a few hundred to a few thousand elements; the cost is visible only on a large `sum`, where `sum-fast`-style
   `f32` accumulation would be quicker and less accurate). **Matrix products and axis reductions accumulate in `f32`**, as BLAS does (`f64` accumulators
   would halve the kernels' speed); `fib.tensor`'s fused softmax and layernorm use lane sums.
3. **Gradient accumulation** (a tensor used twice) is `t/add old g` in the order the backward runs, which is reverse creation order: deterministic. The
   first contribution is a move, not a copy.
4. **Loss scale and overflow.** None (no mixed precision). `exp` underflow flushes to 0 below -708 (`fib.tensor/vmath`), `log 0 = -inf`: the cross-entropy is
   computed from the shifted logits, not from `log(softmax)`, so a confident wrong prediction gives a large finite loss (`lse - z_label`), not `-log 0`.
5. **Memory arithmetic for a transformer block (not built, not measured).** With batch `B`, sequence `T`, width `D`, heads `H`, a pre-norm block saves
   for the backward: the layernorm input and statistics (`BTD + 2BT`), the fused qkv projection input and output (`BTD + 3BTD`), the attention
   probabilities (`BHT^2`, the dominant term for long `T`), the attention output and projection input (`2BTD`), the second layernorm (`BTD + 2BT`), the
   MLP input, pre-activation and activation (`BTD + 4BTD + 4BTD` at the usual 4x width) and the residual sums (`2BTD`); about `17 BTD + BHT^2` elements,
   which is what PyTorch saves too. The tape frees each as its backward runs; the peak is the whole forward's saved set plus the frontier, as measured on
   the MLP in section 7.

### 5.9 Threads: gradient accumulation across tasks

A tape holds `Cell`s, which are never `Send`: one tape per task. **Data parallelism** therefore shards the batch and runs the whole forward and backward per
shard in a task: `fib.autodiff.parallel/shard-value-and-grad workers shards f params k` calls `fib.parallel/pmap-n` with one chunk per shard, each task builds
its own tape and returns plain tensors (sendable), and the shard gradients are added in a **fixed pairwise tree over the shard index**
(`tree-add`), then scaled. The tree depends on the number of shards, not on the number of workers, so the sum has the same bits for 1, 2, 3 and 4 workers
(a spec compares bits for 6 shards over 100 random problems; the planted fault "the tree drops an odd tail" fails the equal-to-the-whole-batch spec).
Parameters are read by every task: they are `share`-marked at the first `spawn` (atomic counts, 20 to 100 ns per touch under contention,
`docs/design/parallelism.md` 2.5); `freeze` (immortal, no counts) is the remedy for large weight sets and `private-copy` for the closures, both of which the
chunk functions of `fib.parallel` already use. **Not benchmarked** here: the prototype shows correctness and determinism, not speed-up. A trap in a shard
(`NaN` check, shape error) is isolated by `try-join` and its message reaches the joiner (`docs/design/exceptions.md`); the prototype uses `pmap-n`, which traps in the
joiner with the original message.

## 6. Tests, tolerances and mutants

All specs use `fib.test` (`fibc test specs`, `docs/design/test-harness.md`), seed 1, so the random data are the same on every run; they are in the gate's `specs`
stage. `specs/adcheck.fib` is the checker; `specs/ad-ops-spec.fib` has 21 properties (one per op or family, 100 random cases each, shapes 1..5 per axis); `specs/ad-core-spec.fib`
has 22 scenarios (`fibc test specs`: 65 scenarios in 3 files, all passing, with the Vec contract).

**Gradient check.** The analytic gradient of `loss = sum(op(params) * W)` with a fixed random weight tensor `W` (so every output element has its own weight and a
gradient put in the wrong place changes the answer) against the **central difference** `(L(x+h) - L(x-h)) / 2h` in `f64`, `h = 1e-5`, for every element of every
parameter. An element passes when `|analytic - numeric| <= 1e-6 + 1e-5 |numeric|`. The truncation error of a central difference is `h^2 L'''/6`, about `1e-11`
for these functions, and the rounding error `1e-16/h = 1e-11`, so the tolerance is two to four orders of magnitude above the measured error and far below what a wrong formula gives
(the mutants below miss it by more than 1e-2). Inputs avoid the non-differentiable points (`relu`: magnitude at least 0.1; `log`, `div`: bounded away from 0).
Largest absolute error over 100 seeds (`1e-10` to `6e-9`, against a tolerance of at least `1e-6`): add 1.6e-10, mul 1.7e-10, div 4.4e-9, exp 3.7e-10, log 5.6e-9, tanh 9.4e-11, sigmoid 5.3e-11, softmax 5.4e-11, dense with identity 1.7e-10, relu 2.1e-10, tanh 2.4e-10, sigmoid 9.7e-11, silu 1.9e-10, gelu 2.1e-10, softmax cross-entropy 4.0e-11.

**Known answers**, because a gradient check differentiates whatever the forward computes, so a wrong forward that agrees with its own backward passes it (found by the first
mutant run: `mean` dividing by `n+1` survived until `ad-mean-and-sum-have-the-right-value-and-gradient` was written): mean and sum values, the cross-entropy of uniform logits
(`ln 4` to `1e-12`) with its gradient `(p - onehot)/m`, a hand-computed `relu` dense layer (value 7.5, `dW`, `db`), `d/dx (x*x + x) = 2x+1`.

**Scenarios of `ad-core-spec.fib`**: used-twice accumulation, a diamond graph, a 12-step loop with a shared broadcast bias, the MLP (relu hidden, softmax cross-entropy) against finite
differences, a parameter the loss does not read (zero gradient of its shape), a non-scalar output traps, `stop-gradient`, `no-grad` (ops record nothing; recording resumes after),
release timing (a probe op records whether a saved tensor is alive: `[true false]`), `f32` gradients against `f64` within `1e-5 + 1e-4 max|g|` through the fused dense kernel, bit-identical
repeat runs, data-parallel bits for 1..4 workers and equality with the whole-batch gradient to `1e-12`, JVP against `<grad, v>`, SGD, Adam against a scalar reference (`1e-9`), and an Adam
training loop that must drive a least-squares loss below `1e-6`.

**Case 7710** (`cases/stdlib`, memory audit): a forward pass whose backward is never run, and a full training step, leave no object behind (`audit: clean`).

**Mutants** (`scripts/mutant-autodiff.sh F`: plants one fault in `lib/fib/autodiff`, runs the specs, passes only when the run exits with failing scenarios; status 2 means the mutant did
not compile and is reported as an error, not a kill):

| mutant | class | the fault | killed by |
|---|---|---|---|
| `tanh-drops-the-one-minus` | wrong backward formula | `tanh` backward `g*y*y` instead of `g*(1-y*y)` | 3 scenarios |
| `sub-forgets-the-sign` | wrong backward formula | `sub` gives `b` the gradient `g`, not `-g` | 1 scenarios |
| `relu-mask-passes-zeros` | wrong backward formula | the relu mask is all ones (gradient passes where the output is 0) | 4 scenarios |
| `bias-column-sums-doubled` | wrong backward formula | the bias gradient (product with a row of ones) is scaled by 2 | 9 scenarios |
| `mean-divides-by-n-plus-one` | wrong forward formula | `mean` divides by `n+1`, in the value and the gradient (consistent, so only a known answer sees it) | 2 scenarios |
| `matmul-right-gradient-is-doubled` | wrong backward formula | `dB = 2 A^T g` | 1 scenarios |
| `dense-bias-gradient-ignores-the-activation` | wrong backward formula | `db` is the reduction of `g`, not of `gz` | 4 scenarios |
| `cross-entropy-drops-the-batch-mean` | wrong backward formula | the gradient lacks the factor `1/m` | 4 scenarios |
| `fused-dense-drops-the-activation` | wrong forward value | the fused kernel is called with `:identity` | 2 scenarios |
| `add-skips-the-reduction-for-b` | dropped broadcast reduction | `add` returns `g` as the gradient of a broadcast `b` | 2 scenarios |
| `unbroadcast-skips-axes-of-extent-one` | dropped broadcast reduction | axes where the input had extent 1 are not summed | 5 scenarios |
| `accumulate-overwrites` | missing accumulation | a second gradient into the same slot replaces the first (a tensor used twice) | 6 scenarios |
| `stop-gradient-keeps-the-id` | scope | `stop-gradient` is the identity | 1 scenarios |
| `no-grad-still-records` | scope | `needs?` ignores the no-grad flag | 1 scenarios |
| `exp-does-not-save-its-output` | memory: saved tensor released too early | the closure recomputes `exp` from the input instead of holding `y`; `y` is gone when the next backward step looks for it | 1 scenarios |
| `backward-holds-the-whole-chain` | memory: saved tensors released too late | the pass keeps the head of the chain, so no entry is freed until the pass ends | 1 scenarios |
| `closure-captures-the-tape` | memory: leak | the closure captures a `Tracked` (which holds the tape): a cycle; caught by the audit of case 7710 (`leaks=54`) | 1 (case) |
| `shard-tree-drops-the-odd-tail` | data-parallel | an odd shard at a tree level is replaced by zeros | 1 scenarios |
| `shard-mean-uses-the-wrong-scale` | data-parallel | the shard mean is not scaled | 1 scenarios |
| `dual-product-rule-drops-a-term` | forward mode | `d(a*b) = da*b` only | 1 scenarios |
| `adam-without-bias-correction` | optimiser | `c1 = 1` | 1 scenarios |
| `sgd-adds-the-gradient` | optimiser | `p + lr g` | 1 scenarios |

Two mutants teach how the suite can fail to see a fault: `matmul-right-gradient-uses-g-transposed` was an **equivalent mutant** (`(g^T A)^T = A^T g`) and survived correctly, and was
replaced by `matmul-right-gradient-is-doubled`; `mean-divides-by-n-plus-one` survived the gradient check by being consistently wrong and made the known-answer scenarios necessary.

## 7. Benchmark summary

`docs/shootout/autodiff.md` has the commands, the table, the profile and what limits each row. In one paragraph: on the 784-256-10 MLP (batch 128, `f32`, one thread, generated data, identical initial loss in all programs, median of 5) fibber's SGD step is 0.932 ms against NumPy+OpenBLAS 0.897, PyTorch 0.927 and JAX 0.893; its Adam step is 1.649 ms against 1.537, 1.155 and 1.030; the loss curves agree within 1e-5 (SGD) and 1e-3 (Adam) at every step; the three matrix products run at OpenBLAS speed (0.357 against 0.368 ms) and the gap is elementwise glue and, for Adam, nine separate passes where PyTorch and JAX do fewer; a 12x1024 model's step needs 53 MB above its parameters against PyTorch's 66 MB. **fibber is not faster than the three on this model**; it is within 4% for SGD and 7% to 60% slower for Adam, in a process 7 to 17 times smaller (21 MB against 155 to 351 MB).

## 8. What NumPy and PyTorch users expect that is missing (the roadmap)

| Expectation | State | Notes |
|---|---|---|
| convolutions (conv1d/2d, pooling), im2col or direct | missing | needs strided windows (`t/slice` views exist) and a GEMM-based path; backward is the transposed convolution |
| attention blocks (multi-head, causal mask, flash-style fusion) | missing | `matmul`, `softmax` exist; needs batched matmul (`fib.tensor` mmul is rank 2 only), `where`/mask backward, transposes over 4-d views |
| layernorm / batch norm / group norm on the tape | `t/layernorm` kernel exists, no backward | formula in 5.x; batch norm adds running statistics (state) and a train/eval switch |
| dropout and random tensors | missing | needs a counter-based generator (`fib.rng` is SplitMix64 and stateful) so a mask can be regenerated in the backward without saving it; per-task streams for determinism |
| embeddings, gather/scatter, `one_hot`, `cross_entropy` with class weights and label smoothing | partial (`softmax-cross-entropy` with integer labels) | scatter-add backward in fixed order (5.3) |
| optimisers: AdamW, RMSprop, Adagrad, momentum/Nesterov SGD, learning-rate schedules, gradient clipping, weight decay | SGD and Adam only | each is one function over a vector of tensors; clipping needs a global norm (a fixed-tree reduction) |
| `nn.Module`-style parameter containers, `state_dict`, save/load | missing | needs a serialisation of tensors (a `.npy`-compatible writer is the useful first step) |
| mixed precision (`bf16`/`f16`, loss scaling, `autocast`) | missing | `fib.tensor` has no `bf16` or `f16` element; `f32` accumulation in `f64` exists for scalars |
| GPU and other devices | missing | out of scope for the library; the lIR route (2.3) is the way to a second backend |
| `vmap`, `jit`, `checkpoint`, `jacfwd/jacrev`, `hessian` | `jvp` only | 5.5 to 5.7 |
| data loading, datasets, a `DataLoader` with workers | missing | `fib.parallel` and `fib.os` are the pieces; no tensor file readers yet |
| broadcasting dtype promotion, integer tensors in the loss | missing | `fib.tensor` requires one dtype per op |
| rank > 2 batched `matmul`, `einsum`, `tensordot` | missing | `mmul` is rank 2 |
| vector `where`, `greater`, axis-0 reductions, `sqrt`, `pow`, `clip`, `erf` | slow or missing | the profile in section 7 names the first three |

## 9. Not done, risks, decisions

**Not done** (said once, here): attention/transformer blocks and their memory measurement; layernorm, `where`, indexing, max/min on the tape; second derivatives;
`jacobian`; in-place optimiser updates; checkpointing; a timed data-parallel run; PyTorch/JAX on more than the one MLP; the macro and lIR options as code; any GPU work;
a doc-comment API reference. The tape was benchmarked on one model shape (784-256-10, batch 128) and one deep memory model.

**Risks.** (1) The prototype relies on the checker inferring protocol bounds for generic ops; a future checker change that stops inferring `:where` would need them written out
(most of `fib.tensor` writes them). (2) Backward closures capture tensors, so the memory profile depends on the caller's scopes (2.1); a user who writes a 1000-line `let` forward
holds every activation until it returns. (3) The relu mask by `y/y` is a trick that has a known limit (`+inf`); the exact fix is a vector `where`. (4) `ad/dense` with `:silu` and
`:gelu` is two passes in the forward; a fused kernel that also returns the pre-activation would remove one. (5) Tape entries and closures allocate per op: a model made of
thousands of tiny ops (scalar autodiff) pays about 1 us per op (2.1); that is not the workload this is for.

**Decisions for the owner.** (1) Vector `where`/`greater`/`sum-axis 0` in `fib.tensor`: the profile blames them for most of the elementwise gap to NumPy; worth doing before any further
autodiff work. (2) Whether `fib.autodiff` joins the `fib.tensor` facade or stays explicit. (3) The macro-time module (open since the flip of stdlib tranche 1): including the implicit
library would reopen option (b). (4) Whether the next investment is the lIR pass (2.3) or A2 and A3 (a usable `fib.nn`): the recommendation is A2/A3 first, since users need layers
before they need a compiler pass.

**Update 2026-10-06.** Decision 1 and risk 3 are closed in `fib.tensor` (vector `where`/`greater`/`relu-grad`, `sum-axis` lane kernels, vector `sqrt`, fused `adam-step`): `fib.autodiff` no longer has the `relu(y/y)` mask, the product-with-ones bias gradient or the scalar-map `sqrt`; the Adam step is 1.220 ms (was 1.649) and level with PyTorch in the run of `docs/shootout/autodiff.md` section 8. In-place optimiser updates (section 5.4) were probed and not achieved: a tensor held by a `Vec` or reached through its struct field keeps its buffer shared.
