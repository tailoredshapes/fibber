# Decisions of 2026-10-04: exclusive views and the interpreter

The owner delegated these (guide: a language that destroys Python at AI and scientific work). Tie-breaker as always: Clojure's
ergonomics unless they break memory safety, then Rust's.

## Exclusive views (docs/design/exclusive-views.md §11)

1. **Sigils or handles.** Keep the sigiled core (the checker sees every write); give the surface a sigil-free macro layer
   (`vget`, `vset!`, `fill!`) so numeric code reads like NumPy indexing.
2. **Closures over view cells.** Forbidden in v1. Parallel work goes through a tile combinator instead, which hands each task its own window.
3. **Read-only lend.** Included: many readers while no writer exists, as for any borrow.
4. **Windows over what.** Contiguous buffers: `Array`, `MArray`, `Tensor`. Not the persistent `Vec`.
5. **Parallel disjoint windows.** Included in the first design (`split!` yields disjoint windows safe across tasks). No GIL is the
   language's main advantage over Python; deferring it would defer the point.
6. Error texts as proposed in the design, including `write (splat x)`.

## Interpreter (docs/design/fibber-interpreter.md §9)

D1 interpret the checked program plus its ownership plan (option a). D2 `Option` of a scalar follows the stage 2 value representation.
D3 macros in v1 reuse `macros.runner`; P9 (interpreter-backed) later. D4 tasks run lazily to completion (A); revisit as B if more than
about 5 of the 51 task cases need preemption. D5 `extern` refused except the whitelist (write, sqrt, strtod, strtof, strfromd).
D6 a separate program `fibi`, renamed `fibref` when the Rust is retired. D7 strings and arrays from the spec; the host only at the
boundary. D8 quick sample in the default gate, full run in `--full`. D9 the `;; stage: 2` labels come off in a separate reviewed
change once P6 is green. Risks R1 to R7 stand as written; P0's census sizes R1 (speed) first.
