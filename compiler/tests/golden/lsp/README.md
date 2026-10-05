# Reference outputs of the Rust language server's one-shot commands

For the planned `fibc lsp` (docs/design/dev-loop.md, DV8; protocol spec/bootstrap.md section 9). Not a check: stage 2 has no
such command yet. The Rust is in tag `seed-1` (`git show seed-1:crates/fibref/src/editor/...`, `.../main.rs`).

* `diagnostics.txt`: `fibref diagnostics FILE` (JSON `{"diagnostics":[{line,col,endLine,endCol,message,severity}..]}`) for the 242
  `cases/ownership/*.fib` that are byte-identical in seed-1 and the tree on 2026-10-05 (what the older language can express).
* `complete.txt`: `fibref complete FILE LINE 0` with LINE the last line of the file, for every fifth of those (48), JSON
  `{"items":[{label,kind,detail,doc}..]}`, cut at 4000 bytes.

Format: a line `#### PATH` (for `complete`, with the position), the output, a line `status N`. The Rust was run from the build of
tag `seed-1` with its own embedded library (docs/rust-legacy.md), one file at a time under `timeout 60`.
