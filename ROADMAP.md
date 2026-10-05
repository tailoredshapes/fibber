# Roadmap

**The goal is a self-hosting fibber:** a fibber compiler written in
fibber that compiles itself. Everything before that is a milestone, not
a destination. Work is ordered so that the bootstrap starts only on a
proven core: liar's self-hosted compiler started on an unproven one,
and every compiler bug became two.

Nothing on this page counts as done until an executable test says so
(spec/method.md).

## State of play (2026-10-05)

Stage 2, the compiler in fibber, is the compiler; the Rust tools are frozen as the seed and legacy oracles. `scripts/gate.sh --full` is the
judge of main, and every merge since 2026-10-04 passed it (fixed point, 354 ownership and 27 module cases, the stdlib suite with only the
recorded expected failure, case 1707). Streams since the bootstrap:

- **Performance**: the shootout against Java, Clojure and C ([docs/shootout.md](docs/shootout.md)); ranked remaining gaps in
  [docs/shootout/improvements.md](docs/shootout/improvements.md) (k-nucleotide, binary-trees, n-body, pidigits are still 2.6 to 4.2x Java).
- **SIMD and tensors**: lane vectors (`<<..>>` literals, checked integer arithmetic, masks, reductions, `fma`/`muladd`, `has-fma`),
  `fib.tensor` with a packed fma GEMM and fused dense layers ([docs/shootout/simd.md](docs/shootout/simd.md),
  [docs/shootout/tensor.md](docs/shootout/tensor.md)). Matrix multiplication is within about 1.15x of NumPy with OpenBLAS (single thread).
- **Exclusive views**: scoped writable windows with ownership rules and disjoint parallel tiles
  ([docs/design/exclusive-views.md](docs/design/exclusive-views.md)).
- **Development loop**: `fibc run -O 0` uses the fast code generator; the compile server, REPL and editor services of
  [docs/design/dev-loop.md](docs/design/dev-loop.md) are designed, not built (DV2 onward). The interpreter design
  (docs/design/fibber-interpreter.md) is superseded: no rule 6 oracle.
- **aarch64**: cross-emission and a native, self-hosting compiler on an Apple M1 Ultra; macOS packaging works; no darwin-arm64 release yet;
  iOS, aarch64 Linux, the NEON GEMM tile and the aarch64 CPU rows are open ([docs/design/aarch64.md](docs/design/aarch64.md)).
- **Libraries beyond the core**: `fib.os`, `fib.http`, `fib.logic` and `fib.logic.fd`, `fib.lacinia`, `fib.regex` (regex literals compile at compile time).

**Open work, in the order I would take it:** reverse-mode autodiff on `fib.tensor` (design not written); the darwin-arm64 release; the
remaining dev-loop packages (DV2 to DV10); the shootout gaps; last-axis reductions in `fib.tensor`; the port of meshql to fibber (deferred by
the owner); `scripts/package.sh` and CI do not yet run the `muladd.sh` check; the Rust retirement (stage 10).

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
- [x] the runtime `fib.rt` as lIR source (`rt`): header,
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
- [x] the C interface to `lair` (spec/compiler.md §9): 23 `lair_*`
      functions in `liblair.so` (`nm -D --defined-only
      target/debug/liblair.so | grep -c ' T lair_'` prints 23; retired
      by `native.*` at the flip of the lair-in-fibber line below, kept
      as a legacy oracle until stage 10),
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
- [x] types (`compiler/types/`, tool `compiler/types.fib`, 8,200 fibber
      lines for 11,000 Rust): `--stage infer --tables` equals `fibref types`
      (spec/bootstrap.md §6) on every corpus program that needs no macro run,
      the library included; the Rust side of the oracle is `fibref types`.
- [x] ownership (`compiler/own/`, tools `compiler/own.fib` and
      `compiler/explain.fib`): the body, facts, summary, taken, error and
      explain sections equal `fibref own` and `fibref explain` (spec §7) on
      the corpus. Not done: the port of the 46 Rust whole-pass tests and
      the mutation reviews (O9).
- [x] the lIR emitter (`compiler/emit/`, tool `compiler/emit.fib`): every
      section of `fibc emit-dump` (spec §8) equals the Rust: runtime,
      externs, types, statics, keywords, defs (constant `def`s through the
      JIT), quotes, function bodies, main, the macro modules.
- [x] stage 2b, macros through lair (`compiler/macros/runner.fib`, the
      `RunnerMaker` of `compiler/expand/maker.fib`): the runner is the
      default of every tool; `--no-runner` keeps the old behaviour.
- [x] the driver (`compiler/fibc.fib`, `compiler/driver/`): `emit`, `build`,
      `run`, `explain`, `emit-dump`, `-I` and `FIB_LIB`; `build -L` writes
      the rpath; a closed pipe is no failure. `cases`, `gen` and `itrace`
      stay in Rust (the harness).
- [x] **stage 2 and stage 3** (2026-10-02, re-checked 2026-10-03): `fibc
      build compiler/fibc.fib` makes stage 2; stage 2 builds stage 3; `fibc`,
      stage 2 and stage 3 emit the same 16,550,159 bytes of lIR for
      `compiler/fibc.fib` (equal md5); stage 3 emits the same lIR as `fibc`
      on the 1,116 programs of cases/ownership, cases/modules and
      cases/stdlib (with the support modules beside them), and stage 3's
      `run`, `explain` and built executables equal stage 1's on the 243
      ownership programs.
- [ ] the last clause of the definition by the Rust harness: `fibc cases`
      has no hook to use another compiler; today it is shown by output
      equality.
- [ ] 8 reflection-error inputs (`compiler/tests/expand/porter2-073..080`)
      differ between the JIT runner and the interpreter in the position of
      the error (spec/compiler.md §6 says they must be equal): package S4;
      then `compiler/expand/stage` moves from 2a to 2b.

**Method since 2026-10-02.** M6 resumed with M7's tools (`try-let`,
destructuring, flat `cond`, `defrecord`, `reduce`, `sort`): the ports are
idiomatic, about 0.7 times the Rust's lines. Each pass has a dump command
in Rust as its oracle (`fibref types|own`, `fibc emit-dump`), a harness that
compares it with the fibber tool, `--sections` so a package is judged alone,
and a differential test over the corpus. **Rust is frozen (owner, 2026-10-03).** The Rust `fibc` and the front end it
shares with `fibref` are the seed: tag `seed-1` is the last Rust-first
commit. New language and library work starts in `compiler/` (fibber); the
mirror rule is retired, because mirroring every Rust change cost more than
the work it copied. What the Rust has and stage 2 lacks (S5's `sys-*`
builtins, X9b's clause fixes, E1's editor commands, X9c once designed) is
ordinary backlog in `compiler/mirror-pending/`, ported when something needs
it. Consequence: a feature that exists only in stage 2 is judged by its
cases and the spec, not by two independent tools (method rule 6) until a
second implementation is written. `bootstrap_expand` (`mirror` feature of
`crates/fibc`) is the old expander oracle and no longer gates.
Speed: stage 2 takes about 25 s to emit its own source against 13.8 s for
the Rust (about 5 times slower on small programs: the library is checked
again for every program). Nothing has been profiled.

`lair` (lIR to native, via LLVM) was Rust, as LLVM stays in C++. The C
interface to `lair` (spec/compiler.md §9) was a stopgap, not a design
to polish: once there is a compiler, `lair` itself is to be rewritten in
fibber (owner, 2026-10-01). Lair in fibber (docs/design/lair-in-fibber.md,
`compiler/lir`, `compiler/native`, `compiler/llvm`, the tool
`compiler/lairf.fib`): stage 7, the flip, is done when its gate says so
(scripts/gate.sh --full, the compare scripts against the Rust): the
compiler's users of lair call `native.api` and `native.call`, a stage 2
links LLVM itself (shared for development, static for the release) and
has no `liblair.so`; the Rust `crates/lair`, `crates/lir`, `liblair.so`
and the Rust `fibc` remain as oracles until stage 10, when this line and
the interface go.

## M7. A standard library as ergonomic as Clojure's, as fast as Rust's

**Aim** (owner, 2026-10-01): "steal Clojure's, or as close to it";
improve inconsistencies and un-idiomatic corners where there is a reason;
the language is to be as ergonomic as Clojure with a run-time
performance that rivals Rust. Status: **Proposed**; tranches 0 and 1 done,
tranche 2 under way (see below); the rules below are mine, for the owner
to amend.

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

**State** (2026-10-03). Tranches 0 and 1 are done and the flip is made: the
implicit library (`fib.core`, `fib.seq`, `fib.coll`, `fib.print`) is on and the
prelude's old protocols are gone. Tranche 2 (the Clojure surface), under way:
- landed in the library: the lazy adaptors and their recipes (Y2), endless
  sources (Y1), cursors and `zip` (Y3), `second` `ffirst` `product` and the
  other consumers (Y4), `Stack` `find` `merge-with` `reduce-kv` (Y5), the
  numeric conversions (Y9), the string-returning printers (Y10), `partial`
  `complement` `volatile!` `as->` `defonce` `lazy-cat` (Y6), `defrecord`
  and `with` (Y8), `vec-assoc` and `vec-pop` in O(log n) (PRE);
- landed in the language: the Rust macros `try-let` `if-some` `when-some`
  `when-first` `doseq` `for` `some` and the rest (X3), patterns in `fn`,
  `defun` and `let` (X4), flat `cond` and bracket binding forms (X2), `def`
  initialisers (X8), determined variables in impl heads (X7), the Clojure
  reader additions `~x` `#(..)` `#{..}` `7/2` (X1), the stage-1 fixes S1 to
  S3;
- checked against real Clojure 1.12 (twelve independent verifiers, 2026-10-02:
  eight defects confirmed and fixed: NaN and out-of-range conversions, `pr`
  of non-finite floats, `select-keys`; Rust-side findings queued);
- open: X5 (truthiness over `Option`, integer literals adopting floats; in
  progress), X6 (keyword calls), X9 (arity overloading) and its library
  rows (Y11), X10 (macro hygiene, `x#`), X11 (struct values), chunked lazy
  seqs (Y12), the fusion tables (X12) and the generator (X13), L17's
  default derive; then the performance tranche below.
Counts: `cases/stdlib` 912 cases, 881 pass, 31 `open-`; `cargo test`
workspace 1,913 passed; `cases/ownership` 243 and `cases/modules` 27 under
both tools. Mutation reviews run in the background and gate nothing (owner,
2026-10-02).

**Performance tranche** (owner, 2026-10-03: "spending a little time on
compilation is worth it"; specialising once the runtime use is known is
fine, and the LLVM backend is trusted). After the library is viable:
profile the compiler's own build first; then in-place update when a value
is uniquely held (refcount 1, checked at run time) for `assoc` `conj`
`pop`; fusion of lazy chains that are only reduced; the library checked
once per process instead of per program; a benchmark suite against Rust
(rule 5). Nothing here starts before it is measured.

Order: spec/stdlib.md (the table of names and deviations); sequences,
transducers and `Iter` fusion; maps, sets and sorted collections;
strings and formatting; then the long tail; then measurement. It
interleaves with M6: library items the compiler needs land first.


## Performance

PERF0 (2026-10-03): measurement only; nothing below is changed yet. The suite is `scripts/bench/` (one fibber
program per benchmark, each printing a checksum; Rust twins in `scripts/bench/rust/`, outside the cargo
workspace; `FIBC=bin/fibc scripts/bench/run.sh [-n RUNS] [name..]` builds, times, compares checksums and prints
the table). Measured with the released fibc 0.1.0 (stage 2, `bin/fibc` layout) against `rustc -O` twins, median
of 3, on the shared 28-core host, one program at a time. Every benchmark printed the same line under fibc and
under its Rust twin (column `check`). Rust is built plain `-O` (integer overflow wraps there, traps in fibber; no
benchmark overflows). No Clojure column: no Clojure 1.12 jar was on disk (the lead's scratchpad copy is gone).

| benchmark | fibber run s | max RSS MB | Rust run s | ratio | what it does |
|---|---|---|---|---|---|
| num-i64 | 2.97 | 1.5 | 2.87 | 1.0 | loop/recur, 1e9 steps |
| num-f64 | 1.50 | 1.5 | 1.51 | 0.9 | f64 series, 2e9 steps |
| recursion | 2.32 | 1.5 | 1.36 | 1.7 | fib 44 + arity-overloaded tail call |
| binary-trees | 2.81 | 97 | 2.55 | 1.1 | depth 18, enum tree |
| dispatch | 2.66 | 1.5 | 0.39 | 6.8 | protocol, enum match, closures |
| vec-index | 0.83 | 2.8 | 0.08 | 10.3 | 1e8 `nth` |
| num-nbody | 1.38 | 1.5 | 0.10 | 13.7 | 5 structs in a Vec, `assoc` per body per step |
| strings | 1.08 | 217 | 0.09 | 11.9 | str, split, join, index-of |
| lazy-fused | 1.05 | 1.5 | 0.09 | 11.6 | fused range/map/filter/reduce, iterate/drop/take |
| map-assoc-get | 1.68 | 151 | 0.14 | 11.9 | `(Map i64 i64)`, 1e6 assoc, 2e6 get |
| vec-sort | 0.92 | 73 | 0.07 | 13.1 | sort 2e6, sort-by 1e6, comparator sort 1e6 |
| set-conj | 0.84 | 79 | 0.05 | 16.7 | 1e6 conj, 1e6 contains? |
| vec-conj-pop | 1.64 | 208 | 0.06 | 27.2 | 2e7 conj, 2e7 pop |
| lazy-bound | 1.48 | 1146 | 0.03 | 49.1 | the same chain, each stage bound by `let`, 5e6 |
| vec-assoc | 3.36 | 3.4 | 0.01 | 333 | 1e7 `assoc` on a unique 1e5 Vec |
| set-disj | 3.38 | 5.0 | under 0.01 | not meaningful (about 1000+) | 2e4 conj then 1e4 disj; quadratic |

Where a Rust time is 0.01 s or less the ratio is bounded by timer resolution: read the absolute fibber time.
Compile (stage 2): `fibc build hello.fib` 0.42 s against 0.115 s for the Rust seed (3.7x); each benchmark compiles
in 0.4 to 0.7 s; `fibc emit compiler/fibc.fib` 37.5 s against 15.9 s for the Rust seed (2.4x).

Scalar code is at Rust's speed (num-i64, num-f64, recursion, binary-trees: 0.9 to 1.7): LLVM does its work and
the checked arithmetic costs nothing visible. Everything that touches a collection is 7x to 330x slower, and
the profiles say why (sampled with `scripts/bench/tools/prof.sh`, an LD_PRELOAD SIGPROF sampler, because
`perf_event_paranoid` is 4 here and perf, valgrind and hyperfine are absent; self time is exact, the
inclusive and phase columns come from a heuristic stack scan and undercount):

1. **`dissoc`/`disj` walks the whole trie twice per call** (library, `lib/prelude.fib` `dissoc`, lines 497-501:
   `(= (mnode-count r) (mnode-count root))` to learn whether the key was present). set-disj at n = 20000, 40000,
   80000 took 3.3 s, 15.0 s, 74.4 s (4.5x per doubling: quadratic); the profile of the 20000 run: 91.4% self in
   `f.fib.prelude.mnode-count.i64.bool`, 8.3% in `fib.release`. This is the known ~20 s finding (n = 220000 did
   not finish in 6 minutes here). It also costs the compiler itself: profiling `fibc emit compiler/fibc.fib`,
   `mnode-count.i64.str` is 19.3% self time (about 7 s of 37.5 s), `dissoc` 18.2% inclusive, called from
   `types.elab.finish`. Fix: `mnode-dissoc` reports whether it removed anything (or compare `cnt`). Expected: set-disj
   3.4 s to well under 0.1 s; `fibc emit` 37.5 s to about 30 s; also helps every program that removes from a map.
2. **Nothing is updated in place, so every `conj`/`assoc` copies and frees** (stdlib.md §2.5 says "today it never
   is"; the checker/runtime layer: `compiler/own` to prove uniqueness or a run-time refcount-1 test in
   `rt/*.lir`, then the library's `array-with`/`array-push`/`vnode-assoc`/`mnode-assoc` take the in-place path).
   Evidence: vec-assoc profile: `vnode-assoc` 27.5% self, `fib.release` 27.1%, `drop.11` 17.3%, `__libc_free` 6.1%,
   `malloc` 5.1% (about 80% of the time is path copying and its release); vec-conj-pop: `fib.array-alloc`
   59.7% and `fib.array-slice` 34.3% inclusive (the 32-slot tail is copied on every `conj`), `fib.release` 44.2%
   inclusive; set-conj: `mnode-assoc` 33.3% self, `fib.release` 81.7% inclusive (every superseded trie path is
   freed); num-nbody: `fib.array-slice` 50.7% and `fib.release` 47.4% inclusive for five-element `assoc`s. Affects
   vec-assoc (333x), vec-conj-pop (27x), set-conj (17x), nbody (14x), map-assoc-get (12x), vec-sort (13x,
   whose `rnd` builds by `conj`) and the compiler's own build. Expected: 5x to 30x on these, to within about 2x
   of Rust (the Rust Vec is the same algorithm without the copy). The largest payoff available after item 1.
3. **A seq bound to a name costs a cell per element** (library `lib/fib/seq` + the expander's fuse rule,
   `compiler/expand`). lazy-bound is 49x Rust and 1146 MB at 5e6 elements (230 bytes per element across three
   stages); profile: `fib.alloc` 93.7% inclusive, `lmap` 64.7%, `lfilter` 48.3%, `range-lseq` 32.1%, `malloc` +
   `free` 24.5% self, `fib.release` 53.3%. The same chain consumed in place (lazy-fused) runs 1.05 s for 100e6 + 2e7
   elements with 1.5 MB: fusion works, 11.6x of the Rust iterator chain, which is the cost of the closure calls
   and checked ops, not of allocation. Fix options: fuse a `let`-bound seq whose only use is a consumer (an
   expander change, no semantic change when the binding is used once), chunk the lazy node (32 elements per
   cell), or free consumed cells as `reduce` walks (the head is not held when the binding is dead after the call).
   Expected: lazy-bound 49x down to about 12x (the fused figure) or better.
4. **Allocator and refcount traffic is a flat 25% to 50% tax on everything** (emitter/runtime: `rt/*.lir`;
   LLVM-level: lair). Self time of `malloc` + `__libc_free` + `fib.release` + `fib.drop-one` + `fib.alloc`
   across profiles: vec-conj-pop 50%, vec-assoc 43%, nbody 43%, lazy-bound 42%, `fibc build hello.fib` 33%,
   set-conj 26% (plus `drop.9` 10.9%). A size-class free list or bump arena in `fib.alloc`, an inlined
   non-atomic (when single-threaded) fast path in `fib.release`, and `drop.N` specialised per node type would
   cut it; binary-trees already runs at 1.1x of Rust's `Box`, so the gain is bounded by Rust's own malloc cost
   there, but the collection benchmarks pay it several times per operation. Expected: 1.3x to 1.8x on
   allocation-heavy programs; do after items 2 and 3, which remove most of the traffic instead.
5. **Compile time: 2.4x to 3.7x slower than the Rust seed.** `fibc build hello.fib` (380 samples = 0.38 s CPU):
   types 24.5%, own 20.3%, expand 13.7% (phase, innermost pass-module frame), liblair only 7.4% self: the front
   end re-checks the implicit library for every program, and its own allocation traffic (`fib.release` 12.4%,
   `malloc`+free 16%, `vec-conj`/`vec-collect` 16% and 8% inclusive) is the next biggest cost. `fibc emit
   compiler/fibc.fib` (38,055 samples): `liblair.so` 33.0% self (called from `emit.defs.jit.emit-def` and the macro
   JIT `macros.runner.build-module`: LLVM compiling each macro module), `mnode-count` 19.3% (item 1), libc 16.9%.
   Fix: item 1 first (a one-line-class change); then check the library once per process or cache its checked form
   (compiler/driver + types/own, the owner's "library checked once" line above); then JIT macro modules at -O0 or
   cache them (compiler/macros + lair). Expected: item 1 alone 37.5 s to about 30 s; library caching hello 0.42 s
   to about 0.15 s.
6. **Smaller items** (not in the top five): dispatch 6.8x (protocol calls and closures: the `f.twice` closure
   chain and the `Box`ed closure environments, check inlining of `m.` method symbols and closure calls in lair;
   not profiled), vec-index 10.3x (a trie walk per `nth` with a bounds check and `Option` tail test; the Rust Vec is
   flat; a flat-array Vec up to 32 slots, or unrolled shift-5 levels, would help), strings 11.9x (unprofiled; str
   concat in a loop copies: in place when unique, as item 2), `sqrt` is not landed (nbody had to avoid it).

Order of work, cheapest first: (1) the dissoc count (a library edit, one case), (2) in-place update, (3) seq
bindings, (5) library check once, (4) allocator. Each is a proposal; each needs a before/after from
`scripts/bench/run.sh` and its case before it counts.

### Where the performance cycle stands (2026-10-04)

Batch 1 landed: the `dissoc` count (`bea0905`), the size-class allocator and inlinable retain/release (`0f280bf`), the
library checked only where reachable (lever B, `99c054b`), the cycle's tools (`8827908`), and the design of in-place update
(`0633f62`, docs/design/in-place-update.md).
Batch 2 landed: the primitives `array-take!`, `array-push!`, `array-pop!`, `cell-update!` and the `ROOMY` flag (`ca8553e`),
the last-use analysis in `compiler/own` (`b016d22`, `71faf5e`), the persistence, audit and count cases 4000 to 4016 (`647b9ba`),
`update!` (`28d715c`, `17473a1`), `Vec` over the primitives (`f6a113b`, L3) and `Map`/`Set` (`a50cb1d`, L4). L8 is in progress
(its work is not in this tree). L5 (shell reuse) is not in this tree. The count cases 4010 to 4015 and 4054 carry `open:`
headers; no run is recorded in this document, and the table above is still the PERF0 one.

### The performance cycle

Builds and case runs are the cost of every lever (stage 2 build 40 s, `emit` 30 s, stdlib cases 11 min one at a time), so the
cycle is shaped to pay each of them once for many changes (scripts/gate.sh, scripts/bench/quick.sh, scripts/batch.sh):

1. Each agent takes one lever, in its own worktree, on files no other lever touches. It runs only the targeted cases
   (`F cases DIR --only PREFIX -j 12`) and the quick bench, commits, and does not run the full gate.
2. The lead runs `scripts/batch.sh BRANCH..` on a batch of branches: the commits are cherry-picked in order onto a scratch branch of
   main, one gate and one bench compare run, and a failing gate is bisected by halving the batch to name the offending branch.
3. Conflicts between levers show at the cherry-pick; the lead resolves them at integration, not the agents in advance.
4. Benchmarks run alone: gate, batch and bench share /tmp/fibsuite.lock.

Where the time goes (measured 2026-10-03, 28 cores, `compiler/fibc.fib` = 17.7 MB of lIR): `fibc build` by the Rust seed (a debug
build) 42.5 s = Rust emit 15.8 s + lair 26.7 s; `build` by stage 2 56.5 s = stage 2 emit 30.7 s + lair 25.8 s. lair's share is
LLVM's: -O2 26 s, -O1 22 s, -O0 about 2 s (`F build -O 0` 32.7 s, but the F it makes emits in 44.5 s, not 30.7 s). A program's
front end re-checks the whole library each time: `F emit hello.fib` 0.4 s, and 0.4 s is most of an average case's 0.67 s (642 s
for 963 cases); the library is checked once per case, 963 times per stdlib run. Cheapest cuts, in order, proposals only:
(1) cache the checked library (types, ownership, lowered lIR) keyed by the stamp of lib/, so a program's front end handles its own
module: hello 0.4 s to about 0.15 s (the "library check once" item above; the estimate is that item's, not measured here); (2) `-O 0` for
builds whose product the cycle only runs once (a lever's check build; `--full` keeps -O 2): the build 56 to 33 s; (3) emit
per module in parallel, the library's modules first (28 cores idle during a 30 s single-threaded emit); (4) one process, many
cases: the harness compiles each case in its own process; a process that keeps the library's checked form and the JIT warm saves
the start and the library, which (1) already gets.

PERFB-B (2026-10-04), lever B done as a cheaper cut than (1): `emit`, `run` and `build` infer and analyse only the library units the program can
reach (`types.infer.demand`: roots are the program's own modules and every library `def`; `defun`s by reference, impl methods by protocol
method index and, for a type of a library module other than the prelude, only once a constructor of it is reachable; library macros are not
checked, the macro runner checks its own). `explain` and `emit-dump` still check everything. Measured on the same host: `F emit hello.fib`
0.356 s to 0.185 s; units checked for hello 815 to 100; a stdlib case (10 cases, one at a time) 0.56 s to 0.37 s; `F emit` of 1233 programs
(ownership, modules, every stdlib case) byte-identical to before, and `explain` of the same 1233; `emit` of compiler/fibc.fib identical and
the fixed point holds. What is left of a case's front end (hello, 0.185 s) is mostly reading, expanding and lowering the library; the next
cut is there (a cached expanded and lowered library, or lowering bodies lazily), not in checking.

## Releases

Binary releases of `fibc` on GitHub (README.md, Install; `scripts/package.sh`,
`.github/workflows/release.yml`). The Rust tools are the 0.0.x line (tag
`seed-1`, released as v0.0.1); the bootstrapped compiler starts at
0.1.0 and stays 0.x.

**The process.**

1. `VERSION` and `compiler/driver/version.fib` hold the same version
   (`scripts/check-version.sh` fails otherwise; `package.sh` runs it).
2. Tag `vX.Y.Z` at a commit whose `VERSION` is `X.Y.Z` and push the tag. For v0.1 and later the
   release workflow refuses a tag that is not `v` + `VERSION`.
3. The workflow fetches the **seed** that the file `SEED` names (`scripts/fetch-seed.sh`: a release
   tarball, its sha256 verified) and runs `scripts/package.sh` with it: the seed builds stage 2 (F), F builds
   F3, and the stage check is the fixed point (F and F3 emit the same lIR for `compiler/fibc.fib`); the
   shipped `fibc` is built by F3. The unpacked tarball must run `hello.fib` under `env -i` before the
   release is created.
4. `package.sh` builds for `FIB_TARGET_CPU=x86-64-v2` unless told otherwise (README, Install), and, when
   `objdump` is present, fails if the shipped `fibc` uses `ymm` or `zmm` registers.
5. The seed moves forward deliberately: a commit changes `SEED` to a newer release's url and sha256
   (v0.1.5 is the `SEED` now; it was v0.1.3 until the flip, commit `0a86093`).

**Lessons: a seed is only usable if it runs on every CPU.** v0.1.2 died with SIGILL on a CPU other than the CI
runner's (code generated for the build host), so v0.1.3 is built for a baseline CPU; v0.1.1 failed the unpack check.

**The bootstrap chain.** `seed-1` (the Rust tools, a tag) builds v0.0.1 (the Rust tools, released; the first
`SEED`), which builds v0.1.0, v0.1.1, v0.1.2 and v0.1.3 (stage 2, the compiler in fibber: the `SEED` file at each
of those four tags names v0.0.1). v0.1.3 is the current `SEED` (commit `0a86093`): the first release that knows the
in-place primitives (spec/types.md section 2.13.1) and is built for a baseline CPU.

- [x] `fibc --version`; the relocatable tarball runs `hello.fib` with an
      empty environment (`scripts/package.sh`)
- [x] v0.0.1 released and its sha256 recorded in `SEED`
- [x] v0.1.3 released and recorded as the `SEED`
- [ ] 1.0.0 readiness: the owner's call, not decided by any test

## Decisions

- **Macros at compile time: JIT, with phase separation** (owner,
  2026-09-27; syntax §3.16). Like Clojure, there is no macro
  interpreter: macros are compiled through lIR and run with its JIT,
  one execution path for everything. Unlike Clojure's AOT, compiling a
  module runs nothing of that module: only macro definitions and what
  they reach in already-compiled required modules run at expansion
  time.

## Open decisions

None that block work. Waiting on the owner: publishing a darwin-arm64 release; when to start the meshql port; whether to build reverse-mode autodiff next.

Older note: none. The last two (the colour argument inside an `impl` on a
colour-parameterised type, and a forwarded `&` cell written through a
closure passed to the same call) were decided by the owner on
2026-09-28 (types §10).
