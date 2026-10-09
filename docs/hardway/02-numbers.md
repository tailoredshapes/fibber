---
examples: required
---

# Exercise 2: Name a calculation

**Aim:** evaluate nested arithmetic and bind intermediate values. Save
`ex02.fib`. Write the value of each binding before running it.

## Type and predict

```fib run
(defun main () -> i64
  (let ((price 7)
        (quantity 3)
        (subtotal (* price quantity))
        (delivery 4))
    (do
      (println subtotal)
      (println (+ subtotal delivery))
      (println (- subtotal delivery))
      0)))
```

`(* price quantity)` multiplies its arguments. `let` binds names in order;
`subtotal` can use the earlier names. The binding list is separate from the body.
Integer literals here are inferred as `i64`, a signed 64-bit integer type.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
21
25
17
0
```

</details>

## Change it

1. Make quantity zero, then five. Predict all three printed values for each run.
2. Introduce a `discount` binding. Print `subtotal + delivery - discount` as a
   nested expression. For the original values and discount `2`, require `23`.
3. Compare `(* (+ 2 3) 4)` with `(+ 2 (* 3 4))`. Explain why they differ.

## Break and repair

Move the subtotal binding before price. It now refers to a name before that
local binding exists. Record the diagnostic and restore the order. In a second
copy, replace quantity with `"three"`; explain why multiplying a number and a
string is a type error rather than a request to repeat text.

## Checkpoint

Calculate the output for price `9`, quantity `2` and delivery `6` on paper.
Check it by running. Explain the scope of a name introduced by `let`.

[← Reading](01-reading.md) · [Course](README.md) · [Strings →](03-strings.md)
