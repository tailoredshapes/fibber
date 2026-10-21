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

Why (docs/design/in-place-update.md 4.3): an `&` argument gives the callee the cell's content to update and takes it back; the content is moved in only when the caller can show that nothing else holds the
cell, and a cell that a struct also holds is not shown to be, so the content is retained for the call and the callee's `array-set!` finds it shared and copies. This is by design (another holder could read the cell
during the call, including on another thread); the cost is what is worth knowing.

What to do: in a loop that updates arrays held in struct fields, write `(array-set! &col i v)` in line on a local name for the cell, as `fib.tensor` and the other library code does; a macro that expands to the
in-line form keeps the call short. If a function must take a cell, take it whole (`(Cell (Array i64))`, no `&`) and write `(array-set! &c i v)` inside it: the write is then in the function's own scope. A growth
of the cost with the size of the array is the sign of a copy.
