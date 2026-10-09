---
examples: required
---

# Protocols, records and enums

A protocol defines operations on implementing types. `impl` supplies methods;
`dyn` packages a value for dynamic protocol dispatch. `defstruct` supplies
fields, `defenum` supplies alternatives and `match` inspects them. `defrecord`
is a library macro, not a core form. `derive` supplies supported library
implementations; it does not implement arbitrary protocols. The coherence and
orphan rules in [types §2.9](../../spec/types.md) constrain where implementations
can be declared. Start with static dispatch:

```fib run
(ns main)
(defprotocol Measure (measure (self) -> i64))
(defstruct Box (value: i64))
(impl Measure Box (measure (self) (. self value)))
(defun main () -> i64 (do (println (measure (Box 7))) 0))
```

```text out
7
0
```
