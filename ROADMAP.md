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

## M2. Reference interpreter (`fibref`) — done

The executable spec (spec/method.md rules 1 to 5). `crates/fibref`
reads, expands, types and ownership-checks a program and runs it over
an audited heap; `crates/fibgen` checks random programs against it.
Decisions taken on the way are in spec/types.md §10.

- [x] audited heap: counting, cells, atoms, weak references, sharing,
      immortal objects, unique writes, stack scopes, leak
      classification (`crates/fibref/src/heap`)
- [x] reader, macro expander (user macros with phase separation), name
      resolution, type inference, ownership checker (`fibref explain`)
      and `lib/prelude.fib`
- [x] evaluator following the checker's plan (`fibref run`), with
      threads, atoms and an async executor that is deterministic and
      fair (types §8.8, "The reference interpreter's schedule")
- [x] case harness and CI: 166 cases in cases/ownership, all passing
      with a clean audit (its README lists them by origin: the 20
      decided, the promoted proposals, the rule-4 adversary's findings
      81 to 95, the rule-5 generator's, and the owner's decisions of
      2026-09-27 and 2026-09-28)
- [x] method rule 4: an adversary attacking the running interpreter;
      its 15 findings are cases 81 to 95
- [x] method rule 5: `fibgen` generates random well-typed programs and
      checks each against a model of its result and against the audit.
      It covers closures (escaping, stored, self-named), `&` parameters
      and forwarding, cells, atoms, weak references, structs and enums
      with colour parameters, protocols (supertraits, defaults, `dyn`
      and `(dyn P :send)`), vector patterns and guards, user macros (a
      preamble of `defmacro`s), arrays, integers of every width and
      floats (NaN, infinities, `rem`), `spawn`, `plet`, `pmap`,
      `async`/`await` and a spin-wait on an atom another thread sets. Its findings are
      cases 96 to 99; two more led to the owner's decisions pinned by
      cases 150 to 153 and 155 to 161. Sweep of seeds 1100000..1159999:
      59691 ok and 309 traps the model predicted; no mismatch, audit
      failure, run failure, crash, hang or rejection. It does not
      generate: programs the checker must reject, `unsafe` and
      `extern`, `trap`, cycles through cells (the permitted leak),
      programs whose result depends on the interleaving, or a thread
      that waits for its spawner (its model runs a spawned thread at
      the spawn)
- [x] the owner's decisions of 2026-09-27 and 2026-09-28 (types §10),
      each pinned by cases: traps (101 to 104), float literal widths
      (105), `(dyn P :send)` (106 to 112), private names (113 to 116),
      supertraits and default methods (117 to 123), colour parameters
      (124 to 127), vector patterns and guards (128 to 149), the
      copy-in at call entry (150 to 153), IEEE float comparisons (154),
      colours in impl heads (155 to 161), no forwarding of a captured
      `&` parameter (162 to 165)
- [x] the known gap closed: a spin-wait on an atom another thread sets
      hung under the run-to-completion executor; the fair executor runs
      it (cases 166 to 168)

## M3. Hardened lIR — done

lIR ported from liar with the fixes in lir-audit/ and specified in
spec/lir.md (**Decided**, owner, 2026-09-28). `crates/lir` is the
reader, AST and whole-module checker, with no LLVM dependency;
`crates/lair` lowers checked modules through LLVM 21, as a JIT and
ahead of time, and runs the case suite. Every case in cases/lir runs
through both paths in its own process, and both must agree.

- [x] method.md rule 7: the checker and the LLVM verifier run on every
      path and cannot be turned off; an invalid module is an error with
      a position, never a backend crash or wrong code (cases/lir/verify,
      cases/lir/adversarial; `lir-audit/README.md` records what each of
      liar's findings did and which case pins its fix)
- [x] the ADR 021 layer removed; string globals, `fence`,
      `indirect-call` (now typed) fixed; `tailcall` is `musttail` under
      `tailcc`, so a 10^7-deep tail recursion runs in constant stack
- [x] end-to-end ahead-of-time tests: 317 cases in cases/lir (63
      accept, 254 reject), each through the JIT and through AOT
      (`cargo test -p lair`, `lair cases cases/lir`); the checker alone
      re-runs them without LLVM (`cargo test -p lir`)
- [x] `lair` as a library: `Jit` compiles modules in-process and returns
      callable functions; a later module reaches an earlier one by
      `declare` and `declare-global`; a `tailcc` function is called from
      Rust through a `ccc` trampoline the `Jit` generates. The macro
      mechanism of M4 and M6 — compile a module, call a function of it,
      add another module that uses the first — is
      `crates/lair/tests/jit.rs`, "a compiler runs a macro"
- [x] the owner's decisions on lIR's seven open questions (lir.md §14),
      and types.md §8 rewritten to what lIR now holds: `switch`, struct
      `alloca`s, arrays, static data for every constant object, the
      overflow and saturation intrinsics for the checked arithmetic
- [x] what the mapping still lacked, proposed in lir.md §14.1 with
      cases: array types, linkage and visibility, `declare-global`, the
      intrinsics and `trap`, `volatile` and `(align N)`
- [x] CI: the `lair` job installs LLVM 21 from apt.llvm.org and runs
      apart from the `fibref` job, which stays LLVM-free
- [ ] not ported: liar's interactive lIR REPL (`lir-repl`) and
      expression evaluator (`lir`). Nothing in M4 to M6 needs them; a
      compiler runs lIR through `lair run` or the `Jit`

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

None. The last two (the colour argument inside an `impl` on a
colour-parameterised type, and a forwarded `&` cell written through a
closure passed to the same call) were decided by the owner on
2026-09-28 (types §10).
