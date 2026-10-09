---
examples: required
---

# Exercise 9: Transform a collection

**Aim:** map, select and reduce values while keeping intermediate results
visible. Save `ex09.fib`.

## Type and predict

```fib run
(ns main (:use fib.core fib.seq fib.coll))
(defun main () -> i64
  (let ((numbers [1 2 3 4])
        (squares (mapv (fn (x: i64) (* x x)) numbers))
        (large (filterv (fn (x: i64) (> x 4)) squares))
        (total (reduce + 0 large)))
    (do
      (println numbers)
      (println squares)
      (println large)
      (println total)
      0)))
```

`fn` creates an unnamed function. `mapv` returns a vector with a result for each
input; `filterv` returns a vector of inputs satisfying the predicate. `reduce`
combines values, starting with the explicit initial value `0`. The `v` matters:
`map` returns a sequence, which is a different type from a vector.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
[1 2 3 4]
[1 4 9 16]
[9 16]
25
0
```

</details>

## Change it

1. Change the threshold to `100`. Predict the empty selection and its total.
2. Filter the original numbers before squaring. Explain how “square then select
   values above four” differs from “select inputs above four then square.”
3. Replace the anonymous squaring function with a named `square` function.
   Require unchanged output.

## Break and repair

Change the reduction's initial value to `10`. It runs but produces the wrong
answer for a sum. Test both a nonempty and an empty selection, then restore zero.
This is a behaviour mistake; a compiler cannot infer the sum you intended.

## Checkpoint

Give the type and element count of each intermediate value. Explain why an
empty reduction needs an initial value. Reference: [collections](../tutorial/02-values-and-collections.md).

[← Loops](08-loops.md) · [Course](README.md) · [Alternatives →](10-matching.md)
