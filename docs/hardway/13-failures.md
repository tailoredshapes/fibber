---
examples: required
---

# Exercise 13: Keep failures visible

**Aim:** preserve an expected failure as data rather than quietly choosing a
fallback. Save `ex13.fib`. Predict both branches separately.

## Type and predict

```fib run
(ns main (:use fib.core))
(defun read-quantity (input: str) -> (Result i64 str)
  (match (parse-long input)
    ((some n) (if (>= n 0) (Ok n) (Err "quantity must be nonnegative")))
    (nil (Err "quantity must be an integer"))))

(defun report (input: str) -> str
  (match (read-quantity input)
    ((Ok n) (str "accepted: " n))
    ((Err message) (str "rejected: " message))))

(defun main () -> i64
  (do
    (println (report "12"))
    (println (report "-1"))
    (println (report "twelve"))
    0))
```

`Result` carries either an `Ok` value or an `Err` value. This is expected input
failure handled by the program. A trap, such as invalid `nth` indexing, is a
runtime failure with different control flow. Do not confuse either with a
compiler rejection, which prevents the program starting.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
accepted: 12
rejected: quantity must be nonnegative
rejected: quantity must be an integer
0
```

</details>

## Change it

1. Test `"0"` and `""`. Keep valid zero distinct from invalid input.
2. Add an upper limit of `100`. Accept `100`, reject `101`, and choose a useful
   error message. Preserve the existing negative/invalid-input errors.
3. Find `nth?` in the [library declarations](../reference/library/fib.seq.md).
   Write a small program matching present/absent results instead of trapping on
   the invalid index from exercise 6.

## Break and repair

Replace the invalid-input branch with `(Ok 0)`. The compiler accepts the
program, but `"twelve"` now reports a valid zero quantity. Record the wrong
behaviour and repair the branch. The next exercise turns this kind of policy
mistake into a test failure.

## Checkpoint

Give one example each of compile-time rejection, runtime trap and an expected
`Err` result. Explain who handles each one. Reference:
[errors, traps and try](../guide/errors-traps-and-try.md).

[← Mutation](12-mutation.md) · [Course](README.md) · [Behaviour specs →](14-specs.md)
