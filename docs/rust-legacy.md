# The Rust, retired

On 2026-10-05 the Rust went from `main` (package RR1, ROADMAP stage 10): `crates/{fibref,fibgen,fibmut,fibc,lair,lir}`, `Cargo.toml`
and `Cargo.lock`, the Rust CI jobs, and the scripts that needed them. It is all in git history. This file says how to get it back, what
each tool gave, what replaced it and what was lost.

## Getting it back

* **The seed.** Tag `seed-1` is the Rust as the bootstrap's first seed: `crates/` at the language the seed knows, with the `lib/`,
  `spec/`, `cases/` and `compiler/` of its own day. `git worktree add ~/.cache/fibber-scratch/rust-legacy seed-1`.
* **Later Rust.** The last commit with `crates/` in the tree is the parent of the commit "delete crates/, Cargo files, the Rust CI jobs
  and the scripts that needed them" (RR1). It differs from `seed-1` in `crates/lair` (`dump-ast`, JIT pruning, the C interface's
  extra functions) and in `crates/fibc/rt` (the runtime source; since RR1 it lives in `rt/`, and that commit's `compile.rs` reads
  `../../../rt/`). The goldens of `lair-*` were recorded from this later lair, the rest from `seed-1`.
* **Build.** Rust 1.97.1 (pinned in the old `ci.yml`), LLVM 21: `LLVM_SYS_211_PREFIX=/usr/lib/llvm-21` (Ubuntu) or `/usr/lib/llvm21`
  (Arch). Keep the target directory out of the tree and the job small:
  `ulimit -v 12000000; CARGO_TARGET_DIR=~/.cache/fibber-scratch/rust-legacy/target nice -n 10 cargo build -j 4 -p fibref -p fibc -p lair -p fibgen`.
  `fibref` embeds `lib/` at build time (`crates/fibref/build.rs`): run it with `FIB_LIB` unset, or it reads the current
  library, which it cannot (`unbound name fib.prelude/cell-update!`: the in-place primitives exist in the compiler in fibber only).
  A tree at HEAD needs the data of the seed's day to be judged by the Rust: `git checkout seed-1 -- cases lib spec compiler`
  (what the deleted `scripts/ci-seed-overlay.sh` did, with `crates/fibref/tests/stdlib_table/rows.rs`).
* **The Rust CI jobs** are in `git show 02b4657:.gitlab-ci.yml` (`rust-seed`, `lairf`) and `git show 02b4657:.github/workflows/ci.yml`
  (`fibref`, `lair`, `lairf`). `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test -p fibref -p fibgen -p lir`, `cargo test -p lair`, `cargo test -p fibc -- --test-threads=2`.
* **Run one program at a time.** A `fibgen` process reached 37 GB resident on 2026-10-05 and the machine's OOM killer took the session
  with it. Never more than a few generator jobs, `ulimit -v`, `timeout 60` per input.

## What each tool gave

Lines are Rust lines at `02b4657`.

| Tool | Size | What it gave | Where it lives now |
|------|------|--------------|--------------------|
| `fibref` | 375 files, 74.9k lines | **(1)** the reference interpreter and the memory audit (`src/eval/`, `src/heap/`; `fibref run`, `fibref cases`; method rules 1, 2, 3, 6): result plus `audit: clean leak-cycles leaks errors`, the free trace; **(2)** the front end and its dumps (`src/syntax`, `expand`, `modules`, `types`, `own`, `*_dump`; `fibref read\|expand\|types\|own\|explain`): the oracle of every pass of `compiler/`; **(3)** the editor side (`src/editor/`: `complete`, `diagnostics`, `lsp`) | (2) is ported (`compiler/syntax expand types own`) and its dumps are goldens; **(1) and (3) are not ported: scheduled, not dropped** |
| `fibgen` | 63 files, 13.2k lines | the random program generator (method rule 5): `gen/` builds well-typed programs from a seed, `model/` computes the value `main` must return independently of the interpreter, `run.rs` runs one through `fibref` with a time limit and classifies the outcome (leak, audit error, wrong value, reject), `shrink.rs` minimises a finding, `report.rs` writes it as a case, `coverage/` counts the constructs a run exercised, `pipe`/`macros` generate pipelines and macro uses; driver `fibc gen` | **not ported: scheduled, not dropped** (follow-up F2) |
| `fibmut` | 20 files, 3.6k lines | mutation review of one library module: mutants of its source, each judged by the cases that exercise it (spec/bootstrap.md section 3); never gating | not ported; the `scripts/mutant-*.sh` are the hand-made kind and stay |
| `fibc` (Rust) | 118 files, 24.5k lines | stage 1: the front end lowered to lIR, `fibc run\|build\|emit\|emit-dump\|explain\|itrace\|gen\|cases`, the rule-6 harness (interpreted against compiled, results and audits equal), `tests/bootstrap` (reader of stage 2 against the Rust reader), `tests/capi` (the C interface from fibber) | stage 2 is the compiler; `emit-dump` is goldens; `itrace`, `gen` and the interpreted half of rule 6 went |
| `lair` | 73 files, 9.5k lines | lIR to native through LLVM 21 (JIT, AOT, case harness `lair cases`), `liblair.so` and `include/lair.h` | `compiler/native` and `lairf`; its dumps are goldens |
| `lir` | 33 files, 4.6k lines | lIR reader, AST and whole-module checker, no LLVM | `compiler/lir` |

Seed-1 paths for the two ports: `git show seed-1:crates/fibgen/src/lib.rs` (and `src/gen`, `src/model`, `src/run.rs`, `src/shrink.rs`,
`src/report.rs`, `src/coverage`, `src/driver.rs`, `src/main.rs`); `git show seed-1:crates/fibref/src/eval/...`,
`.../src/heap/...`, `.../src/editor/...`, `.../src/main.rs` for the commands (`cases`, `run`, `complete`, `diagnostics`, `lsp`);
`git show seed-1:crates/fibc/src/trace.rs` and `.../src/harness/` for `itrace` and the rule-6 harness. Reference outputs:
`compiler/tests/golden/audit/` (the audit and the trace on 242 ownership cases) and `compiler/tests/golden/lsp/`
(`diagnostics` and `complete`).

## What replaced the compare scripts

Every script that ran a stage 2 pass against a Rust dump, with what was done. `golden` = a suite of `compiler/tests/golden`
(README there), recorded from the Rust oracle once, for the inputs on which stage 2 and the Rust agreed byte for byte; `golden.sh
--update` regenerates from stage 2 alone.

| Script (all removed) | Choice | What now / why |
|----------------------|--------|----------------|
| `crates/fibc/tests/bootstrap.rs` (reader of stage 2 against the Rust, dump and `--print`, every `.fib` under `cases/ lib/ compiler/`) | (a) | `reader-dump`, `reader-print` over `compiler/tests/reader/*.fib`: 1184 of 1187 inputs each (SHA-256 per input; the text is 100 MB). The reader also reads the whole compiler in the fixed point |
| `compiler/tests/expand/compare.sh` | (a) | `expand-e1` 91/97, `expand-r` 160/166, `expand-porter` 944/949 (SHA-256) |
| `compiler/tests/types/compare.sh`, `top-check.sh`, `lower-body-gen.py` | (a) | `types-infer` 54/55, `types-top-sections` 34/35, `types-top-ast` 29/35. The static `lower-cases/ lower-body-cases/ protos-cases/*.out` (made by the generator from the Rust) stay, unchecked as before |
| `compiler/tests/own/compare-bodies.sh`, `records.sh` | (a) | `own-amp` 6/7, `own-taken` 2/6 (`--sections taken,error`: the dumps carry ids that follow the prelude), `explain-amp` 7/7 |
| `compiler/tests/own/golden.sh` | (a) kept | Rust-recorded `golden/*.txt`: 5 still equal stage 2's; the other 10 had gone stale (prelude ids, the last-use pass that `fibref own` lacks) and were re-recorded from stage 2: no Rust confirmation. Script takes only TOOL now |
| `compiler/tests/emit/{builtins,call,causes,ctrl,defs,macros,objects}.sh` (`emit-dump` against the tool) | (a) for fns and defs, (b) for the rest | `emit-fns` 105/135, `emit-defs` 3/8. The `--layout` and `--macro` judges and the marks of `defs.sh` are gone: every program of `cases/` compiles and runs under the gate, and the fixed point compiles the compiler |
| `compiler/tests/emit/resume.sh` + `resume-oracle.rs` (rustc on copies of `ir.rs value.rs resume.rs`) | (a) | `emit/resume.golden`: 8 cases, 7196 bytes, equal; `unit-resume.fib` now checks with `native.api` instead of `lair.jit` |
| `compiler/tests/native/compare-{ast,check,llvm}.sh` | (a) | `lair-ast` 384/475, `lair-check` 360/475, `lair-llvm` 326/441 over `cases/lir/*/*.lir` (+ `native/h2-edge`) with `FIB_TARGET_CPU=x86-64-v2` |
| `compare-{emit,obj,exe,cases}.sh`, `f-compare.sh` (fuzz seeds), `h-cli.sh`, `h2-edge.sh`, `lib.sh`, `drivers/*.fib` | (b) | machine code, objects, executables and mutants of lair against lairf: what they judged is what the fixed point does every gate (stage 2 builds itself through lairf's pipeline, and the case suites run through it); `lairf cases cases/lir` still runs the lIR cases by hand |
| `native/a-cc-args.sh`, `a-rpath.sh` | (b) | the `cc` command line and rpath of `lair build` against `lairf build`; `scripts/package.sh` builds the shipped binary with the rpath-dropping `cc` shim and checks it (no RUNPATH, `ldd`), and `driver/cli.sh` checks `-L`/`-l` and the rpath on a shared library it builds with `cc` |
| `native/{j-call,j-cases,j-demo,c-call,c-call-orc,c-jit-demo}.sh`, `c-*.fib`, `c-jit-demo.hooks.golden` | (b) | tests of the C interface `liblair.so` and of `lair.jit`, which went with the crate (`compiler/lair/` and `jit-demo.fib` are unreferenced legacy now: follow-up F6) |
| `driver/compare.sh`, `build-check.sh` (stage 1 against stage 2: emit, explain, run, build, heap trace) | (b) | covered by the cases (compiled and run, `allocs:` bounds from the `FIB_TRACE=1` trace) and the fixed point |
| `driver/cli.sh` | kept, Rust-free | the `STAGE1` comparison dropped; the `-l lair` checks use a shared library it builds |
| `scripts/ci-lairf.sh`, `lair-corpus.sh` | (b) | their consumers are gone |

Totals: 4873 recorded inputs in 16 suites (2368 reader, 1195 expand, 117 types, 8 own, 7 explain, 108 emit, 1070 lair), 392 inputs
left out because the two disagreed (in `RECORDED.txt`), plus the goldens above. Each suite can fail: planted faults in the reader's dump,
the printer, the expander, the type checker's dump and the ownership checker's error text each failed their suite, and a corrupted line
failed each of the nine other suites tried (see the RR1 commit of the goldens).

## What was lost

* **Cross-checking against an independent implementation.** The goldens were equal to the Rust on one day; after an intended change
  they are stage 2's own. A bug that stage 2 and its own earlier output share is not found by them.
* **The language the Rust knew only.** Inputs where the Rust and stage 2 differ are checked only against stage 2 (the left-out lists).
* **Method rules 1, 2, 5 and 6 have no executable form in the tree.** The reference interpreter, the memory audit, the generator and the
  interpreted-against-compiled comparison were the Rust. `spec/method.md` still states them; the disagreement is recorded here and in
  the follow-ups, and the text of the rules was not changed. The nearest checks now: `allocs:` bounds and the heap trace of compiled
  programs (`FIB_TRACE=1`), `fibc cases`, the gate.
* **`covers:` header check.** `crates/fibref/tests/stdlib_table` failed a delivered row of `spec/stdlib.md` that no case covered, a
  name that is no row, or a name a case never calls. Nothing replaces it yet (F4).
* **Generated programs** (`fibc gen`, `fibgen`): no random-program testing until the port (F2).
* **The editor's language server** (`fibref lsp`): see `editors/vscode/README.md`; the grammar and snippets are unaffected.
* **`fibmut`** mutation review of library modules.
* Rust unit tests of the front end that had no stage 2 counterpart (type-checker, ownership checker and interpreter unit tests in
  `crates/fibref/src/**`): their intent is in the cases and the goldens, not their text.

## Follow-ups (scheduled, not dropped)

* **F1 `fibref` in fibber:** the interpreter and heap audit (unported parts of `fibref`: `eval/`, `heap/`), the editor commands.
  Reads the Rust from `seed-1`; references: `compiler/tests/golden/audit/`. Restores method rules 1, 2, 6.
* **F2 `fibgen` in fibber:** the generator (method rule 5), same sources, in a separate package. Keep the memory discipline above.
* **F3 `fibc lsp`:** docs/design/dev-loop.md DV8; references `compiler/tests/golden/lsp/`; then the extension starts it again.
* **F4** a fibber check of the `covers:` headers against the table of `spec/stdlib.md`.
* **F5** `fibmut`, if the mutation reviews are wanted again.
* **F6** delete `compiler/lair/`, `compiler/jit-demo.fib` and the `lair.fibm`/`lair.expand` users once nothing builds against them.
* **F7** `h-cases/` (the fixtures of the case harness of `lairf`) have no check since `compare-cases.sh` went; make them a golden suite.

## Wall time and size

Measured 2026-10-05 on the 28-core machine (other agents' jobs running), `ulimit -v 16000000`, `nice`:

| What | Time | Disk |
|------|------|------|
| `cargo build --workspace` (dev profile, opt-level 1), cold | 13.7 s | 1.7 GB target |
| the same, warm / after touching one file | 0.03 s / 0.33 s | |
| `cargo test --workspace --no-run`, cold | 28.4 s | |
| `cargo test -j4 -p lair -p fibc --no-run`, cold | 43.6 s | 4.4 GB |
| `cargo clippy --workspace --all-targets --all-features -D warnings` (after the test build) | 8.3 s | |
| `cargo test -p fibref -p fibgen -p lir` | 10.9 s; 5 fibgen tests FAILED at HEAD (they judge the Rust against the data of its own day: the old `ci-seed-overlay.sh`) | |
| `cargo test -j4 -p lair` | 23.6 s, all passed | |
| all of the above in one target directory | | 7.1 GB |
| `cargo test -p fibc` (the rule-6 harness, interpreted and compiled) | not measured; the CI limit was 45 min | |

The full gate never ran cargo: stage 2 is built by the seed in `SEED`, so the gate's own time did not change except for the golden
step this change adds. Full gate on the committed tree with no cargo on `PATH`, no `crates/`, a fresh scratch and `-j 8`: PASS in
492 s of its own time (build 66 s, fixed point 80 s, golden 124 s with the 7 tools built in parallel and 16 suites, cases 222 s:
ownership 354 pass, modules 27 pass, stdlib 1229 pass, 21 open, 1 expected failure, case 1707); 883 s of wall time with the wait
for `/tmp/fibsuite.lock`. `scripts/package.sh` with the seed and static LLVM: 291 s, the tarball holds `bin/fibc`, `share/fibber/lib`,
`LICENSE`, `README.txt` and no `bin/fibref`; the unpacked `fibc run hello.fib` works with `env -i`. What the Rust cost was in
the CI jobs that built it (the `lair` job links LLVM statically through `llvm-sys`; limits of 90 and 45 minutes), in a target
directory per worktree, and in every agent that had to keep a Rust build current for the compare scripts.
Checkout: 7371 files, 18.4 MB of tracked files before; 6673 files, 16.7 MB after (the Rust: 699 files, 4.7 MB; the goldens add 2.9 MB).
