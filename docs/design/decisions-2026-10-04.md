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

## Amendment, later on 2026-10-04: the interpreter is a development tool

The owner: "Rule 6 can suck it. We're past that. The interpreter is a development tool. It helps agents and meat developers as
quickly as possible." This supersedes the interpreter decisions above where they serve rule 6. Dropped: the independence
requirement and script, the divergence table, `fibi compare`, the compare gate stage, mutants of the comparison, the audit-fidelity
goal. The interpreter may reuse anything (front end, expander, even the JIT), and is judged by speed to first result: start-up
time, a REPL, `run` without an LLVM compile, readable errors and traces, completion for the IDE. It needs a new design pass.

## Dev loop (docs/design/dev-loop.md §6), decided by the lead under the owner's delegation

1. Yes: `fibc run -O 0` uses FastISel; `-O 1` is for long computations; `build` unchanged. 2. Yes: opt-in `fibc serve`, client falls
back in-process. 3. Yes: the JSON diagnostics schema of §4.1 with a stable `code` per error kind. 4. Yes: session values are borrowed
by later inputs, consumed only by explicit `take!` or `clone`. 5. Yes: `fibber-interpreter.md` is superseded; no `fibi`, no
`compare`; the interpreter decisions D1 to D9 above are void.

## fibref port (docs/design/fibref-port.md §11), decided by the lead on 2026-10-05

The owner asked for fibref and fibgen to be ported and the Rust oracles left to git history. D1 `fibref`, source `compiler/fibref.fib`.
D2 the language server in both `fibc` and `fibref` (one `lsp.server`). D3 the interpreter links LLVM in v1 (macros via the JIT).
D4 a compiled `FIB_AUDIT=1` mode: after F1, not now. D5 no catch in fibber: fuzz the front end on garbage buffers first. D6
`editors/vscode/test/lsp.js` fails rather than skips when no server exists (unless `FIBREF_SKIP=1`). D7 keep the `;; stage: 2` labels.
R1 measure at F2's first milestone, plan B below 1M nodes/s. R2 re-exec under `setrlimit` for stack depth. Order: the language server
first (the editor pack is broken without it), then the heap and audit, evaluator, builtins, tasks, commands.

## fibgen port (docs/design/fibgen-port.md §6), decided by the lead on 2026-10-05

1 a standalone tool `compiler/fibgen.fib` beside `fibc` (a `fibc gen` entry later, at no cost). 2 bit-for-bit equality with the seed-1
Rust per seed and size until the port is accepted, then the manifests freeze. 3 the model's object limit is 4 million (the Rust has none:
seed 1409, kind programs, size 5, asks for one 2.3 GB allocation and killed the session on 2026-10-05; the port bounds it). 4 no threads in
the generator. 5 classification from the compiled run only; a compiled outcome with a failed audit is an audit failure.

## Exceptions (docs/design/exceptions.md §7), decided by the lead on 2026-10-05

The owner: catching a trap "seems like a big one". Decisions: (1) mechanism (b), a transitively inferred "may throw" effect with a hidden
`{T,i1}` result reusing the scope-exit release code; landing pads (a) held in reserve. (2) Stage 1 first: a trapping task is isolated at
the thread entry and `try-join` returns the trap as a `Result`; plain `join` and `@t` keep trapping in the joiner (case 911 changes to use
`try-join`). (3) Fatal and uncatchable: out-of-memory, stack overflow, thread-start failure, and `(trap)` in the runtime's own five sites.
(4) `&` in-out cells are written back on unwind; `cell-update!` poisons the cell as Rust's `Mutex` does; an `array-take!` region must be
provably trap-free. (5) A trap inside a `finally` that runs during unwinding is fatal. (6) `fib.spawn` never detaches finished threads (20,000
spawns used 165 MB; ~2,000 live threads abort under a 16 GB cap): fixed first, independent of the rest.

## Parallelism (docs/design/parallelism.md), decided by the lead on 2026-10-05

The owner: "I also want best in class parallelism". Decisions: three tiers with one `Task` type (`async` as it is; a new pool tier with work-stealing deques and
help-while-waiting joins; `spawn`/`future` stay the OS-thread tier); structured `with-tasks` scopes with `try-join` trap semantics and chunked
`pmap`/`pfor`/`preduce`/`pscan`; a fixed-tree deterministic reduction as the DEFAULT (identical bits for any worker count, measured free);
tensor kernels parallelise themselves above a size threshold; refcount contention is the main hazard (lock-free scalar atoms, `freeze`, no
counts on peeked reads). Order: P-race (two real races TSAN found: the plain `fib.mt` global; `tile-window`'s element-0 seed), then P-struct
and P-count-a in parallel, then P-tensor, P-sched, P-chan, P-det, P-count-b.

## P-count-b (docs/design/parallelism.md 3.6.1), decided by the agent on 2026-10-06 within the lead's brief

1 `freeze` goes through objects that are already SHARED and refuses a cell by type (`Send`), an atom, weak reference or task by a run-time trap; a refused freeze may leave the objects above the refusal frozen (safe). 2 The share-marking walk keeps its mode in its worklist, not in a global. 3 Borrowed element reads: not extended; the derived-read rule covers a builtin `array-get`, not a prelude call (measured). 4 The `:scoped` closure colour is not built: `private-copy` (a per-chunk shallow copy of a shared closure) gives the call cost without a new colour or a second closure ABI. 5 A pmap or pfor chunk that traps abandons its closure copy too (case 7608: 86 to 88).

## Supported instruction sets (the owner, 2026-10-06)

"I'm comfortable saying we only support isa that have tail call capabilities. I am also happy to say the same thing about FMA. Its 2026. This
language is partly about making modern capabilities like simd and multiple cores easier to access." A native target is supported only when
its toolchain guarantees tail calls between functions of any signature (LLVM `tailcc`/`musttail`) and the instruction set has fused
multiply-add. Consequences: the x86-64 baseline is x86-64-v3 (AVX2 and FMA; releases move from x86-64-v2); AArch64 is unchanged; riscv64 is
parked until LLVM implements `tailcc` for RISC-V (its emit check stays, to notice when it lands). wasm and JS: open question to the owner
(proposal: portability targets with software FMA and return_call/trampolines, same results, slower).

## Exceptions stage 2 (docs/design/exceptions.md 9, docs/adr/0009), decided by the agent on 2026-10-06 within the lead's brief

1 The may-throw effect is decided per program: a program that calls `catch-run` compiles every function to return its failure next to its
value, any other compiles as before (identical user code); a per-function inference inside a catching program is not built. 2 A result of
two leaves or a SIMD vector takes a failure slot parameter instead of a third leaf, so that tail calls still jump. 3 `cell-update!`'s
function and a function with an `array-take!` do not pass failures on (fatal there) in place of poisoning and of a trap-free-region
checker: sound, and catchability there is what is lost. 4 `try` is a macro of fib.ex over the builtin `catch-run` and a closure, not a
core form; one exception type, `(ExInfo (Map keyword Datum))`, and `(catch e :when cond ..)` for Clojure's `(catch T e ..)`. 5 A trap
unwinds only while a catch is active on its thread; elsewhere it is the trap it was (abort, or the task's failure of stage 1).

## GPU (docs/design/gpu.md), the owner, 2026-10-07

The kernel target and `fib.gpu` are CORE, not a driver: "I think its core, we just have to be careful to reject on platforms that don't have
a gpu... 'Defkernel' is a smart way of making it a deliberate dev choice." So: `defkernel` is the deliberate opt-in; a program with a
kernel built for a platform that has no kernel target is rejected with a clear message naming the kernel and the platform (never a silent
CPU fallback); the drivers (fib-gpu-cuda, later Metal/Vulkan) stay in their own repositories per ADR 0011; the lIR forms of spec/lir.md
§6.9a (`kernelcc`, `(sreg R)`, `(barrier)`, the align-1 rule) are accepted. Phase 1 (the `:kernel` core form, the source-level kernel
checker, the launch ABI, the `fib.gpu` builtins and the `Device` protocol) is the next package.
