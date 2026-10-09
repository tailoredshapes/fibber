---
examples: required
---

# Exercise 6: Keep a collection

**Aim:** index a vector and observe a persistent update. Save `ex06.fib`.

## Type and predict

```fib run
(ns main (:use fib.core fib.seq fib.coll))
(defun main () -> i64
  (let ((original [10 20 30])
        (extended (conj original 40)))
    (do
      (println (count original))
      (println (nth original 0))
      (println original)
      (println extended)
      0)))
```

Square brackets construct a vector. Indexing starts at zero. `conj` returns a
vector with the new item at the end; `original` still names its old value.
The `:use` declaration introduces the exported core/sequence/collection names.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
3
10
[10 20 30]
[10 20 30 40]
0
```

</details>

## Change it

1. Print index `2`, then print the last index as `(- (count original) 1)`.
2. Make a second extension of `extended`. Print all three vectors to show that
   each earlier holder still sees its earlier value.
3. Replace the numbers with strings and predict the displayed vector.

## Break and repair

Ask for index `3` in the original vector. It has no such element. This complete
case records the intended runtime failure:

```fib case
;; spec: docs/hardway/06-vectors.md
;; expect: trap
;; trap: nth: index out of range
(defun main () -> i64 (nth [10 20 30] 3))
```

Run a copy without those comments if you want to observe the trap directly.
Repair the index, then explain why a compiling program can still fail at runtime.

## Checkpoint

Write every valid index for a five-element vector. Explain why changing the
name `extended` cannot itself make `original` contain a fourth item.

[← Functions](05-functions.md) · [Course](README.md) · [Maps →](07-maps.md)
