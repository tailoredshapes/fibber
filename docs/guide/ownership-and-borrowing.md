---
examples: required
---

# Ownership and borrowing

Values are borrowed for reads and retained when stored, returned or captured
by an escaping closure. The compiler chooses scope allocation and counting.
Closures that stay within a call can borrow; escaping closures keep their data
alive. Private cells mutate locally; atoms synchronize shared updates. `freeze`
marks a shareable object graph frozen and `weak` avoids retaining a referent.
Cycles of strong references can leak; use a weak link where appropriate.

`&x` is an in-out place, not a second first-class reference. Passing the same
place to two in-out parameters in one call is rejected. Save the accepted
example for `fibc explain FILE`. Full rules: [ownership](../../spec/ownership.md),
[types](../../spec/types.md), and the checked Appendix A cases in
[syntax](../../spec/syntax.md#appendix-a--the-20-ownership-cases-in-this-syntax).

```fib run
(ns main)
(defun size (s: str) -> i64 (str-len s))
(defun main () -> i64
  (let ((s "hello") (n (size s))) (do (println n) (println s) 0)))
```

```text out
5
hello
0
```

```fib reject "passed to more than one & parameter"
(defun bar (&a &b) (append &a 1) (append &b 2))
(defun main () -> i64
  (let ((x (cell [3 4]))) (do (bar &x &x) 0)))
```
