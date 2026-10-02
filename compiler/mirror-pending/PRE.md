# Mirror pending: PRE (prelude fixes of the M6 types plan and the Clojure differential check)

Not mirrored into `compiler/expand/*.fib` (mirror duty suspended, 2026-10-02). The expander
port reads `lib/prelude.fib` from disk, so the prelude's own changes need nothing ported; the
rows say what changed in the Rust expander and in the prelude, and which case or test shows it.

## The Rust expander

| Rust file, function | What it does now | Test, case |
|---|---|---|
| `crates/fibref/src/expand/mod.rs` `PRELUDE_SOURCE` | the string no longer holds `(derive Hash List)`: it is the `List` enum, `(derive Eq Option) (derive Ord Option) (derive Hash Option)` and `(derive Eq List) (derive Ord List)`; `expand_prelude` returns the `defenum` and five `impl`s, not six. `compiler/expand/program.fib` line 45 holds a copy of the string and must lose `(derive Hash List)` when the port is resumed | `prelude_expands` in `crates/fibref/src/expand/tests/derive.rs` (6 forms, no `Hash (List a)` among them); cases `ownership/199`, `stdlib/2140` |
| `prelude/text.rs` `print-str`, `pr-str`, `println-str`, `prn-str` (X3; `print-raw` is the renamed `print-str` of the library) | unchanged by this package; listed because the port reads them with the prelude: they expand over `fib.prelude/show` and `fib.prelude/str-concat`, see `X3.md` | `crates/fibref/src/expand/tests/prelude_x3.rs`, cases 1300 to 1399 |

## The prelude (`lib/prelude.fib`, read from disk by the port)

| Name | What it does now | Case |
|---|---|---|
| `vec-assoc` (public), `vnode-assoc` (private) | copies the path from the root to the leaf, or the tail alone; `i` = count appends; otherwise `assoc: index out of range`. The `Assoc` instance of `Vec` in `lib/fib/coll/seqs.fib` calls it | 2100 to 2103 |
| `vec-pop` (public), `vnode-pop`, `vnode-leaf`, `vec-pop-root` (private) | the tail without its last element, or the last leaf of the trie becomes the tail (the root loses a level when one kid is left); the pop of the only element is `VecEmpty`; empty traps `pop: empty`. The `Stack` instance of `Vec` in `lib/fib/coll/stack.fib` calls it (the helper `vec-pop-rebuilt` is deleted) | 2120 to 2122, 2566 |
| `map-empty-hashed` (public) | the empty `Map`, today `map-empty` under the name the `hash-map` macro (X3, `prelude/vector.rs` `hash_map`) expands to | 1307 |
| `impl Hash (List a)`, `list-hash` (private) | `hash-combine` folded over the elements' hashes from the seed 1, then combined with the count: the value of `hash-ordered-coll`, so a list hashes as the `Vec` of its elements. It replaces the derived instance, which folded the variant index and the fields | 199 (its pinned numbers changed, from an independent Python transcription), 2140 |
