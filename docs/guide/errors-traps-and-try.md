---
examples: required
---

# Errors, traps and try

Use `Result` for expected operational failures and `match`/`try-let` to handle
them. A trap covers invalid indexing, arithmetic overflow and explicit `trap`.
`fib.ex` provides `try`, `catch`, `finally`, `throw` and exception information;
without a handler the trap aborts. Out of memory, stack overflow, inability to
start a thread and a trap during an unwinding `finally` remain fatal.
Task failure is observable through `task-failure` or structured task APIs.
See [ADR 0009](../adr/0009-a-catching-program-returns-failures-next-to-values.md) and
[the exception design](../design/exceptions.md).

```fib run
(ns main (:use fib.core fib.ex))
(defun main () -> i64
  (do (try (nth [1 2 3] 10)
           (catch e -1)
           (finally (println "done")))
      0))
```

```text out
done
0
```
