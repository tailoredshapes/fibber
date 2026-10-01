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
- [x] case harness and CI: 191 cases in cases/ownership, all passing
      with a clean audit (`fibref cases cases/ownership`, run
      2026-10-01: "191 cases: 191 pass, 0 fail, 0 pending"; there were
      184 when M5 closed and 187 to 190 and 192 to 194 came with M6,
      case numbers 30, 35 and 191 being unused; its README lists them
      by origin: the 20 decided, the promoted proposals, the rule-4
      adversary's findings 81 to 95, the rule-5 generator's, and the
      owner's decisions of 2026-09-27 and 2026-09-28)
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
- [x] end-to-end ahead-of-time tests: 323 cases in cases/lir (64
      accept, 259 reject), each through the JIT and through AOT
      (`cargo test -p lair`, `lair cases cases/lir`); the checker alone
      re-runs them without LLVM (`cargo test -p lir`)
- [x] method rule 7 attacked: `lair fuzz`, a mutation fuzzer over the
      accept cases (lir.md §10.1); 80,000 mutants and a batch of
      hand-written modules, one finding (tail-call results returned in
      memory, §7.3 rule 5), kept as adversarial cases
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
- [x] what the mapping still lacked, added with cases and decided by
      the owner on 2026-09-28 (lir.md §14 items 8 to 11): array types,
      linkage and visibility, `declare-global`, the intrinsics and
      `trap`, `volatile` and `(align N)`
- [x] CI: the `lair` job installs LLVM 21 from apt.llvm.org and runs
      apart from the `fibref` job, which stays LLVM-free
- [ ] not ported: liar's interactive lIR REPL (`lir-repl`) and
      expression evaluator (`lir`). Nothing in M4 to M6 needs them; a
      compiler runs lIR through `lair run` or the `Jit`

## M4. Compiler (`fibc`, in Rust) — done

Lower the checker's plan to lIR, with a small runtime (header,
retain/release, share marking, atom locks, weak table, task executor)
and monomorphisation. `fibc` runs macros by JIT-compiling the
macro-time module through `lair`, proving the mechanism the bootstrap
depends on; its expansions must match `fibref`'s. Every case and generated program runs both
interpreted and compiled; results and free traces must match (method
rule 6). This is the "working language" milestone.

State (spec/compiler.md, **Decided**, owner, 2026-09-30; `crates/fibc`):

- [x] `fibc` reuses `fibref`'s front end and lowers the plan to lIR
      text that `crates/lir` re-reads and re-checks (`tests/emit.rs`,
      no LLVM needed); `lair` compiles it (`fibc run`, `fibc build`)
- [x] the runtime `fib.rt` as lIR source (`crates/fibc/rt`): header,
      counts, drop, unique test, traps, stack objects, strings, arrays,
      the prelude's `Vec`, atoms and share marking, weak references,
      threads, tasks; and a trace mode for the free trace of
      compiler.md §4
- [x] method rule 6 harness: `fibc cases` runs every case interpreted
      and compiled and compares results, rejections, traps and free
      traces; all 191 cases pass both ways, 0 fail, 0 pending
      (`fibc cases cases/ownership`, run 2026-10-01)
- [x] macros through the JIT: one macro-time module per `defmacro`,
      `gensym` and reflection through hooks into the expander;
      `tests/macros.rs` shows the expansions equal `fibref`'s on every
      case that defines a macro
- [x] `async` as the state machine of types §8.8, resumed by a pool of
      workers and by its joiners through one run queue (owner,
      2026-09-30; compiler.md §8 item 3; `resume.rs`, `rt/task.lir`;
      cases 174 to 176 beside the earlier async cases)
- [x] `(Weak (dyn P))` (compiler.md §8 item 10, cases 87 and 110)
- [x] the native `Show` and `Hash` instances on scalars, field-less
      enums and `str` (cases 169 and 178; the texts decided in types
      §2.12, compiler.md §8 item 11)
- [x] `def` initialisers through the JIT (compiler.md §8 item 4;
      `defs/jit.rs`, with the interpreter backend kept as the
      executable spec of the same constants, `tests/defs.rs`)
- [x] a `(dyn P)` of a native instance: the vtable slot is a function
      around the native method (case 177, `(dyn Hash "abc")`)
- [x] generated programs through the harness: `fibc gen` writes each
      program `fibgen` generates as a case whose header is the model's
      verdict and runs it both ways (`tests/gen.rs`: the first 120
      seeds). Its first 660 seeds found four compiler faults and one
      harness gap, each pinned as a case (170 to 173) or a decision
      (compiler.md §8 item 12)
- [ ] an intermittent crash of compiled threaded programs under load:
      on 2026-09-29 the first runs of `tests/gen.rs` saw the child of
      seeds 4 (size 4), 33 (size 3) and 86 (size 2) die of SIGILL or
      SIGSEGV once each, with nothing on standard error; 6000 later
      runs of the same three programs at 56-way parallelism and under
      a CPU hog, and their AOT executables, all passed. Not reproduced,
      not understood, not closed; `fibc gen` keeps any program that
      fails, so a long sweep is the way to catch it again
- [x] the owner's answers to compiler.md §8 (2026-09-30: every item
      decided, the page **Decided**)

## M5. A library a compiler can live on — done

Prioritised by what a self-hosted compiler needs, ahead of anything
else in the library; every addition is pinned by cases that run
interpreted and compiled:

- [x] hash maps and sets (symbol tables, environments): `Map` and
      `Set` as an HAMT in the prelude, with `Associative` (`assoc`,
      `get`), `dissoc`, `contains?`, `map-put!`, `map-del!`, `Entry`,
      `for-each` over entries, `set-empty`, `disj`, `set-contains?`
      (cases 179 to 182)
- [x] the persistent vector as a real trie: the prelude's `Vec` has
      been a 32-way trie since M2 (`VNode`, `VecOf`)
- [x] macros at compile time (M4): the compiler JIT-compiles the
      macro-time module through lIR, so the self-hosted compiler links
      `lair` as a library
- [x] strings: `str-from-bytes` (the inverse of `str-bytes`, checked
      UTF-8) beside the builtins that were there (`str-slice`, `str-eq`,
      `Ord`, `Hash`), and in the prelude `str-join` (one allocation for
      a vector of parts), `str-chars`, `char->str` (cases 183 and 184)
- [x] file I/O, command-line arguments, exit codes, stderr
      diagnostics: `read-file`, `write-file` and `args` as builtins,
      `println` beside `eprintln`; `fibc run FILE -- a b` and `fibref run
      FILE -- a b` hand the arguments on, and a built executable returns
      `main`'s result as its exit status and prints nothing of its own
      (compiler.md §1; cases 185 and 186, `crates/fibc/tests/cli.rs`)
- [x] multiple modules (syntax §5): `ns` with `:require` aliases and
      `:use`, modules found under the main file's directory and read
      once each in dependency order, private names and `var` across
      modules, in both tools (`crates/fibref/src/modules.rs`;
      `cases/modules`, 6 programs both harnesses run), macros included:
      reached through `:use` and aliases, `:private` ones at home

## M6. Bootstrap

1. Stage 1: the Rust `fibc`.
2. Stage 2: the fibber compiler written in fibber (reader, expander,
   types, ownership checker, lIR emitter printing lIR text for `lair`),
   compiled by stage 1.
3. Stage 3: the compiler compiled by stage 2.
4. Done when stage 2 and stage 3 emit identical lIR for the whole case
   suite and for the compiler itself, and the stage-3 compiler passes
   every case with a clean audit.

State (spec/bootstrap.md, **Proposed**; `compiler/`; the counts below
are from runs on 2026-10-01):

- [x] step 1, the reader: `compiler/syntax/*.fib` reads text to `Stx`
      (a form with its position) and prints it; `compiler/read.fib`
      prints what `fibref read` prints, in dump and in `--print` mode,
      byte for byte and with the same exit status.
      `cargo test -p fibc --test bootstrap` compares, in each mode:
      1212 inputs (the 1211 `.fib` files of `cases/`, `lib/` and
      `compiler/`, 983 of them the edge inputs of
      `compiler/tests/reader/`, and the expander's prelude), 4
      unreadable paths, 417 Unicode-class files and 300 generated
      inputs; it reports a changed output of the real tool (the canary)
      and reads five large inputs against a time bound of 5 s (the
      slowest took 0.06 s). Two one-off mutation reviews (the first of
      1797 mutants) found the gaps in that test; the scripts and mutant
      lists are not in the repo, only the killing inputs are:
      `compiler/tests/reader/rmut-001..021` and `rmut2-001..006`
      (`rmut2-004..006` are the 250 KB timing inputs).
- [x] the C interface to `lair` (spec/compiler.md §9): 21 `lair_*`
      functions in `liblair.so` (`nm -D --defined-only
      target/debug/liblair.so | grep -c ' T lair_'` prints 21),
      declared in `crates/lair/include/lair.h`, which a unit test keeps
      equal to the exports; the fibber bindings `compiler/lair/*.fib`
      and `compiler/jit-demo.fib`, which runs the macro modules of
      `cases/ownership` from fibber and compares them with the Rust
      runner (`cargo test -p lair`, `crates/fibc/tests/capi.rs`).
      `fibc build FILE -o OUT -L DIR -l LIB` links with an absolute
      rpath per `-L` (`lair::aot::Options::lib_dirs`,
      `crates/lair/tests/link.rs`). Not yet reachable through the C
      interface: a library directory for `lair_build_executable`.
- [x] stage-1 faults the compiler work found, each fixed with a case or
      a test (spec/bootstrap.md §4 has 13 rows): float `show` (case
      187), `read-file` (188), `strtod`/`strtof` in the interpreter
      (189), `alloc` zeroed (190), `println` writes (192), a raw `ptr`
      counted as an object (193), float bit casts (194), the macro
      module's keyword table, `(args)` bytes, `fibc build -L`, and out
      of memory in the runtime; two more rows are notes. A failed
      `(alloc n)` traps `out of memory` in both tools (case 195).
- [ ] a mutation review that can be repeated: both reviews of the
      reader ran from scripts that are not in the repo
- [x] step 2a, the expander (`compiler/expand/`, tool
      `compiler/expand.fib`): its dump equals `fibref expand
      --no-runner` on 2,533 programs in five modes (plain, `--context`
      and three limit modes), 0 failing
      (`cargo test -p fibc --test bootstrap_expand`). The JIT macro
      runner now keeps the positions of the forms a macro was given, as
      the interpreter does. Stage 2b, user macros through lair, needs
      the later passes.
- [ ] types (10,800 Rust lines), ownership (4,400), the lIR emitter
      (8,000), macros through lair (750), the driver (830)

**Paused** (owner, 2026-10-01) after step 2a until M7's library is
viable. The faithful ports so far are 0.8 times the Rust's code lines
and 1.25 times its bytes, because the prelude lacks what makes Clojure
terse (`try-let`, `reduce`, `pop`, destructuring, sort); porting the
remaining 25,000 lines in that style would mean rewriting them. Until
then step 2a stays in step with the Rust expander: a library package
that changes the Rust expander or the prelude mirrors the change in
`compiler/expand/` and keeps `bootstrap_expand` at 0 failing.

`lair` (lIR to native, via LLVM) stays in Rust, as LLVM stays in C++.
The C interface to `lair` (spec/compiler.md §9) is a stopgap, not a
design to polish: once there is a compiler, `lair` itself is to be
rewritten in fibber (owner, 2026-10-01), at which point this line and
that interface go.

## M7. A standard library as ergonomic as Clojure's, as fast as Rust's

**Aim** (owner, 2026-10-01): "steal Clojure's, or as close to it";
improve inconsistencies and un-idiomatic corners where there is a reason;
the language is to be as ergonomic as Clojure with a run-time
performance that rivals Rust. Status: **Proposed**, not started; the
rules below are mine, for the owner to amend.

Rules:

0. **The tie-breaker** (owner, 2026-10-01): unless it breaks memory
   safety, Clojure has the ergonomics we are replicating; where Clojure's
   way would break memory safety, Rust has them. A deviation from
   Clojure needs a memory-safety reason (or the static-typing the
   language already decided), recorded in spec/stdlib.md; "it is
   cleaner" or "it is faster" is not one.
1. **Clojure's names and shapes first.** `map filter reduce assoc conj
   get first rest nth into take drop partition group-by frequencies
   sort-by update assoc-in get-in merge select-keys keys vals str ...`,
   with Clojure's argument order (sequence functions take the
   collection last, collection functions first, so `->` and `->>` keep
   working). A deviation is recorded with its reason in a table
   (spec/stdlib.md), the way spec/bootstrap.md §4 records gaps. The
   known inconsistencies to decide, not inherit: `contains?` on a
   vector (index, not element), `nil` punning against fibber's
   `Option`, `rest` versus `next`, `=` across numeric types, `first` of
   a map, `conj` onto a list versus a vector, `empty?` versus `seq`.
2. **Zero-cost by construction.** Everything generic is monomorphised
   and protocol calls are static (compiler.md §7). Sequence functions
   work over `Traversable`/`Iter` and return fused adaptors, not lazy
   cells: `(->> v (map f) (filter p) (reduce g 0))` must compile to the
   loop a Rust iterator chain does, with no allocation per element.
   Materialising is explicit (`vec`, `into`, `set`, `zipmap`), and
   transducers (`transduce`, `into` with an `xf`) compose the same way.
3. **Persistent in the API, in place when unique.** `assoc` and `conj`
   keep the value semantics of Clojure; where the ownership checker
   proves the collection unique (types §6, the unique write) they
   update in place, so a loop of `assoc`s is Clojure's transient
   without the transient API. Whether that holds for each collection is
   a test, not a belief.
4. **Unboxed elements, good hashing.** A `(Vec i64)` stores `i64`s; the
   hash is a real one (the FNV-1a of types §2.12 is a spec text and
   slow on integers; decide a replacement before the HAMT's speed is
   judged); sorted maps and sets (a B-tree), queues, a small-vector fast
   path, `str` building without quadratic copies.
5. **Performance comes after a viable library** (owner, 2026-10-01):
   no benchmarking and no optimisation work until the library is
   usable. Terse, elegant code that performs well hinges on a good
   library that compiles well, so the work now is the library's
   *design* (rules 1 to 4: what makes `(->> v (map f) (filter p))`
   compile to a plain loop is the shape of the protocols and
   adaptors, decided here). When it is viable, a benchmark suite of
   fibber and Rust programs measures the ratio per kernel against the
   aim of within 1.5x, and each kernel above it is an open item with
   its cause.
6. **Every function has an executable test** (cases both ways, method
   rule 6; generated programs against a model, rule 5), and the
   compiler (M6) uses the library as it grows: what stage 2 needs
   (`sort`, `Map` iteration order, formatting for diagnostics) comes
   first.

Order: spec/stdlib.md (the table of names and deviations); sequences,
transducers and `Iter` fusion; maps, sets and sorted collections;
strings and formatting; then the long tail; then measurement. It
interleaves with M6: library items the compiler needs land first.


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
