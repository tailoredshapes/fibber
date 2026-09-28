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
- [x] ownership checker and `fibref explain`
- [x] evaluator following the checker's plan; threads, atoms, async
      executor (deterministic: each run is one valid interleaving);
      user macros with phase separation; `fibref run`
- [x] all 20 cases pass
- [x] the proposed cases promoted: 78 cases, all passing (30 and 35
      withdrawn: they use field places, removed by D1)
- [ ] known gaps: a spin loop on an atom that another thread would set
      hangs under the deterministic executor
- [x] method rule 4: an adversary attacking the running interpreter;
      its 15 findings promoted as cases 81 to 95 after the owner's
      decisions of 2026-09-27 (spec/types.md §10): 93 cases, all passing
- [ ] method rule 5: random well-typed programs, all passing the audit.
      `fibgen` generates them and checks each against a model; its
      findings so far are fixed and promoted as cases 96 to 99 (100
      pins annotated bindings): 98 cases, all passing. A sweep of seeds
      200000..259999 gave 59999 ok and one result mismatch (seed
      233285): an `&` parameter forwarded at a tail call was observably
      different from a copy-in when a later argument of the same call
      wrote the variable. The owner decided (2026-09-28) that the
      copy-in happens at call entry, after every argument (types §10);
      fibref and fibgen's model follow it, and cases 150 to 153 pin it
- [x] the owner's decisions of 2026-09-28 (spec/types.md §10): lift
      the "v1" restrictions and fix what a trap means; cases 101 to 127:
      125 cases, all passing
  - [x] `expect: trap` cases; a trap aborts the program and what is
        live then is not a leak (types §2.11; cases 101 to 104)
  - [x] a float literal of a width other than f32/f64 is an error, read
        or macro-built (syntax §1.1; case 105)
  - [x] `(dyn P :send)`: a distinct `Send` dynamic type (types §2.15;
        cases 106 to 112)
  - [x] colour parameters on structs and enums (`k :colour`, types §1.3;
        cases 124 to 127)
  - [x] protocol supertraits (`:requires`) and default methods; `Ord`
        requires `Eq` (types §4.1; cases 117 to 123)
  - [x] private names: `:private` after a definition's name, `(var
        m/x)` past it (syntax §5, §3.20; cases 113 to 116)
- [x] pattern matching complete enough for a compiler that takes
      forms apart: vector patterns `[p* & rest]` in `match` and `let`
      and guarded clauses `(pat :when g body+)`, after the owner's
      decision of 2026-09-28 lifting syntax open item 11 (spec: syntax
      §1.4, §3.6; types §2.6, §6.3, §8.3, §10). Cases 128 to 149, all
      passing with a clean audit: 147 cases in all. `fibgen` does not
      yet generate either construct
- [x] the owner's decision of 2026-09-28 on the time of the copy-in:
      an `&` argument is copied in at call entry, after all of the
      call's arguments (ownership.md §5; syntax §2, §3.13; types §6.6,
      §10). Cases 150 to 153: 151 cases in all, all passing
- [x] the owner's decision of 2026-09-28 on built-in comparisons: the
      scalar types' `Eq` and `Ord` instances define every method, so
      floats compare as IEEE 754 and `Ord`'s defaults apply only to
      user impls (types §2.12, §8.12, §10). Case 154: 152 cases in all,
      all passing

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

- A forwarded `&` cell written during the call through a closure that
  captures the `&` parameter is still observably different from a
  copied-in one (types §10, "Decided on the time of the copy-in",
  Open).
