# Roadmap

**The goal is a self-hosting fibber:** a fibber compiler written in
fibber that compiles itself. Everything before that is a milestone, not
a destination. Work is ordered so that the bootstrap starts only on a
proven core: liar's self-hosted compiler started on an unproven one,
and every compiler bug became two.

Nothing on this page counts as done until an executable test says so
(spec/method.md).

## M1. Specification — done

spec/ownership.md, spec/syntax.md, spec/types.md, decided by the owner
after six adversarial review rounds. 20 decided cases in
cases/ownership; 80 proposed in spec/drafts/PROPOSED_CASES.md.

## M2. Reference interpreter (`fibref`) — in progress

The executable spec: reader, expander, types, ownership checker and an
evaluator over the audited heap.

- [x] audited heap: counting, cells, atoms, weak, sharing, immortal
      objects, unique writes, stack scopes, leak classification
- [x] case harness and CI
- [x] reader
- [x] macro expander (user `defmacro` waits on the evaluator)
- [x] name resolution and type inference; `lib/prelude.fib`
- [ ] ownership checker and `fibref explain`
- [ ] evaluator following the checker's plan; threads, atoms, async
      executor; user macros
- [ ] all 20 cases pass; the proposed cases promoted (30 and 35 are
      withdrawn: they use field places, removed by D1)
- [ ] method rule 4: an adversary attacking the running interpreter
- [ ] method rule 5: random well-typed programs, all passing the audit

## M3. Hardened lIR

Port lIR from liar with the fixes in lir-audit/: verification on by
default, a module-level type checker, the ADR 021 layer removed,
string globals, `fence` and `indirect-call` fixed, `musttail`, and
end-to-end ahead-of-time tests. The JIT is a first-class path, not an
extra: `lair` is usable as a library that compiles a module and returns
callable functions, because compilers run macros through it (M4, M6).

## M4. Compiler (`fibc`, in Rust)

Lower the checker's plan to lIR, with a small runtime (header,
retain/release, share marking, atom locks, weak table, task executor)
and monomorphisation. `fibc` runs macros by JIT-compiling the
macro-time module through `lair`, proving the mechanism the bootstrap
depends on; its expansions must match `fibref`'s. Every case and generated program runs both
interpreted and compiled; results and free traces must match (method
rule 6). This is the "working language" milestone.

## M5. A library a compiler can live on

Prioritised by what a self-hosted compiler needs, ahead of anything
else in the library:

- strings: building, slicing, comparing, hashing, efficiently
- hash maps and sets (symbol tables, environments)
- the persistent vector as a real trie (today's `conj` copies)
- file I/O, command-line arguments, exit codes, stderr diagnostics
- multiple modules (§5 of syntax.md beyond one module plus the prelude)
- macros at compile time (decided, below): the compiler JIT-compiles
  the macro-time module through lIR, so the self-hosted compiler links
  `lair` as a library

## M6. Bootstrap

1. Stage 1: the Rust `fibc`.
2. Stage 2: the fibber compiler written in fibber (reader, expander,
   types, ownership checker, lIR emitter printing lIR text for `lair`),
   compiled by stage 1.
3. Stage 3: the compiler compiled by stage 2.
4. Done when stage 2 and stage 3 emit identical lIR for the whole case
   suite and for the compiler itself, and the stage-3 compiler passes
   every case with a clean audit.

`lair` (lIR to native, via LLVM) stays in Rust, as LLVM stays in C++.

## Decisions

- **Macros at compile time: JIT, with phase separation** (owner,
  2026-09-27; syntax §3.16). Like Clojure, there is no macro
  interpreter: macros are compiled through lIR and run with its JIT,
  one execution path for everything. Unlike Clojure's AOT, compiling a
  module runs nothing of that module: only macro definitions and what
  they reach in already-compiled required modules run at expansion
  time.

## Open decisions

None.
