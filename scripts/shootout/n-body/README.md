# n-body

The Benchmarks Game's n-body: the Sun and four giant planets, positions and velocities from the published table,
momentum offset so the Sun cancels the total, then N steps of a symplectic advance with dt = 0.01 (all ten pairs in
turn, then every position), printing the energy before and after with `%.9f`.

Files. `n-body.fib` (fibber), `nbody.java` (Java; the class is `nbody` because `n-body` is not a Java name: INFRA must
map the directory name to it), `n-body.c` (C: gcc -O3 -march=native **-ffp-contract=off**, because fused multiply-add
changes the last bits and the output must be byte-identical), `n-body.clj` (Clojure, run as a script,
`clojure.main n-body.clj N`; it prints its in-program time to stderr). `sizes.txt`: small N 500000, full N 50000000, with
the md5 of the stdout. The full output is the published `-0.169075164` then `-0.169059907`.

Rules followed. The same operation order everywhere (Java's `a.vx -= dx * b.mass * mag` is `(dx*mj)*mag`), `Math.sqrt`
and libm `sqrt` (correctly rounded), no `Math.fma`. Java: objects with `double` fields, as the game's entry. C: a static
array of structs. Clojure: one `double-array` (35 doubles, body i at 7i), `^doubles` hints, `*warn-on-reflection*` and
`*unchecked-math* :warn-on-boxed` with zero warnings; no integer wrapping is needed (double arithmetic). fibber: one
`(Array f64)` in a `cell`, written in place by `array-set!` through `&` parameters, reads through a borrowed parameter;
no `unsafe` in the program.

Gaps hit (docs/shootout/gaps.md): `sqrt` is not a builtin (library `fib.math/sqrt` over libm, in this commit),
`format "%.9f"` is not landed (library `fib.fmt/format-f64`, in this commit), no `MArray`; and the profile finding that
a read of `@cell` retains and releases the array. The program is correct against Java, C and Clojure at every size
tried (see sizes.txt and the report); the cost is in the compiler, not in the algorithm.
