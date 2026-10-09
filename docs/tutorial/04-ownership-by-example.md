---
examples: required
---

# Ownership by example

The compiler borrows a value while a call reads it and counts it when it
escapes. Calling `size` does not prevent the caller from using the string again.
Save this example and run `fibc explain FILE` to inspect ownership decisions.
`Cell` supplies local mutation and `&` passes a cell in-out; cells cannot cross
threads. The [ownership guide](../guide/ownership-and-borrowing.md) includes a
checked rejection. [Next: a project](05-a-project.md).

```fib run
(ns main)
(defun size (s: str) -> i64 (str-len s))
(defun main () -> i64
  (let ((s "hello") (n (size s))) (do (println n) (println s) 0)))
```

```text out
5
hello
0
```
