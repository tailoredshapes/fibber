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
| `cases/` | test programs with verdicts fixed in their headers |
| `crates/fibref` | the reference interpreter and memory audit |
| `crates/fibgen` | the random program generator (method rule 5) |
| `crates/lir` | lIR's reader, AST and whole-module checker; no LLVM |
| `crates/lair` | lIR to native through LLVM 21: JIT, AOT, the lIR case harness. Needs `LLVM_SYS_211_PREFIX` |
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
