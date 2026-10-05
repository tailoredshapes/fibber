# fibgen in fibber: the port plan (package G0)

Status: plan and contracts, 2026-10-05 (G0). **The source of truth for the shared types is the code**: `compiler/gen/` holds every record and
variant as a real definition and every function as a stub whose body is `(trap "todo: MODULE FUNCTION")`, each with a comment naming the Rust
file and function it ports (the pattern of `docs/design/lair-interfaces.md`). `compiler/tests/gen/skeleton.sh` builds a program that requires every module and
must keep printing `ok`. Everything below that says "measured" was run in this session and its output is quoted; nothing in section 5 is implemented yet.
The Rust is at tag `seed-1` (`git show seed-1:crates/fibgen/src/...`, 63 files, 13,344 lines in `crates/fibgen`).

## 1. What fibgen is

Method rule 5: a generator makes random well-typed programs; every one the checker accepts must pass the memory audit and compute what an
independent model says. Four parts, all in `crates/fibgen/src`:

| Part | Rust | Lines | What it does |
|---|---|---|---|
| tree | `ast.rs`, `ty.rs`, `macros.rs`, `pipe/mod.rs` | ~1,300 | the generated program as a typed tree: every node has a type (`Ty`, 24 variants), so a node can be replaced by a smaller one of its type |
| generator | `gen/*.rs` (27 files) | ~5,900 | `generate(seed, size)`: a type-directed grammar: `expr(ctx, ty, depth)` picks by weight among variable, let, if, do, match, loop, helper call, type-specific production, join, await; productions per type (scalar, objects, funcs, tasks, protocols, hooks/jobs, vector patterns, `&` parameters, numeric widths, arrays), "gadgets" (templates for ownership situations scope alone does not decide), and a second generator for library pipelines (`gen/pipelines.rs`, `lambdas.rs`) |
| model | `model/*.rs` | ~1,800 | an independent value interpreter over the tree: strict left to right, copy-in/copy-out `&`, shared cells, lazy tasks, checked arithmetic at every width, IEEE floats, protocol dispatch, vector patterns and guards, and Clojure-lazy-seq pipelines; no counts, no heap. It gives the result `main` must return or the trap message it must trap with, or `Err(Unsupported)`/`Err(Budget)` (a "gap": counted, not run) |
| printer | `print/*.rs` | ~780 | the tree as fibber source: an 80-column layout rule, a fixed preamble of 13 declaration blocks plus 7 macros chosen by which names the body mentions |
| run, classify | `run.rs`, `driver.rs`, `main.rs`, `report.rs` | ~1,000 | runs each program in `fibref` with a time limit, classifies (ok, expected trap, audit failure, result mismatch, run failed, panic, hang, over-rejection, unsupported, model gap), groups by `(class, normalised key)`, and `main.rs` offers `run`, `show`, `emit` |
| shrink | `shrink.rs`, `pipe/shrink.rs` | ~500 | greedy delta debugging over the typed tree: replace a node by the smallest value of its type or one of its children, drop a let binding, do step or vector element, prune unreachable items; a pipeline loses a stage, a terminal or its shape |
| coverage | `coverage/*.rs` | ~600 | labels per construct and a count table. **Reporting only**: nothing feeds back into generation, so it is outside the determinism contract |

`fibc gen` (`crates/fibc/src/harness/gen.rs`) is the same generator with the rule-6 harness: it writes each program as a case whose header is the
model's verdict (`;; expect: accept`, `;; result: N`, `;; audit:  clean`, or `;; expect: trap` and `;; trap:   MSG`) and runs it interpreted and
compiled. Today `compiler/fibc.fib` refuses `gen` with status 2 and stage 2 has no interpreter (`docs/design/dev-loop.md`: the interpreter is a
development tool, rule 6 is dropped), so the port's run half is **the compiled run only**, through the harness `F cases` already has.

Generation does not read the model, the interpreter or the coverage: `generate(seed, size)` is a pure function of two integers. That is what makes the
determinism contract below checkable.

## 2. What the fibber toolchain already gives (checked)

| Need | Where | Evidence |
|---|---|---|
| run a program and its memory audit, judge it against a header | `F cases DIR [--only ..] -j N` (`compiler/driver/harness.fib`, `verdict.fib`, `header.fib`), the audit of the compiled runtime (`FIB_TRACE`) | measured below |
| the Rust-generated programs are valid today | 150 programs of the corpus as cases (`corpus-run.sh`, section 3.3) | 150 pass; pipelines 99 of 100 pass |
| wrapping 64-bit arithmetic, logical shift | `unchecked-add`, `unchecked-multiply`, `shr`, `shl`, `bit-xor`; **only in a stage 2 built from the tree**: the v0.1.5 seed rejects them (`unbound name unchecked-add`) | `compiler/gen/rng.fib` builds with `~/.cache/fibber-scratch/rr1/tools/F`, not with the seed |
| no unsigned type | `u64 % n` is `rng-umod` (section 4.1) | tested against 7 seeds |
| a reader and printer | `compiler/syntax/` reads and prints `Stx`. **Not used**: the generator prints its own layout byte for byte (section 4.5) and never reads | |
| `str` building, `Vec`, `Cell`, records, enums, `defstruct` with function-typed fields, mutual recursion of records inside a module | the compiler's own modules | `skeleton.sh` prints `ok` |
| no name collisions with the compiler | a copy of `compiler/fibc.fib` that requires all 41 `gen.*` modules and refers to one function of each | builds: `fibc-collide.fib`, section 3.5 |

Not available and not needed: the interpreter `fibref` (frozen at seed-1, cannot run the current library), `fibc`'s Rust test harness, threads for the
generator (the harness gives the parallelism).

## 3. The determinism contract and the golden corpus

### 3.1 Is bit-for-bit equality with the Rust generator required?

Required for the port's acceptance, not for the project's use of it. Reasons to demand it:

1. The Rust generator is the only oracle for the port. A port that generates *different but plausible* programs cannot be told from a port with a bug in a
   rarely drawn branch; equal output per seed turns every divergence into a located diff (first differing line of seed N).
2. The corpus of found cases (`cases/found/generated`) cites seeds (`fibgen show --seed S --size K`): those citations stay meaningful.
3. It is cheap: the draw order is fixed by the code, the RNG is 10 lines, and there is no hash iteration order in generation (checked: `HashMap`/`HashSet`
   occur only in `model/`, `shrink.rs` and the preamble's token set, all lookups, never iterated into output).

After the port is accepted and the Rust is deleted, new generator features legitimately change the output; the manifest then freezes as "the
seed-1 generator" and a new epoch starts. So the contract is: **same seed and size give the same program text and the same model verdict as the
seed-1 Rust, for every row of the manifests, until the owner decides to move on.**

Consequences the port must honour (each is a way to differ by one draw):

- **The order of draws is the order of the Rust code**, including draws whose value is unused and draws inside `loop { .. }` rejection samplers
  (`gen-any-type`, `gen-send-type`); `rng-chance` always draws; `rng-weighted` does not draw when the total is 0; `rng-pick-index` does not draw on an
  empty collection.
- **Evaluation order of Rust function arguments is left to right**, and so is fibber's (`spec/syntax.md` section 2): a call like `f(g.small(), g.small())` ports as is.
- `Gen::fresh` numbers names from 1 in one counter for the whole program; `universe()` has 42 entries in a fixed order.
- Floats: the generator makes `k/4` for integer `k` in `-10..=10`, `k/2`, and the specials; all are exact in binary, so the printed `{:?}` text is the
  shortest-round-trip text of a dyadic rational (`1.5`, `-0.25`, `3.0`; f32 literals print with an `f32` suffix). The port needs a printer for dyadic
  rationals only, not Ryu.
- `size` is `max 1`; `run` assigns sizes `1 + (i mod 6)` to program `i` (seed `S + i`).

### 3.2 The corpus, how it was recorded, and what it holds

Recorded **now** from a Rust build of seed-1 (`git archive seed-1` into `~/.cache/fibber-scratch/gen-port/seed1`, `CARGO_TARGET_DIR=~/.cache/fibber-scratch/gen-port/target`,
`cargo build --release -p fibgen`, 10 s; nothing under the tree), by `compiler/tests/golden/gen/record.sh`, one seed per process under `ulimit -v 6000000` and
`timeout 60`, in batches of 250 seeds. Size of seed `s` is `1 + ((s-1) mod 6)`, as `fibgen run --seed 1` assigns.

| File (all under `compiler/tests/golden/gen/`) | Rows | Bytes |
|---|---|---|
| `manifest-programs.tsv`: `seed size bytes sha256-16 model` for kind `programs` | seeds 1..2000, 1,999 rows (1409 absent, 3.4) | 70,086 |
| `manifest-pipelines.tsv`: the same for kind `pipelines` | seeds 1..2000, 2,000 rows | 80,735 |
| `programs.txt.gz`: the full text of seeds 1..300 (`;;;; seed N size K kind programs` before each) | 300 programs, 1.0 MB raw | 310,084 |
| `pipelines.txt.gz`: the full text of seeds 1..150 | 150 programs | 11,799 |
| `rng.tsv`: SplitMix64 outputs for 7 seeds (`next`, `below 7`, `below 1000003`, `range -3 9`, `chance 60`, `weighted [3 0 5 1]`); computed by `mkrng.py` from the algorithm, not by the Rust | 7 rows | 1.5 KB |
| `blowups.tsv` | 1 row | 0.2 KB |

Total about 470 KB. The hash is the first 16 hex digits of the sha256 of the program text (without the `;; model:` line); the model column is the
Rust `Debug` text of `Result<i64, ModelError>` (`Ok(5)`, `Err(Trap("integer overflow in + at i64"))`). In the manifest of programs 13 verdicts are traps; in the
manifest of pipelines 183 (`nth: index out of range` 113, `reduce: empty collection` 70).

Measured: the recorder is deterministic (seeds 1..120 recorded twice give identical manifests: `DETERMINISTIC`), and `compare.sh` accepts the Rust binary on
60 + 60 rows and **rejects a planted fault** (a wrapper that flips the seed for multiples of 10: seeds 10, 20, 30, 40 reported with different bytes, hash and
model). The text of the 450 full programs is what `compare.sh` cannot show (it shows a hash): a package that differs on seed N extracts N from
the `.txt.gz` and diffs.

### 3.3 Two checks that need no part of the port (they run now)

- `compiler/tests/gen/compare.sh GEN KIND FROM TO` (the port's acceptance gate): `GEN show --seed S --size K --kind KIND` against the manifest. Self-test with the Rust
  binary: `ok 60 rows` (programs), `ok 60 rows` (pipelines), `ok 7 rows` (seeds 1405..1412, which skips 1409).
- `compiler/tests/gen/corpus-run.sh F KIND FROM TO JOBS`: the Rust-generated programs of the corpus as cases (header from the manifest's model column) through `F cases`.
  Measured with `~/.cache/fibber-scratch/rr1/tools/F`: programs 1..150: all pass (22.8 s wall, 2 jobs); pipelines 1..100: 99 pass and
  **one failure**, `gen-00000071-5.fib  FAIL  expected accept, but rejected: ...:12:516: unbound name sq3`. That is a finding about stage 2 or the library (a `let` that binds
  `sq1 sq2 sq3` and reads `sq3` in the same form), not about the port; it is reported, not fixed here. Reproduce:
  `corpus-run.sh F pipelines 71 71`.
- `compiler/tests/gen/rng-check.sh`: SplitMix64 in fibber against `rng.tsv`. Measured `ok`; with `(shr z1 27)` changed to `28` it fails and prints the diff.

### 3.4 The memory finding: seed 1409 of kind programs

The session that recorded the first corpus was killed by the kernel: one Rust `fibgen show` process grew to about 37 GB. Cause, measured afterwards one seed at a
time under `ulimit -v 6000000` and `timeout 60`:

```
$ fibgen show --seed 1409 --size 5        (rc 134)
(8,941 bytes of program text are printed, then)
memory allocation of 2324522952 bytes failed
```

**Generation is bounded** (the program prints in full: 8,941 bytes, hash `2ecdbd8bfa3f8c6f`, in `blowups.tsv`); the **model has no allocation bound**. It has a step budget
(`ModelError::Budget`) but one step can ask for a 2.3 GB vector; without a limit the process keeps growing. No other seed of the 4,000 recorded blew up (one seed at a time,
same limits). The cause inside the model (the program is mutual recursion `muta122`/`mutb123` over loops that build vectors by `conj`) is **not diagnosed**: that is the
job of package M0.

Contract for the port: (a) the generated text for 1409 must equal the recorded hash (the generator is the contract); (b) the model must be bounded in memory as well as in
steps: a counter of cells and vector elements made (`FgMachine.cells-made`), `MeBudget` past a limit (decision: 4 million objects), so that `show --seed 1409` prints
the program and `;; model: Err(Budget)` instead of dying; (c) every tool that runs seeds in a loop (record, compare, `fibgen run`) runs each seed under `ulimit -v` and
`timeout`, as `record.sh` and `compare.sh` do; the project rule (never thousands of seeds, at most 8 jobs) stands. Seed 1409 stays out of the corpus manifests.

## 4. Shared types and the decisions that shape them

### 4.1 RNG: the exact algorithm, written (not stubbed)

`compiler/gen/rng.fib` is real code: SplitMix64 (`state += 0x9E3779B97F4A7C15; z = (z ^ z>>30) * 0xBF58476D1CE4E5B9; z = (z ^ z>>27) * 0x94D049BB133111EB; z ^ z>>31`),
state held as the i64 with the same bits (`unchecked-add`, `unchecked-multiply`, logical `shr`). The one unsigned operation, `u64 % n`, is `rng-umod`: for a negative
`z` it computes `q = ((z >>> 1) quot n) << 1`, `r = z - q*n` (wrapping), and subtracts `n` once if `r >= n` (valid for `0 < n < 2^62`; every use of `below` has `n` far below that).
`below`, `range` (`lo + below(max(hi-lo+1, 1))`), `chance` (`below(100) < pct`), `weighted`, `pick-index` follow `rng.rs`. Verified against the Python of the same algorithm for 7 seeds including `-1` and `-2^63`
(the negative-state path); the Rust itself is checked indirectly, by every manifest row.

### 4.2 Names and shapes

- Every type is `Fg...`; every variant carries a short prefix of its enum (`Y` types, `Nm` numeric types, `Pr` protocols, `Ek` expression kinds, `Gp` patterns, `Rs` rest, `Ar` call arguments,
  `Mc` macros, `Sr`/`Sg`/`Op`/`Tm`/`Sh` pipeline source/stage/operand/term/shape, `Vk`/`Rg` variable kind/region, `Vl` model values, `Me` model errors, `Ts` task state, `Cl` classes, `Ob` observations,
  `Gk` generator kinds, `Cm` commands). Type and variant names are global across the modules a program links (the lair-interfaces rule): the copy of `fibc.fib` that requires all `gen.*` modules builds.
- Functions are `STEM-NAME` in kebab case (`scalar-int`, `hooks-hook`) so that none meets a builtin (`int`, `list`, `join`, `apply`, `call` are all names the Rust uses).
- `Box<T>` is a plain field; a Rust tuple is a record (`FgPatVars`, `FgDefs`, `FgBind`, `FgNameInit`, `FgNameTy`, `FgArm`, `FgClause`); `Option<NumTy>` is `(Option FgNum)` (nil is `i64`).
- Modules: `gen.ty gen.ctx gen.ast gen.rng gen.state gen.core` and the productions `gen.<stem>`; `gen.model.*`, `gen.print`, `gen.run`, `gen.shrink`, `gen.coverage`, `gen.report`,
  `gen.driver`, `gen.cli`; the tool is `compiler/fibgen.fib`. Files stay under 500 lines (the largest stub is 111).
- `pipe/mod.rs` is in `gen.ast`: `Kind` holds a `Pipe` and a `Pipe` holds `Expr`s, and modules cannot be cyclic. `vpat2`/`protos2` are merged into `vpat`/`protos` for the same reason (they call each other).

### 4.3 The generator state

`FgGen` is a record whose mutable parts are Cells (rng, fresh counter, helper functions made, node count, defs, impls, `methods-ok`), the Rust `Gen`. `FgCtx` is immutable (the Rust clones it).
No global mutable state.

### 4.4 The cycle problem and the hook table

The production modules of the Rust call each other in cycles (`control` and `vpat`, `effects` and `inout` and `funcs`, `objects` and `funcs`, `observe` and `hooks` and `jobs`...) and all of them call
`Gen::expr`, which calls them. Fibber modules cannot be cyclic. Measured (a dependency graph of the 24 production files from their `stem::` references, `layers.py`): a layering of the 24 files
has **13 back-edges**, which are 14 functions. They, and `expr`, `leaf`, `leaf_value`, are fields of `FgHooks` in `gen.state`, function values that `core-hooks` in `gen.core` fills in; a production calls
`(. (. g hooks) FIELD)` through `gen-expr` and friends. The 13 back-edges: `consts -> scalar`, `control -> vpat`, `effects -> inout`, `funcs -> inout`, `helpers -> funcs`, `hooks -> observe`, `mcalls -> effects`,
`objects -> funcs`, `objects -> vpat`, `observe -> jobs`, `protos -> protos2` (merged), `tasks -> funcs`, `vpat -> vpat2` (merged). A package that finds a call that is not covered adds a field to `FgHooks` and
a line to `core-hooks`, and tells the caller of the wave (the only shared edit allowed).

### 4.5 Printing

The printer is its own: an `FgSexp` (atom, list, vector, prefix) laid out by `print-layout` (one line when it fits in 80 columns, else the head kept (`defun` 5, `impl` 3, `let loop plet match fn if` 2, else 1) and
one item per line at indent + 2). `syntax.print` prints `Stx` and is not the same layout. The preamble (13 blocks, selected by the tokens of the printed body split on whitespace and `()[]@&`, in table order)
and the 7 macro definitions are text constants of the printer.

### 4.6 The model

`FgV` replaces `Rc`/`RefCell` with fibber objects and Cells; `FgEnv` is a persistent linked list. The model returns `(Result i64 FgModelError)`; the machine counts steps and objects made.
The model's trap messages are part of the contract (the harness checks `trap: MSG`).

### 4.7 What the stubs do not fix

Closure-typed helpers of the Rust that take `impl FnOnce` (`protos2::spawned`) and `vars_where(pred)` are private helpers: their packages choose the fibber shape. Types private to a Rust file
(`Lens`, `Site`, `Wrapper` in `vpat`; `PGen` is in `gen.lambdas`) are declared by the owning package in its own file, not shared.

## 5. Packages

Sizes: the project's 0.7 port ratio on the Rust lines (tests included in the Rust count, ported with the code). Total Rust 13,344 -> about 9,300 fibber lines; G0 is done (about 930 lines of types, stubs and tests).

| Pkg | Files (under `compiler/`) | Rust lines | Fibber lines (0.7) | Depends on | Tests that can fail |
|---|---|---|---|---|---|
| **G0** (done) | `gen/*` stubs, `fibgen.fib`, `tests/gen/*`, `tests/golden/gen/*`, `gen/rng.fib` real | | 930 | | `skeleton.sh` ok; `rng-check.sh` ok and fails on a planted shift; `compare.sh` rejects a faulty generator |
| **T** types | `gen/{ty,ctx,ast}.fib` bodies | ty 330, ctx 228, ast 450, macros 161, pipe 327 | 1,000 | G0 | port of the Rust unit tests (`send_follows_types_5_1`, `cell_reach_is_conservative_for_closures`, `shadowing_replaces_the_older_binding`, `children_skip_inout_arguments`); planted: `ty-send?` of `YCell` true must fail |
| **P** printer | `gen/print.fib`, `gen/print/{items,pipelines}.fib` (new) | 784 | 550 | T | the Rust `print` tests; `compare.sh` on hand-built trees is not enough: P is verified by **G-first** below; planted: layout width 79 instead of 80 changes one corpus row |
| **M0** model core | `gen/model.fib`, `gen/model/value.fib` | eval 415, mod 104, value 80 | 420 | T | the Rust `model/tests.rs` (286 lines: later write-back wins, copy-in at entry, loop/recur, weak refs); the **1409 bound** (`show` prints `Err(Budget)` within 60 s and 6 GB); planted: copy-in at call exit instead of entry |
| **M1** model numbers | `gen/model/{nums,builtins}.fib`, arrays | 335 + 190 + 96 | 435 | T | `nums.rs` unit tests (overflow traps with the Rust texts, saturating conversions, NaN to 0); planted: `rem` of floats as `%` of ints |
| **M2** model patterns, protocols | `gen/model/patterns.fib` (+ protos) | 93 + 87 | 126 | T, M0 | `tests_new.rs` (false guards fall through, dispatch with defaults, weak/atom of dyn) |
| **M3** model pipelines | `gen/model/pipelines.fib` | 404 + 269 tests | 283 | T, M0 | the 12 Rust cases of `model/pipelines/tests.rs` (call counts per element pulled); planted: `take` pulls its source once more |
| **GP** pipeline generator | `gen/{pipelines,lambdas}.fib` | 473 | 330 | T, rng | `compare.sh GEN pipelines 1 300` (needs P, M3, M0, D) |
| **GA** scalar-ish productions | `gen/{scalar,consts,derive,nums,arrays,mcalls}.fib` | 682 | 480 | T, state | `compare.sh` on programs once the core exists; unit: `scalar-literal-text` draws match rng-check order |
| **GB** objects, funcs, tasks | `gen/{objects,funcs,helpers,tasks,observe}.fib` | 994 | 700 | T, state | as GA |
| **GC** control, patterns | `gen/{control,vpat}.fib` (vpat2 merged) | 781 | 550 | T, state | as GA; the Rust `vpat` coverage rules (`covered` redundancy) as unit tests |
| **GD** effects, `&` | `gen/{effects,inout}.fib` | 514 | 360 | T, state | as GA |
| **GE** protocols, hooks, jobs | `gen/{protos,hooks,jobs}.fib` (protos2 merged) | 731 | 510 | T, state | as GA |
| **GF** gadgets | `gen/{gadgets,gadgets2}.fib` | 757 | 530 | T, state, GB | as GA |
| **GK** core | `gen/{state,core}.fib` | mod 268 + ctx use | 250 | T, rng | `compare.sh programs 1 300` is the gate of GA..GF together; unit: `fresh`, `body_ctx`, budget behaviour (`node_budget = 60*size`) |
| **R** classify | `gen/run.fib` | 318 | 225 | M0 | the Rust `classify` tests (rejection, panic, hang, leak) as unit tests on `FgObserved`; planted: `normalise` not dropping `line:col` |
| **D** driver, report, cli, tool | `gen/{driver,report,cli}.fib`, `fibgen.fib` | 238 + 190 + 210 | 450 | all | `fibgen show` against the manifests; `fibgen emit` + `F cases` on 100 seeds; `fibgen run --count 50` prints tallies; the 1409 bound |
| **S** shrink | `gen/shrink.fib` | 500 | 350 | T | the Rust shrink tests (`shrinks_to_the_part_that_matters`, `smallest_values_evaluate`: every `smallest` type evaluates in the model); planted: a candidate of the wrong type must be rejected by the type check of the program |
| **V** coverage | `gen/coverage.fib` | 607 | 425 | T | counts for seeds 1..300 equal a table recorded from the Rust (`coverage-1-300.txt`, to be recorded by V from the seed-1 binary the same way); planted: a label dropped |

### 5.1 Order, soonest useful output, and what runs in parallel

0. **Now, no port needed**: `corpus-run.sh F programs 1 150` and `... pipelines 1 100` already run Rust-generated programs on stage 2 (section 3.3). That is rule 5 working today with the Rust generator; the port must not
   make it worse.
1. **Wave 1 (parallel, 8 agents)**: T first (it is small; every other package needs `ty-*` and `expr-*` bodies, so T lands in a day, the others start from the stubs and merge T), then at the same time P, M0, M1,
   M3, GP, GA, GK, R. M2, GB, GC, GD, GE, GF start together too; they only need T and the types, and each tests with its own unit programs. No package edits another's file.
2. **First useful generated programs: kind `pipelines`**, because that generator is self-contained (own state `FgPGen`, own rng stream, no hook table) and short: **T + GP + P (pipelines part) + M3 + the minimal M0 + the `show` part of D**, about 2,300 fibber lines. Gate: `compare.sh GEN pipelines 1 300` (and later 1 2000 in batches
   of 250). Also useful at once: the library's lazy-seq fusion is exactly what 183 of the 2,000 manifest rows stress (traps) and what the failure of seed 71 shows.
3. **Wave 2: kind `programs`**: GK joins GA..GF (all merge by one integrator because they share `FgHooks`), gate `compare.sh GEN programs 1 300`, then 1..2000 in batches of 250. A divergence is found by diffing seed N's text with the `.txt.gz` for N <= 300.
4. **Wave 3**: D (run, emit, report), R, S, V; `fibgen run --count 100 --jobs 2` on a stage 2 built from the tree; then S on a planted bug (the shrinker minimises a program that a mutated stage 2 miscompiles).
5. **Acceptance of the port**: `compare.sh` ok over both manifests (3,999 rows) in batches; `fibgen run` over 500 seeds each kind, finding set equal to `corpus-run.sh` on the same seeds (rule 5: any finding is a case); the unit tests; the size and file-limit checks; **then the owner decides** when the Rust `fibgen`, `fibc gen` and the CI lines for `-p fibgen` go (RR1 deletes `crates/`; this plan depends on none of it, only on tag seed-1 and the corpus).

### 5.2 Rules for every package

1. Own your files; keep signatures. A signature, record field or variant that must change is reported to the caller of the wave, not changed quietly (spec/bootstrap.md 5.3).
2. A package replaces the `trap` bodies of its files and adds `:private` helpers; modules of its own go under its own directory. Files under 500 lines, functions under 50, flat `cond`, `if-some`, `defrecord`.
3. Port the Rust unit tests next to the code (`compiler/tests/gen/unit-STEM.fib`) and add the planted faults of the table: a fault is a one-line change, applied by a script, that makes the test or `compare.sh` fail; the script restores it. A test that cannot fail is worse than none.
4. Never run more than a few hundred seeds in one run; each seed under `ulimit -v 6000000` and `timeout 60`; at most 2 concurrent jobs of a package, 8 gen jobs in all (the project rule after the OOM).
5. The skeleton keeps printing `ok` (`compiler/tests/gen/skeleton.sh`; build with a stage 2 from the tree).

## 6. Decisions for the owner

1. **Tool, not `fibc` command** (recommended): `compiler/fibgen.fib` is built beside `fibc` like `lairf.fib`; `fibc.fib` keeps refusing `gen` until the port is accepted, then `fibc gen` can call `gen.cli` (a module of the same program) at no cost. Reason: a separate binary keeps the compile time of `fibc` and the self-hosting fixed point unaffected while the port is built.
2. **Equality with the Rust per seed until the port is accepted, then freeze the manifests** (section 3.1): recommended; the alternative (statistical equivalence only) loses the oracle.
3. **Model object limit** 4 million (section 3.4); the Rust has none.
4. **No threads in the generator** (the harness gives `-j`): recommended.
5. The interpreter-less run: the port's classification is from the compiled run only (rule 6 dropped); an `ObCompiled` with `audit-clean` false is an audit failure. Recommended as stated.

## 7. What this document did not do

- None of the packages above is implemented, except `gen.rng` and the contracts. `compiler/tests/gen/compare.sh` was run against the Rust binary, not against any fibber output (there is none yet).
- The cause of the 1409 allocation inside the model is not found.
- Seed 71 of kind `pipelines` (stage 2 rejects `sq3`) is not diagnosed.
- The coverage table of the Rust (needed by package V) and recorded shrink results are not in the corpus.
- The stubs were built with a stage 2 from another agent's scratch (`~/.cache/fibber-scratch/rr1/tools/F`, built 2026-10-05 18:47), not with `scripts/gate.sh`; with the v0.1.5 seed alone `rng.fib` does not compile (`unchecked-add`).
