# Shootout gaps

What the shootout programs needed and the language or library did not have. One section per gap, appended by
each benchmark's agent. (Each benchmark worktree started this file on its own: merge the sections.)

## spectral-norm: `MArray` (`double-array`, `aget`, `aset`) is not in the library

**Missing.** `unknown type MArray` (stdlib §2.11, tranche 3, specified, not landed).

**What Java and C use.** `double[]`.

**What was done.** Two versions. `spectral-norm.fib` keeps its vectors as `(Array f64)` (read with `array-get`) and
builds each product in a `(Cell (Array f64))` written in place by `array-set!` through `&`: safe, no `unsafe`.
`spectral-norm-vec.fib` is the same with `(Vec f64)`, `nth` and `assoc`, the Clojure-shaped way. The Vec version is
about 5 times slower (the profile is in README.md): `nth` on a `Vec` is a call that walks the trie.

**Recommendation.** Land `MArray`; then `aget` on it should compile to the same load as `array-get`. Independently,
`nth` on a `(Vec a)` with a known-small count (the tail only, count <= 32) or a hoisted loop-invariant `v` would
close the gap for programs that use persistent vectors for numerics.

## spectral-norm: `sqrt` is not a builtin

**Missing.** `(math/sqrt x)`: `unbound name math/sqrt` (stdlib §4.16 says builtin; not landed).

**What Java and C use.** `Math.sqrt` / libm `sqrt`.

**What was done.** `sqrt-newton` in the program: 60 Newton steps from 1.0 on the final ratio, one call, so the time
is not distorted. It converges to within an ulp, far inside the 9 printed decimals; the three-way md5 agreement
at both sizes is the check.

**Recommendation.** The builtin (`llvm.sqrt.f64`), as the n-body section of this file (when merged) says.

## spectral-norm: `format` / `printf` with `%.9f` is not landed

**Missing.** `unbound name printf`; `str` of an `f64` is the shortest round-trip text.

**Reproduction.** `(ns main) (defun main () -> i64 (do (printf "%.9f\n" 0.5) 0))`: `unbound name printf`.

**What Java and C use.** `String.format("%.9f", x)` / `printf("%.9f\n", x)`.

**What was done.** `fmt9` in the program: rounds the shortest-repr text of a value in [1, 10) to 9 decimals, half
up, as Java does; it runs once. It is a local string function, not a library one: a general `format-f64` belongs in
the library (see n-body's section when merged, `fib.fmt/format-f64`).

**Recommendation.** The `%f` directive of the `format` macro with precision, written in fibber.

## spectral-norm: integer `/` yields a Ratio, so `(i+j)(i+j+1)/2` needs `quot`

Not a gap, a note for ports: `(/ a b)` on `i64` is a `(Ratio i64)`; `quot` (or `shr` for a power of two) is the
integer division Java's `/` is.
