---
examples: required
---

# Functions and types

`defun` names a function. Parameters may have `name: Type` annotations and
`->` introduces a return type. `defstruct` defines named fields; `defenum`
defines alternatives that `match` handles. This example sums the fields of a
point. `Option` and `Result` are enums used throughout the library.
[Next: ownership](04-ownership-by-example.md).

```fib run
(ns main)
(defstruct Point (x: i64 y: i64))
(defenum Shape (At point: Point) (Empty))
(defun describe (s: Shape) -> i64
  (match s ((At p) (+ (. p x) (. p y))) ((Empty) 0)))
(defun main () -> i64 (do (println (describe (At (Point 3 4)))) 0))
```

```text out
7
0
```
