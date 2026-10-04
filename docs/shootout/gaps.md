# Shootout gaps

One section per gap found while writing the shootout programs: what is missing, the smallest reproduction, what the
Java/C version uses, and a recommendation.

## fannkuch-redux: no mutable array that a callee can write without a copy

**Missing.** `(MArray t)`, `long-array`, `aget`, `aset` (spec/stdlib.md §2.11, rows `aget aset alength long-array`) are specified
but not in the library or the v0.1.4 seed:

```
$ fibc build t.fib -I lib      ; (defun f (p: (MArray i64)) -> i64 (aget p 0))  (defun main () -> i64 (f (long-array 4)))
rejected:
t.fib:1:15: unknown type MArray
t.fib:2:38: unbound name long-array
```

The program therefore uses the builtin value form: `(cell (array n 0))`, `@c`, `(array-get @c i)`, `(array-set! &c i x)`, and
passes the arrays to helpers as `&p: (Array i64)` in/out parameters. That is legal safe fibber, but see the next gap.

**Java/C use** `int[]` / `int a[32]`, written in place from any callee.

**Recommendation.** Library: land `MArray` with `long-array aget aset alength` as specified (the benchmark would then read like
the Clojure twin).

## fannkuch-redux: forwarding an `&` parameter to a callee copies the array on every call

**Reproduction.** `(defun rev (&p: (Array i64) k: i64) -> unit ...(array-set! &p ...))` called from
`(defun flips (&p: (Array i64)) ...  (rev &p k) ...)`. `fibc explain` says `@20:17 (reverse-prefix ..) call &p: acquire`
(whereas a call from a function that owns the cell says `&p: move in`, cases/ownership/261). `acquire` retains the array for
the callee's private cell, so the callee's first `array-set!` finds it shared and copies it (`fib.array-slice`, `fib.array-alloc`
in the profile: 30% of the run at N=10, 0.55 s against 0.39 s after inlining the callee by hand).

**Java/C use** a plain array reference; no copy.

**Recommendation.** Language/compiler change: treat `&p` of an `&` parameter like `&c` of a cell (move the content in, store the
result back) when nothing else can read `p` during the call. Until then the benchmark inlines the loop (a local rewrite).
