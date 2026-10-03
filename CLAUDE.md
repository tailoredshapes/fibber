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
| `spec/` | the specification; its executable form is `fibref` |
| `cases/` | test programs with verdicts fixed in their headers: `ownership/` (`fibref cases`, and `fibc cases` interpreted and compiled), `modules/` (programs of several modules), `stdlib/` (the library's cases), `lir/` (`lair cases`) |
| `crates/fibref` | the reference interpreter and memory audit |
| `crates/fibgen` | the random program generator (method rule 5) |
| `crates/lir` | lIR's reader, AST and whole-module checker; no LLVM |
| `crates/lair` | lIR to native through LLVM 21: JIT, AOT, the lIR case harness; also a `cdylib`, `liblair.so`, with the C interface in `include/lair.h` (spec/compiler.md §9). Needs `LLVM_SYS_211_PREFIX` |
| `crates/fibc` | the compiler in Rust (stage 1): `fibref`'s front end lowered to lIR through `lair`, the runtime `fib.rt` in `rt/*.lir`, the rule-6 harness (`fibc cases`, `fibc gen`); `tests/bootstrap` compares stage 2's reader with the Rust one |
| `lib/` | `prelude.fib` and the implicit library `fib/` (facades `fib.core fib.seq fib.coll fib.print` and their parts): M7, design in `spec/stdlib.md` |
| `compiler/` | the compiler in fibber (M6, spec/bootstrap.md), bootstrapped: `syntax/` reader, `expand/` expander, `macros/` macro runner, `types/` type checker, `own/` ownership checker, `emit/` lIR emitter, `driver/` commands, `lair/` bindings of lair's C interface, tools `fibc.fib read.fib expand.fib types.fib own.fib explain.fib emit.fib`, `tests/` edge inputs, golden programs and compare scripts, `mirror-pending/` what the ports still owe the Rust |
| `editors/vscode/` | the VS Code language pack for `.fib`: a TextMate grammar, language configuration and snippets; no build step |
| `scripts/` | release engineering: `package.sh` (the relocatable tarball of stage 2), `package-rust.sh`, `fetch-seed.sh` (the seed named by `SEED`), `check-version.sh` (`VERSION` against `compiler/driver/version.fib`); `.github/workflows/release.yml` runs them on a `v*` tag. See README.md, Install |
| `lir-audit/` | findings from auditing liar's lIR, each re-established as a case in `cases/lir/audit` |

## Rust standards

- Edition 2021. `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings` and `cargo test` must all pass before every commit.
- Files under 500 lines, functions under 50 lines; split by
  responsibility, not by section comments.
- No thread-local or global mutable state. Passes take and return
  data; all context is an explicit parameter.
- Unit tests live next to the code. Every audit check has a test that
  shows it firing.
- No `unwrap`/`expect` outside tests unless a comment says why it
  cannot fail.

## Commits

- One logical change per commit; the message says what and why.
- Never commit build output. Never rewrite pushed history.

## `compiler/` is where the work is

- The Rust `fibc` and the front end shared with `fibref` are frozen as the
  seed (tag `seed-1`); do not add language features there. New language and
  library work starts in `compiler/` and the library in `lib/`. The mirror
  rule is retired; `compiler/mirror-pending/` is backlog of what the Rust
  has and stage 2 lacks.
- Build stage 2 with the seed: `fibc build compiler/fibc.fib -I compiler -I
  lib -L target/debug -l lair -o F`; check it with `F emit compiler/fibc.fib`
  equal to `fibc emit compiler/fibc.fib`, then F builds F3 and F3 emits the
  same again, while the seed still reads the program.
- Each pass has a Rust dump as its oracle for the language the seed knows
  (`fibref read|expand|types|own`, `fibc emit-dump`) and a compare script
  under `compiler/tests/`. Rebuild the binaries after any edit of
  `lib/prelude.fib` (it is embedded).
- Fibber source follows the same limits as Rust where it can (files under
  500 lines, functions under 50) and uses the library's own tools: flat
  `cond`, `try-let`, `if-some`, destructuring, `defrecord`.
