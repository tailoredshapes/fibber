# spectral-norm

Algorithm (Benchmarks Game): the matrix A(i,j) = 1 / ((i+j)(i+j+1)/2 + i + 1); start with u = 1, ten rounds of
`v = AtA u; u = AtA v` (the products summed left to right in f64), then print sqrt(u.v / v.v) as `%.9f`.
Reference: N=5500 gives 1.274224153, N=100 gives 1.274219991.

Files: `spectral-norm.fib` (fibber, `(Array f64)` and a cell), `spectral-norm-vec.fib` (fibber over `(Vec f64)`,
`nth` and `assoc`), `spectralnorm.java` (the class name cannot contain a dash), `spectral-norm.c`, `spectral-norm.clj`
(run as `clojure.main spectral-norm.clj N`; it calls `-main` itself and prints its in-program time to stderr).

Rules followed: the same arithmetic order everywhere, so the output is byte-identical (md5 in `sizes.txt`; all of
fibber, the Vec variant, Java, Clojure and C agree at both sizes). Java and C use `int` indices as the published
programs do; fibber `i64`, Clojure `long` (with `*unchecked-math*`; the products are exact at these sizes so no result
changes). Clojure: `^doubles` hints, `loop`/`recur`, `double-array`, zero reflection warnings, one `-main`.
No `unsafe` in the fibber programs.

Gaps hit (docs/shootout/gaps.md): no `MArray`; no `sqrt` (Newton, one call on the final ratio); no `printf`
(`fmt9`, a string rounding of the shortest repr, one call).

## Measured (median of 5 small, 3 full; elapsed s / user s / peak RSS MB; one machine, serialised by flock)

| size | fibber | fibber (Vec) | Java | Clojure (whole process) | Clojure (in program) | C |
|------|--------|--------------|------|-------------------------|----------------------|---|
| small N=1500 | 0.10 / 0.10 / 1.5 | 0.56 / 0.56 / 1.9 | 0.12 / 0.17 / 45 | 0.57 / 1.70 / 129 | 0.101 | 0.04 / 0.04 / 2.0 |
| full N=5500 | 1.35 / 1.35 / 1.7 | 7.28 / 7.27 / 2.9 | 1.11 / 1.15 / 44 | 1.37 / 2.26 / 133 | 0.995 | 0.51 / 0.51 / 2.2 |

Start-up: C hello 0.00 s, fibber hello 0.00 s, `java` hello 0.01 s, `clojure.main -e nil` 0.27 s (0.81 s user).
Ratios at full size: fibber/Java 1.22, fibber/Clojure 0.99 (whole process; 1.36 against its in-program time),
fibber/C 2.65. The Vec variant is 5.4 times the Array one: profile by elision, replacing `(nth v j)` by a constant
takes N=2000 from 0.92 s to 0.16 s, so `nth` on a `Vec` (a non-inlined call into the trie walk, about 4.7 ns) is 85% of
the time; `(Array f64)` with `array-get` is a plain bounds-checked load. Against C the remaining cost is the checked
i64 arithmetic of the index expression (every `+` and `*` carries an overflow branch) and 64-bit `quot`/`double`
conversions where C uses 32-bit `int`; `-O3` and `shr` for `quot` change it by under 10%.
fibber's sampling profiler (scripts/bench/tools/prof.sh) segfaulted on this binary, so the evidence above is by elision.
