---
examples: required
---

# Concurrency and parallelism

| Tier | API | Use |
|---|---|---|
| Native thread | `spawn`, `join` | Independent or blocking tasks |
| Work-stealing pool | `fork-task`, `fib.parallel` | Chunked CPU work and nested parallel calls |
| Stackless | `async`, `await`, `block-on` | Suspended computations |

`fib.parallel` adds `with-tasks`, `fork`, `pmap`, `pfor`, `preduce` and `pscan`.
A scope waits for its children. Ordered reductions use input-derived chunks,
so changing worker count does not choose a different default reduction tree
(ADR 0021). `FIB_THREADS` sets the pool size. Mark OS-blocking sections in pool
tasks with `blocking`; cancellation is still planned. Use atoms for shared
updates. Cell captures and other non-Send data cannot cross threads. Exclusive
tiles use `fib.view.tiles`; ordinary views cannot escape their scope.

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
