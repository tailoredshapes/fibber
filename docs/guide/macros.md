---
examples: required
---

# Macros

`defmacro` receives forms at compile time. Quasiquote constructs code; `~`
unquotes a form and `~@` splices forms. Use `gensym` for generated bindings.
Expansion runs compiled macro code, including dependency macros: trust them
as build-time programs. This macro intentionally repeats its argument, so
pass a pure expression; a macro should bind effectful arguments once when
single evaluation is required. See [syntax §3.16](../../spec/syntax.md).

```fib run
(ns main)
(defmacro twice (x) `(+ ~x ~x))
(defun main () -> i64 (do (println (twice 21)) 0))
```

```text out
42
0
```
