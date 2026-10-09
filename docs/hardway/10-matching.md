---
examples: required
---

# Exercise 10: Handle alternatives

**Aim:** describe data with named fields and handle optional values explicitly.
Save `ex10.fib`. Predict the successful parse and the failed parse separately.

## Type and predict

```fib run
(ns main (:use fib.core))
(defstruct Order (name: str quantity: i64))

(defun quantity-or-zero (input: str) -> i64
  (match (parse-long input)
    ((some n) n)
    (nil 0)))

(defun main () -> i64
  (let ((order (Order "tea" (quantity-or-zero "12"))))
    (do
      (println (str (. order name) ": " (. order quantity)))
      (println (quantity-or-zero "twelve"))
      0)))
```

`defstruct` defines named fields; `(. order name)` reads one. `parse-long`
returns an `Option i64`: `(some n)` has a value, while `nil` has none. `match`
selects a pattern and binds its payload. Here the function deliberately turns
absence into zero; that is a policy, not proof that the input was valid.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
tea: 12
0
0
```

</details>

## Change it

1. Try `"0"`, `"-3"` and `""`. Explain the difference between a parsed zero
   and failure even though the original function returns zero for both.
2. Write `describe-quantity` returning `str`: return `"quantity: N"` for a
   parsed integer and `"invalid quantity"` otherwise. Test both branches.
3. Define `(defenum Availability (InStock quantity: i64) (SoldOut))`. Write a
   description function with a `match` branch for each variant.

## Break and repair

Misspell a field name in a copy. Observe the diagnostic and restore it. Then
change the `nil` branch of `quantity-or-zero` to a string. Explain why both
branches must satisfy the function's declared result type.

## Checkpoint

Explain why an `Option` carries more information than a fallback integer.
Draw `Order`'s fields and the two possible shapes of the parse result.
Reference: [functions and types](../tutorial/03-functions-and-types.md).

[← Transformations](09-transformations.md) · [Course](README.md) · [Ownership →](11-ownership.md)
