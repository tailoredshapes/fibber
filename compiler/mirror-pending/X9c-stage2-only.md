# X9c: cases that only stage 2 passes (the Rust seed is frozen)

The Rust expander (`crates/fibref/src/expand/overload.rs`, header) tracks no scopes and has no value for
a bare overloaded name. Stage 2 (`compiler/expand/scope.fib`, `walk.fib`, `overload.fib`) has both, so
these cases carry `;; stage: 2` in their header. `fibref cases` and `fibc cases` report each as a header
error (`unknown header key stage`), which is expected; they are run by `F run -I cases/stdlib/support FILE`
with `F` built from `compiler/fibc.fib` and `FIB_LIB=$PWD/lib`, and must print `0` (ownership 05 prints `2`).

| Case | Shows |
|---|---|
| `cases/stdlib/1900` | a `let` name called like an overloaded function is a call of the local |
| `cases/stdlib/1901` | a `defun` and a `fn` parameter likewise, and as a value |
| `cases/stdlib/1902` | a `loop` variable and a `match` pattern variable likewise, out of scope after the form |
| `cases/stdlib/1903` | a `let` initialiser still sees the overloaded function; a later binding sees the local |
| `cases/stdlib/1904` | a bare overloaded name as a value is its clause of the fewest parameters (`pick$1`) |
| `cases/stdlib/1905` | a local passed on as a value stays the local; a `match` clause's variable ends with its clause |
| `cases/ownership/05-closures-share-state` | with `get` clauses in the library, `(let ((get (nth c 1))) (get))` is the local's |

Under the seed with the Y11 library (clauses for `get nth range ...`): ownership 05 is
`get takes 2 or 3 argument(s), got 0`. Case 874 passes under both, but its expansion differs: the seed
leaves the value `range` as the symbol, stage 2 writes `range$1`.
