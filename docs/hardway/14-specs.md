---
examples: required
---

# Exercise 14: Write a behaviour spec

**Aim:** turn a requirement into a check, then prove the check catches a mistake.
Save `ex14-spec.fib`. The `-spec.fib` suffix is used for test discovery.

## Type and predict

```fib case
;; spec: docs/hardway/14-specs.md
;; expect: accept
;; result: 0
;; audit: clean
(ns main (:use fib.test.core fib.test.run))
(defun total (price: i64 quantity: i64 delivery: i64) -> i64
  (+ (* price quantity) delivery))

(defspecs specs
  (feature "order totals"
    (scenario "three items plus delivery"
      (then (expect = 25 (total 7 3 4))))
    (scenario "zero items still pay delivery"
      (then (expect = 4 (total 7 0 4))))))

(defun main () -> i64 (run-main (specs)))
```

```sh
fibc test ex14-spec.fib
```

Expect two passing scenarios and no failures or pending scenarios. The reporting
layout and timings can vary; assert behaviour, not a screenshot. The expected
values come from the requirement and your arithmetic, not a call to `total`
used to calculate its own expected answer.

The opening comments let the documentation case harness also check main's zero
result and a clean memory audit. They are comments when you run `fibc test`.

## Change it

1. Add an order with price `9`, quantity `2`, delivery `6`; require `24`.
2. Add the boundary case with quantity zero and delivery zero; require `0`.
3. Implement the quantity validation from exercise 13 and add specs accepting
   `"0"`, rejecting `"-1"` and rejecting `"twelve"` with the chosen errors.

## Break and repair

In a copy, replace `+` in `total` with `-`. Run the same specs unchanged.
Require a failing run. Explain which requirement catches the mistake. Restore
the addition and require a clean run again. Do not change expected values to
match the broken function.

## Checkpoint

Show a passing run, a deliberately failing run and a passing repaired run.
Explain why a scenario that never checks an outcome would not protect the
program. The [testing guide](../guide/testing.md) describes runner commands.

[← Failures](13-failures.md) · [Course](README.md) · [Projects →](15-project.md)
