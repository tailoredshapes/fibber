# A write through a helper with an `&` parameter (AMP-0)

Found 2026-10-21 in `physics.kernel` (physics-fib), where it made every event cost O(n) and a grid slower than the all-pairs look.

```
fibc run docs/shootout/amp-param/amp.fib
```

Measured on an Intel Core i7-14700KF, LLVM 21, single runs, nanoseconds per write to an `(Array i64)` of `n` entries held in a cell (`(array-set! &c i v)`):

| n | a cell of its own, in line | a cell of its own, through `(defun wi! (&c: (Array i64) i v) ..)` | a cell a struct also holds, in line | a cell a struct also holds, through the helper |
|---|---|---|---|---|
| 1,000 | 1.7 | 5.4 | 1.7 | **192** |
| 100,000 | 1.7 | 5.4 | 1.3 | **16,158** |

So a helper with an `&` parameter is free on a cell that only the caller holds, and copies the whole array, once per call, on a cell the caller also reaches through a struct field
(`(let [head (. k head)] (wi! &head i v))`). Written in line the write is in place in both cases. Reads through a helper (`(defun rd (c: (Cell (Array i64)) i) (array-get @c i))`) are 8 ns either way.

Why: what is measured is that the callee's `array-set!` finds the array shared when the cell is also reachable through a struct, and not when it is the caller's alone. The reading that fits is the rule of
docs/design/in-place-update.md section 1 (a write is in place when the array is held once): the content is moved into the callee only where the caller is known to hold the cell alone. **The ownership checker's own
rule for an `&` argument was not read for this note**; the table is the evidence, the explanation is a reading of it. Whether the compiler could steal the content for a struct-held cell, and whether it should
(another holder could read the cell during the call), is an open question for the owner.

What to do: in a loop that updates arrays held in struct fields, write `(array-set! &col i v)` in line on a local name for the cell; a macro that expands to the
in-line form keeps the call short. If a function must take a cell, take it whole (`(Cell (Array i64))`, no `&`) and write `(array-set! &c i v)` inside it: the write is then in the function's own scope. A growth
of the cost with the size of the array is the sign of a copy.
