---
examples: required
---

# Testing

`fibc test` discovers `*-spec.fib`, or accepts explicit files/directories.
Specs use `feature`, `scenario`, `given`, `upon`, `then` and `expect`. A spec
program's main uses `run-main`. The example is also compiled and run by the
documentation gate. Run it with `fibc test FILE` for driver reporting.
`fibc cases` instead checks verdict headers, results and memory audits.
Contributors use `make specs`, `make adr`, `make doc-examples`, `make quick`
and the full `make gate`. A pending/open result is not a pass.

```fib run
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "arithmetic"
    (scenario "addition"
      (then (expect = 4 (+ 2 2))))))
(defun main () -> i64 (run-main (specs)))
```
