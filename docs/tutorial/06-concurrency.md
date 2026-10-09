---
examples: required
---

# Concurrency

`spawn` starts a native thread task; `join` waits for its result. The closure
and result must satisfy `Send`. `fork-task` instead uses the work-stealing pool;
`fib.parallel` adds scopes, ordered maps and reductions. `async` is the
stackless tier, driven by `block-on`; these tiers have different scheduling
costs. Use an `Atom` for shared updates, and a private `Cell` for local ones.
[Next: deployment](07-ship-it.md).

```fib run
(ns main)
(defun main () -> i64
  (let ((task (spawn (fn () (+ 20 22)))))
    (do (println (join task)) 0)))
```

```text out
42
0
```
