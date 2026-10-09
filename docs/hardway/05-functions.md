---
examples: required
---

# Exercise 5: Write a function

**Aim:** give a calculation a name and separate returning from printing. Save
`ex05.fib`. Predict what each call returns before predicting the whole output.

## Type and predict

```fib run
(defun total (price: i64 quantity: i64 delivery: i64) -> i64
  (+ (* price quantity) delivery))

(defun receipt (amount: i64) -> str
  (str "total: " amount))

(defun main () -> i64
  (do
    (println (receipt (total 7 3 4)))
    (println (receipt (total 7 0 4)))
    0))
```

Each parameter has a name and type. A function body returns its value; no
`return` keyword is needed here. Neither `total` nor `receipt` prints anything.
Main decides what to print. Keeping this boundary makes calculations testable.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
total: 25
total: 4
0
```

</details>

## Change it

1. Call `total` for three orders using the same function. Check the arithmetic.
2. Add a `discounted` function taking an amount and a discount, returning their
   difference. Compose it with `total`; `(discounted (total 7 3 4) 2)` must be `23`.
3. Call `receipt` in main without wrapping it in `println`. Predict what disappears.

## Break and repair

Call `total` with two arguments in a copy. Read the argument-count diagnostic.
Then supply three arguments, but use a string for quantity. Explain why adding
an argument repairs the first error without repairing the second.

## Checkpoint

Draw a table of each function's inputs, result type and side effects. Recreate
`total` from its input/output description without looking at the implementation.

[← Decisions](04-decisions.md) · [Course](README.md) · [Vectors →](06-vectors.md)
