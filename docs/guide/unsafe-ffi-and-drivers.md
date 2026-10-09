---
examples: required
---

# Unsafe FFI and drivers

Native access belongs in driver modules ([ADR 0011](../adr/0011-native-access-is-confined-to-drivers-and-the-platform-layer.md)).
A driver declares native symbols with `extern` and `:lib`, contains unsafe
pointer operations, and exposes typed errors and contracts to applications.
Use `fib.os` for OS services rather than repeating libc flags or layouts.
`--link NAME=static` changes one reached native library's mode; `--link-mode`
changes the default. Shipping static libraries requires their archives and
transitive dependencies. The [static-linking design](../design/static-linking.md)
details this boundary. Database drivers should implement `fib.db` protocols
and pass `fib.db.contract`; SQLite/Postgres drivers are external packages,
not in-tree modules. An application can use an ordinary checked library API:

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64 (do (println (text/join ", " ["tea" "cup"])) 0))
```

```text out
tea, cup
0
```

A driver can declare a native function with its library and wrap the unsafe call
in a checked API. This single-file driver demonstration uses libm's `hypot`:

```fib run
(ns main)
(extern hypot (f64 f64) -> f64 :lib "m")
(defun distance (x: f64 y: f64) -> f64 (unsafe (hypot x y)))
(defun main () -> i64 (do (println (distance 3.0 4.0)) 0))
```

```text out
5.0
0
```

Before publishing, put native declarations in the driver's module, export the
wrapper through its facade, specify supported platforms and run its contracts.
For a concrete native-library link fixture, see
[the compiler driver example](../../compiler/tests/driver/linklib/uses-answer.fib).
