---
examples: required
---

# Exercise 7: Look up a key

**Aim:** use a map and make a missing-key policy explicit. Save `ex07.fib`.

## Type and predict

```fib run
(ns main (:use fib.core fib.coll))
(defun main () -> i64
  (let ((stock {"tea" 3 "coffee" 0})
        (restocked (assoc stock "tea" 5)))
    (do
      (println (get stock "tea" -1))
      (println (get stock "coffee" -1))
      (println (get stock "water" -1))
      (println (get restocked "tea" -1))
      (println (get stock "tea" -1))
      0)))
```

Map entries alternate key and value. This map's keys are strings and its values
are integers. The third argument to `get` supplies a default for an absent key.
`assoc` returns an updated map. Do not depend on map iteration/printing order.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
3
0
-1
5
3
0
```

</details>

## Change it

1. Add water with quantity `2` to a new map. Show that stock still lacks water.
2. Change the default to zero. Explain which two situations now look identical.
3. Find `contains?` in the [library declarations](../reference/library/fib.coll.md)
   and use it to distinguish absent water from coffee with zero stock.

## Break and repair

In a copy, add a string quantity to this integer-valued map. Read the type
diagnostic, then restore a number. A key's spelling can be wrong even when its
type is right: try `"Tea"`, predict the default and repair the spelling.

## Checkpoint

Explain why a default of zero would be reasonable for a word-frequency counter
but insufficient when you need to distinguish “unknown product” from “sold out.”

[← Vectors](06-vectors.md) · [Course](README.md) · [Loops →](08-loops.md)
