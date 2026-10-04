# P2: the let rule of the fusion rewrite (stage 2 only)

`compiler/expand/fuselet.fib` (`fuse-lets`, called from `fuse-body` in `fuserun.fib` before the fusing walk) folds a `let`
binding that is read once, by the call of the next binding or by the one body form, in the collection position of a
terminal consumer or of a chain of stages, into its reader, provided every other argument of those calls is atomic
(spec/stdlib.md section 2.1, "The let rule"). The Rust expander (`crates/fibref/src/expand/fuse*`) is frozen and does not
have it, so `fibref expand` and the Rust `fibc emit` differ from stage 2 on a program with such a `let`.

| Item | |
|---|---|
| Rust function | `fuse::fuse_form` (would call a `fold_lets` before the walk) |
| Behaviour | `(let ((r (range n)) (m (map f r)) (a (reduce + 0 m))) ..)` becomes `(let ((a (reduce + 0 (map f (range n))))) ..)` |
| Cases | `cases/stdlib/4200` to `4203` (header `stage: 2`: they pass under stage 2 only) |
| Fibber function | `fuse-lets` of `compiler/expand/fuselet.fib` (the port is the other way round: Rust owes it) |
