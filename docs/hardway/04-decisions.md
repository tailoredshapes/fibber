---
examples: required
---

# Exercise 4: Make a decision

**Aim:** test a boundary and both branches of `if`. Save `ex04.fib`.

## Type and predict

```fib run
(defun main () -> i64
  (let ((temperature 18)
        (limit 20))
    (do
      (println (< temperature limit))
      (println (= temperature limit))
      (println (if (< temperature limit) "bring a coat" "leave the coat"))
      0)))
```

Comparisons produce booleans, `true` or `false`. `if` chooses one of its two
branches and produces that branch's value. Both branches here produce `str`.
It does not execute both branches and discard one answer.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
true
false
bring a coat
0
```

</details>

## Change it

1. Run temperatures `19`, `20` and `21`. Write the full expected output first.
2. Replace `<` with `<=`. Which of those three cases changes?
3. Use `(and (>= temperature 10) (< temperature 20))` as the condition.
   Test `9`, `10`, `19` and `20`.

## Break and repair

Replace the string in one branch with a number. The two result types no longer
agree. Restore a string in that branch; do not “fix” it by deleting the test.

## Checkpoint

Describe the exact interval in drill 3 in words, including both ends. Explain
why testing only temperature `18` would miss a boundary mistake.

[← Strings](03-strings.md) · [Course](README.md) · [Functions →](05-functions.md)
