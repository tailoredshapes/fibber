# X9b: what the stage-2 expander must reproduce (mirror pending)

Two Rust changes after X9 (arity overloading). Mirror duty is suspended as for X9; these are the
functions that must change when it resumes (Y11b needs them before the library gains clauses).

## 1. A `:borrow` after a parameter's type is part of that parameter

| Rust | What it does now | Shown by |
|---|---|---|
| `expand/params.rs` `split` (used by `arity` and `desugar`) | a typed parameter `x: T` is two items; when the next item is the keyword `:borrow` it is a third item of the same parameter (`Param::Plain` of three items), so `f: (fn (a) b) :borrow x: a` counts 2, not 3. Only `:borrow` is taken. | cases 1850, 1851; unit test `expand::tests::params::a_borrow_qualifier_after_the_type_is_part_of_its_parameter` |

Fibber function that must change: `compiler/expand/params.fib` (the split behind `arity` and the
pattern desugaring). Clause tables (`compiler/expand/overload.fib`, once it exists, per X9.md) take their
counts from it, so nothing else changes.

## 2. The fuser sees a clause-picked head as its base name

The fusion rewrite runs after the clause pick, so a library function with a second clause is called
`map$2`, `sort$1`. The tables key on the base name and the call's argument count.

| Rust | What it does now | Shown by |
|---|---|---|
| `expand/fuse/tables.rs` `base_head(head, argc)` (new) | `head` with a trailing `$argc` taken off the name, then `split_head`: `map$2` with 2 arguments is `(None, "map")`, `fib.seq/map$2` is `(Some(Seq), "map")`; `map$3` with 2 arguments is left as it is. | tests `tables::tests::a_clause_picked_head_is_its_base_name`, `tests::fuse::a_clause_picked_library_head_still_fuses` |
| `expand/fuse/stage.rs` `Env::library(head, argc, facade)`, `terminal_call`, `stage_call` | use `base_head`; for a bare head the free test (not bound by the form, not in the module's shadow set) is made on both the base name and the head as written, so a module that defines its own `map` clauses (`map$2` in its shadow set, from `defn`) is not the library's. A clause of another count (`map$3`) is not a stage and its terminal row is looked up by the call's count. | `tests::fuse::the_other_clause_of_a_library_head_is_not_a_stage`, `a_clause_head_of_the_modules_own_is_not_the_librarys` |
| `expand/fuse/scan.rs` `calls_a_terminal` | the prefilter uses `base_head(head, argc)` (and `saturating_sub` for the empty list). | `scan::tests::a_terminal_call_is_seen_by_name_and_arity` |

Fibber functions that must change: `compiler/expand/fusetab.fib` (add `fuse-base-head` beside
`fuse-split-head`: strip `$N` when N is the call's count, then split), `compiler/expand/fuse.fib`
(`fuse-library` gains the `argc` parameter and tests the base name and the head as written;
`fuse-terminal-call` and the stage-call check pass the count), `compiler/expand/fusescan.fib`
(`terminal-named?`). `compiler/tests/expand/compare.sh` must show the same fused dump for a module
that calls `(sort (map f v))` once the library has the clauses.

## Not solved here (expander, X9 limitation)

A local binding that is called is not told from the overloaded library function (overload.rs header).
Once `map` has a clause, `(let ((map (fn ..))) (map f v))` is picked as `map$2`: case 824 fails under
the probe for that reason, not for fusion. It needs scope tracking in the pick (or a pick that leaves a
name alone where a binder in scope has it).
