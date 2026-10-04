# fannkuch-redux

Every permutation of 1..n is generated in the game's order (rotate the first r+1 entries, count down `cnt`); for each, flip the
prefix named by its first entry until the first entry is 0, counting flips. Output: the checksum (flips summed, sign alternating
with the permutation's index) and `Pfannkuchen(n) = maxflips`. N=12 gives `3968050` / `Pfannkuchen(12) = 65`.

Files: `fannkuch-redux.fib`, `fannkuch_redux.java` (a Java class name cannot contain a hyphen, so the class and file are
`fannkuch_redux`), `fannkuch-redux.c`, `fannkuch-redux.clj` (run as a script: `clojure.main fannkuch-redux.clj N`; it prints the
in-program seconds to stderr), `sizes.txt`.

Rules followed: the same single-threaded algorithm in all four; int/long arrays, no allocation in the loop; Clojure has
`*warn-on-reflection*` on with zero warnings and `*unchecked-math* :warn-on-boxed` with no warnings (checked `long` arithmetic is
kept; nothing overflows). fibber: no `unsafe`, library tools only.

Gaps (docs/shootout/gaps.md): `MArray`/`long-array`/`aget`/`aset` are specified but not implemented, so the program uses
`(cell (array n 0))` with `array-get`/`array-set!` and `&` parameters; forwarding an `&` parameter to a callee copies the array on
each call, so the flip loop is written inline in `count-flips`.
