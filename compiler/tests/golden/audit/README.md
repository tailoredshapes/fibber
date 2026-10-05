# Reference outputs of the Rust memory audit

For the port of `fibref`'s interpreter and heap audit to fibber (the owner has scheduled it; read the Rust from tag `seed-1`:
`git show seed-1:crates/fibref/src/heap/...`, `.../eval/...`, `git show seed-1:crates/fibc/src/trace.rs`). Not a check: nothing in
stage 2 prints these yet, so nothing compares with them. A port compares with them.

* `run.txt`: `fibref run FILE` for every `cases/ownership/*.fib` of seed-1 that is byte-identical in the tree on 2026-10-05
  (242 of 243): the `result:` line, the `audit:  clean=.. leak-cycles=.. leaks=.. errors=..` line (the audit's classes: leaks, leak
  cycles, errors) and the text of a trap, rejection or audit error, and the exit status. At most 40 lines per case.
* `itrace.txt`: `fibc itrace FILE`, the interpreter's heap trace (`A` alloc, `S` store, `D` drop, `F` free, with object ids, as
  `fibc run --trace` prints for compiled code), for the 211 of those whose trace is at most 120 lines.

Format: a line `#### PATH`, the output, a line `status N`. Recorded one case at a time under `timeout 60` and `ulimit -v 4000000`
from the Rust built from tag `seed-1` (`cargo build -p fibref -p fibc`, `LLVM_SYS_211_PREFIX=/usr/lib/llvm-21`). To regenerate, build
that Rust again (docs/rust-legacy.md) and run the two commands above over the same files; there is no stage 2 command that does it.
