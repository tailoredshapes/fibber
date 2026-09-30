# cases/modules

Programs of several modules (syntax §5). Each case is a directory:
its `main.fib` carries the header (method.md rule 3) and the `ns`
form whose `:require` and `:use` clauses name the other modules,
which live beside it, `a.b` at `a/b.fib`. Both harnesses run a
directory's `main.fib` as one case (`fibref cases cases/modules`,
`fibc cases cases/modules`), and every case runs interpreted and
compiled with matching results and free traces (method.md rule 6).

- 001: a `:require` under an alias and a `:use`, three modules read
  once each in dependency order (17).
- 002: a `:private` definition of a required module is not reachable by
  its qualified name (reject: `secret is private to util; it is not
  exported`).
- 003: `(var u/secret)` is the one reference that reaches it (syntax
  §3.20), beside a public function of the same module (83).
- 004: two modules requiring a third: it is one module, its struct one
  type, its `def` one value (7).

What the reference implementation does not enforce yet: macros are
visible across the modules loaded together unqualified (the expander
keeps one table), which is wider than a `:use` alone gives, and a
macro cannot be named through an alias.
