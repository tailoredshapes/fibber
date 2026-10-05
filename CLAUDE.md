# fibber

A Lisp with memory safety and no GC: borrow first, count second.
Successor to liar; see README.md. fibber does not use moth.

The goal is self-hosting (ROADMAP.md): plan work so it moves toward a
fibber compiler written in fibber, and prefer library and language work
that such a compiler needs.

## How work is judged

Read spec/method.md. In short: nothing is done until an executable
test says so; claims in docs, commit messages and chat carry no weight.

- Do not describe anything as working, passing or implemented unless
  you ran it in this session and can quote the output.
- A test that cannot fail is worse than no test. Pending is not pass.
- When a spec rule and code disagree, stop and report; do not quietly
  change either.

## Layout

| Path | What |
|------|------|
| `spec/` | the specification (its executable form, the Rust `fibref`, is retired: docs/rust-legacy.md) |
| `cases/` | test programs with verdicts fixed in their headers: `ownership/` (`fibc cases`), `modules/` (programs of several modules), `stdlib/` (the library's cases), `lir/` (`lair cases`) |
| `rt/` | the runtime `fib.rt` as lIR source (`rt/*.lir`): data that stage 2 consumes (`compiler/emit/runtime.fib` is generated from it by `compiler/tests/emit/gen-runtime.fib`; `compiler/tests/emit/runtime.sh` checks the drift) |
| `docs/rust-legacy.md` | the Rust (`crates/fibref fibgen fibmut fibc lair lir`) was retired in RR1 and lives in git (tag `seed-1`): how to get it back, what each tool gave, what was lost, the ports scheduled (`fibref`'s interpreter and audit, `fibgen`, `fibc lsp`) |
| `lib/` | `prelude.fib` and the implicit library `fib/` (facades `fib.core fib.seq fib.coll fib.print` and their parts): M7, design in `spec/stdlib.md` |
| `compiler/` | the compiler in fibber (M6, spec/bootstrap.md), bootstrapped: `syntax/` reader, `expand/` expander, `macros/` macro runner, `types/` type checker, `own/` ownership checker, `emit/` lIR emitter, `driver/` commands, `lir/` lIR reader, AST and whole-module checker, `llvm/` bindings of LLVM-C, `native/` lair (lowering, passes, JIT, AOT, the `native.api` and `native.call` that `emit.defs.jit`, `macros.runner` and `driver.native` use, the case harness `cases`, `cli`), `lairf.fib` the tool (`lairf check|run|build|emit-llvm|dump-ast|cases`; renamed `lair` when the Rust one goes), `lair/` legacy bindings of `liblair.so` (`lair.ffi` is still the byte and word helpers; `lair.jit`, `lair.call`, `lair.err` and the old `lair.fibm`/`lair.expand` serve `jit-demo.fib` only), tools `fibc.fib read.fib expand.fib types.fib own.fib explain.fib emit.fib`, `tests/` edge inputs, golden programs and compare scripts, `mirror-pending/` what the ports still owe the Rust |
| `editors/vscode/` | the VS Code language pack for `.fib`: a TextMate grammar, language configuration, snippets, and the language client for `fibc lsp` (server in `compiler/lsp/`; `test/lsp.js` fails when no server is found unless `FIBREF_SKIP=1`) |
| `scripts/` | release engineering: `package.sh` (the relocatable tarball of stage 2), `fetch-seed.sh` (the seed named by `SEED`), `check-version.sh` (`VERSION` against `compiler/driver/version.fib`); `.github/workflows/release.yml` runs them on a `v*` tag; `SEED` at the root names the release (url, sha256) that builds stage 2 in CI and for releases (v0.1.5 now; per-platform rows `url.PLATFORM=`/`sha256.PLATFORM=`). `FIB_TARGET_CPU` (read by lair) picks the CPU code is generated for: `package.sh` sets `x86-64-v2` so a release runs on any CPU; unset, code is for the host. See README.md, Install and ROADMAP.md, Releases |
| `lib/fib/tensor/` | the explicit numerical library `fib.tensor` (dense tensors, strided views, fma GEMM, fused dense layers, vector math), design in `docs/design/numerical-library.md`; `lib/fib/simd.fib` (`fib.simd`), `lib/fib/view.fib` (exclusive windows: `with-view`, `with-tiles`) |
| `docs/` | `design/` (one file per design, decisions in `decisions-2026-10-04.md`), `shootout/` (measurements with their commands: `simd.md`, `tensor.md`, `aarch64.md`), `shootout.md` |
| `lir-audit/` | findings from auditing liar's lIR, each re-established as a case in `cases/lir/audit` |

## The Rust is gone

`crates/`, `Cargo.toml` and `Cargo.lock` were removed in RR1 (ROADMAP stage 10); there is no cargo in the build, the gate or CI.
The code is in git: tag `seed-1`. docs/rust-legacy.md says how to build it again (outside the tree: `CARGO_TARGET_DIR` under
`~/.cache/fibber-scratch`, `ulimit -v`, `nice`, a few jobs), what each tool gave and what was lost. The old Rust standards
(edition 2021, `cargo fmt`, `clippy -D warnings`, files under 500 lines, no global state) apply to anyone who revives it.
Ports scheduled, not dropped: `fibref`'s interpreter and heap audit, `fibgen`, `fibc lsp`.

## Commits

- One logical change per commit; the message says what and why.
- Never commit build output. Never rewrite pushed history.

## `compiler/` is where the work is

- New language and library work starts in `compiler/` and the library in `lib/`. `compiler/mirror-pending/` is the backlog of what the
  Rust had and stage 2 lacks, kept as history: there is no Rust to mirror any more (docs/rust-legacy.md).
- Build stage 2 with the seed: `fibc build compiler/fibc.fib -I compiler -I
  lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F` (F links LLVM; `LLVM_LINK=static` in the scripts uses `scripts/llvm-static.sh`). The stage check is the fixed point: F
  builds F3, and `F emit compiler/fibc.fib` equals `F3 emit compiler/fibc.fib`.
  F's emit equals the seed's only while `lib/prelude.fib` is the one the
  seed embedded (a seed built before a prelude change emits the old one), so
  that comparison is a note, not a gate.
- The in-place primitives `array-take!`, `array-push!`, `array-pop!` and `cell-update!` (spec/types.md §2.13.1) exist in the compiler in
  fibber only; `lib/prelude.fib` calls them.
- Each pass has golden outputs under `compiler/tests/golden/` (recorded once from the Rust oracles, since stage 2's own; README there):
  `compiler/tests/golden/golden.sh --fibc F` checks them, `--update` regenerates them from stage 2 after an intended change (read the
  diff). `scripts/gate.sh --full` runs them. Rebuild the tools after any edit of `lib/prelude.fib` (it is embedded).
- Fibber source follows the same limits as Rust where it can (files under
  500 lines, functions under 50) and uses the library's own tools: flat
  `cond`, `try-let`, `if-some`, destructuring, `defrecord`.

- **The self-hosting trap.** A new core form or builtin name (`splat`, `native-lanes` so far) must not equal the name of a function the
  compiler itself defines: once stage 2 compiles the compiler, the form captures the function and the fixed point breaks (F builds F3 but
  they disagree, or F3 fails to build). Grep `compiler/` for the name first; the full gate catches it, the quick gate may not.
- `--target TRIPLE` / `FIB_TARGET_TRIPLE` cross-compiles (objects and assembly; linking another target's executable is refused). A Mac
  (Apple Silicon, `llvm@21` from Homebrew) is the real-hardware test for aarch64: `scripts/mac-check.sh`.
- Vector fma: `simd/fma` is exact (a libm call per lane without hardware FMA); `simd/muladd` is fused where the target has FMA; library code
  chooses with `(has-fma)`, never with the lane count.

## Performance cycle

Make many changes, then build and test once. The tools are shell scripts over stage 2 (`F` below).

- `F cases DIR [--only PREFIX..] -j N` runs N cases at a time (default 1); the rows and counts are the same, in the same order.
- `scripts/gate.sh [--quick|--full]`: builds stage 2 (cached while compiler/ and lib/ are unchanged), the fixed point (full), the case
  directories with `-j`, compares the non-passing set with `scripts/ci-stage2.expected`; one timing line per stage and PASS or FAIL.
  `--quick` is ownership, modules and about 100 stdlib cases; `--full` adds the fixed point and the golden checks.
- `scripts/bench/quick.sh [--record]`: 10 benchmarks of 1-2 s, median of 3, against `scripts/bench/baseline.tsv`; flags a delta over 10%
  and a changed checksum (a wrong answer is a failure, not a speedup).
- `scripts/batch.sh BRANCH..`: the lead's integrator (scratch branch from main, cherry-picks, one gate, one bench, halving on failure).
- An agent working a lever uses one worktree and its own files, runs targeted cases and the quick bench, commits, and does NOT run
  the full gate; the lead batches branches through `scripts/batch.sh`. Gate, batch and bench hold `/tmp/fibsuite.lock`, so a benchmark
  never overlaps a run; no more than about 12 heavy jobs at once.
