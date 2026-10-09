---
examples: required
---

# Exercise 8: Repeat with a loop

**Aim:** trace changing loop bindings and prove when the loop stops. Save
`ex08.fib`. Make a paper table with columns `n`, `acc` and the chosen branch.

## Type and predict

```fib run
(defun sum-through (limit: i64) -> i64
  (loop ((n 1) (acc 0))
    (if (> n limit)
        acc
        (recur (+ n 1) (+ acc n)))))

(defun main () -> i64
  (do
    (println (sum-through 4))
    (println (sum-through 0))
    0))
```

`loop` establishes bindings for this repetition. `recur` supplies the next
value of each binding in the same order. Its arguments use the current values:
the second argument adds the current `n`, not the next one. `recur` is in tail
position, the last action of that branch. The stopping branch returns `acc`.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
10
0
0
```

For limit `4`, the `(n, acc)` pairs are `(1, 0)`, `(2, 1)`, `(3, 3)`, `(4, 6)`
and `(5, 10)`. The last pair chooses the stopping branch.

</details>

## Change it

1. Trace limits `1` and `5` before running them. Require `1` and `15`.
2. Add a print of `n` inside a `do` before the recursive call. Predict whether
   the stopping value prints. Keep `recur` last.
3. Sum squares instead of numbers. For limit `4`, require `30`.

## Break and repair

Change the stopping test from `>` to `>=`. The program still compiles, but now
returns `6` for limit `4`. Explain the skipped value and repair the boundary.
Do not remove progress from the recursive call: reason on paper about why
repeating the same bindings forever would fail to terminate.

## Checkpoint

Trace an entire run without executing it. State what quantity moves toward the
stopping condition and what the accumulator means at the start of each iteration.

[← Maps](07-maps.md) · [Course](README.md) · [Transformations →](09-transformations.md)
