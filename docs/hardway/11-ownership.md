---
examples: required
---

# Exercise 11: Follow ownership

**Aim:** observe a borrowed read and an escaping captured value. Save `ex11.fib`.
You will inspect compiler decisions, not add manual retain/release calls.

## Type and predict

```fib run
(ns main)
(defun size (s: str) -> i64 (str-len s))

(defun greeting (name: str) -> (fn () str)
  (fn () (str "hello, " name)))

(defun main () -> i64
  (let ((name "Ada")
        (n (size name))
        (say (greeting name)))
    (do
      (println n)
      (println name)
      (println (say))
      0)))
```

`size` reads its argument. That read does not stop the caller using `name`
again. `greeting` returns a closure: the captured name must remain alive after
that call returns. Fibber borrows for reads and counts escaping values; the
compiler chooses. The type `(fn () str)` describes a function taking no arguments
and returning a string.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
3
Ada
hello, Ada
0
```

</details>

## Inspect and change

```sh
fibc explain ex11.fib
```

1. Find `size` and `greeting` in the explanation. Record the decisions around
   reading and capturing `name`. The exact diagnostic layout is not a fixture.
2. Call `say` twice. Predict whether the first call consumes the greeting.
3. Produce two greetings with different names and call them in reverse order.
   Require each closure to retain its own name.

## Break and repair

Replace `(say)` with `(say 1)` in a copy. The closure's function type takes no
arguments. Repair the call and explain why ownership checking does not replace
ordinary type/argument checking. Exercise 12 will provoke an ownership-specific
rejection with two in-out uses of the same place.

## Checkpoint

Explain what must survive after `greeting` returns, and why a borrowed read is
different from a value kept by a returned closure. Consult the
[ownership guide](../guide/ownership-and-borrowing.md) if your explanation relies
on garbage collection or programmer-written lifetimes.

[← Alternatives](10-matching.md) · [Course](README.md) · [Mutation →](12-mutation.md)
