# pidigits

The streaming spigot of the Benchmarks Game (Gibbons' unbounded spigot): three big integers `numer`, `accum`, `denom`; each
step k adds a term (`accum = (accum + 2 numer)(2k+1)`, `numer *= k`, `denom *= 2k+1`); when `numer <= accum` and the digit of
`(3 numer + accum) / denom` equals that of `(4 numer + accum) / denom`, it is the next digit, printed ten to a line followed
by a tab, a colon and the running count (a last short line is padded with spaces to ten); the digit is then eliminated
(`accum = (accum - denom d) 10`, `numer *= 10`).

Rules followed: the same algorithm in all four programs (the C one uses GMP's `mpz` as the game's C entries do, the Java one
`java.math.BigInteger`, the Clojure one `BigInteger` through interop with type hints and no reflection, as the published
entries; fibber uses the library's `BigInt`, `lib/fib/bigint`, with plain `+ * quot` and `long`); nothing is memoised or
cached across steps; output through each language's ordinary output (`println` per line in fibber).

Files: `pidigits.fib`, `pidigits.java`, `pidigits.c` (link with `-lgmp`: the file `cflags` says so), `pidigits.clj`
(`-m pidigits`; the in-program time is printed to standard error as `clj-compute-s`), `sizes.txt`.

Output md5 at N=1000 and N=10000 is that of a fifth, independent program (Machin's formula in Python's integers,
`reference.py` here: digits cut to N, ten to a line); all four programs agree with it.

Gaps hit: docs/shootout/gaps.md (no widening multiply, so 31-bit limbs; the allocator's heap trimming; `+'` unreadable).

## Measured (2026-10-04, shared 28-core machine, suite lock held, `ulimit -v 16000000`; median of 5 small, 3 full)

| size | | fibber | Java | Clojure | C (GMP) |
|---|---|---|---|---|---|
| N=1000 | elapsed s | 0.03 | 0.07 | 0.48 (in-program 0.056) | 0.00 |
| | user s | 0.03 | 0.20 | 1.60 | 0.00 |
| | peak RSS MB | 1.9 | 80 | 160 | 2.2 |
| N=10000 | elapsed s | 5.23 | 2.82 | 3.66 (in-program 3.13) | 0.50 |
| | user s | 3.67 | 2.92 | 4.59 | 0.50 |
| | peak RSS MB | 3.2 | 412 | 1199 | 2.4 |
| | fibber / Java | 1.85 | | | |
| | fibber / Clojure (whole process, in-program) | 1.43, 1.67 | | | |
| | fibber / C | 10.5 | | | |

Baselines: JVM hello 0.01 s, `clojure.main -e nil` 0.34 s, C hello 0.00 s. Fibber's 1.56 s of the 5.23 s is system time: the
allocator hands back and re-faults the heap on every freed 60 to 120 KB array (32276 `brk` calls in a 5000-digit run; gaps.md).
