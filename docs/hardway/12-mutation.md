---
examples: required
---

# Exercise 12: Change a local value

**Aim:** distinguish persistent values from local mutable places. Save
`ex12.fib`. This exercise uses a private `Cell`, not a shared-thread counter.

## Type and predict

```fib run
(ns main (:use fib.core fib.coll))
(defun add-one (&items)
  (append &items 1))

(defun main () -> i64
  (let ((items (cell [3 4])))
    (do
      (add-one &items)
      (println @items)
      (add-one &items)
      (println @items)
      0)))
```

`cell` creates a local mutable holder. `@items` reads its current value.
`&items` passes the place in-out so the callee can change it. This is different
from exercise 6's `conj`, which returned a new persistent vector.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
[3 4 1]
[3 4 1 1]
0
```

</details>

## Change it

1. Append `9` instead of `1`; predict both printed vectors.
2. Create a second cell. Call the function once for each and print both.
3. Explain which function signature tells you that calling it can change the
   caller's place. Compare it with a persistent-update function's signature.

## Break and repair

Save the following as `ex12-broken.fib`. Predict whether it is rejected before
running it. Passing the same place to two in-out parameters in one call is forbidden.

```fib reject "passed to more than one & parameter"
(defun add-to-both (&a &b)
  (append &a 1)
  (append &b 2))
(defun main () -> i64
  (let ((items (cell [3 4])))
    (do (add-to-both &items &items) 0)))
```

Repair it using two distinct cells. If you intend two updates to the same cell,
perform them in separate calls instead. Explain how either repair changes the
simultaneous access that the original call requested.

## Checkpoint

Explain `items`, `@items` and `&items` separately. Cells are local; do not capture
this cell in a spawned thread. See [concurrency guidance](../guide/concurrency-and-parallelism.md)
for `Send` and atoms after finishing the course.

[← Ownership](11-ownership.md) · [Course](README.md) · [Failures →](13-failures.md)
