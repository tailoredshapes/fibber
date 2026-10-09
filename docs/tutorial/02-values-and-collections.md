---
examples: required
---

# Values and collections

Numbers and strings are values. Square brackets construct a persistent `Vec`;
map literals use braces, and set literals use `#{...}`. Updating a collection
returns a value; an existing holder still sees its old contents. `mapv` eagerly
returns a vector, while `map` returns a sequence. A sequence and a vector have
different types; use `vec` to convert. Try expressions in `fibc repl`.
[Next: functions](03-functions-and-types.md).

```fib run
(ns main (:use fib.core fib.seq fib.coll))
(defun main () -> i64
  (let ((xs [1 2 3]) (squares (mapv (fn (x: i64) (* x x)) xs)))
    (do (println squares) (println (reduce + 0 squares)) 0)))
```

```text out
[1 4 9]
14
0
```
