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
