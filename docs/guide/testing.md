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

For unit tests, use `fib.test.unit`. `deftest` defines a function that returns
a named test value. The `[t]` argument gives each run a fresh assertion log.
`is` checks a boolean expression; `is=` checks equality and reports expected
and actual values. Each assertion records its source location, evaluates its
arguments once, and lets subsequent assertions run after a failure. An empty
test fails.

```fib run
(ns main (:use fib.test.unit fib.test.run))

(deftest addition [t]
  (is= t 4 (+ 2 2) "addition")
  (is t (> (+ 2 2) 3)))

(defun main () -> i64
  (run-main [(unit-suite "arithmetic" [(addition)])]))
```

Save this as `arithmetic-spec.fib`, then run `fibc test arithmetic-spec.fib`.
Use `--only arithmetic/addition` to select the test, `--list` to list it,
or `--format json` / `--format tap` for machine-readable reports. The same
runner serves units and behaviour specs. `--isolate fork --timeout 10`
isolates crashes and limits each test to ten seconds; `--isolate task`
catches ordinary traps, and `--isolate none` runs in the current thread.
Use fork isolation for tests that can hang or corrupt process state.

Test modules can export a `tests` function returning a `Suite`. A shared
`*-spec.fib` entry point imports those modules and passes their suites to
`run-main`. This compiles shared dependencies once. `test-case` also wraps
an existing function returning a vector of assertion steps, so older test
bodies can move to the runner without changing their checks.

The compiler follows this pattern: `compiler/tests/units.sh` builds one
optimized runner for each of `emit`, `own`, `types` and the remaining units.
The generated executables stay in its output directory. Run one again with
`--only unit-annot`, for example, without rebuilding it. Each module runs
in a forked child by default; build and execution timings are separate.

```fib run
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "arithmetic"
    (scenario "addition"
      (then (expect = 4 (+ 2 2))))))
(defun main () -> i64 (run-main (specs)))
```
