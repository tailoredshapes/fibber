# mandelbrot

Writes the N x N Mandelbrot set as a P4 PBM bitmap on standard output. Pixel (x, y) is the point
c = (2x/N - 1.5, 2y/N - 1.0); it is in the set when |z|^2 <= 4 after 50 iterations of z = z^2 + c. Eight pixels
make a byte, the first pixel the high bit; a row whose width is not a multiple of 8 is padded with zero bits.

Rules followed: the same escape loop (the classic one with the squares kept in `tr`, `ti`) in all four
languages; no `unsafe`; one write per row (fibber `fd-write` of an `(Array i8)` held in a `Cell` and updated with
`array-set!`, Java `BufferedOutputStream`, C `fwrite`, Clojure a `byte-array` and `BufferedOutputStream`).
C is built with `-O3 -march=native -ffp-contract=off` so that no fused multiply-add changes the bits; the other
languages do not fuse. Clojure: type-hinted, `*warn-on-reflection*` on with zero warnings, primitive double and
long math in `loop`/`recur`, no unchecked arithmetic needed; it prints `in-program S s` to stderr.

Gaps: none that block it. Small things met: integer `/` is a ratio, so the byte count is `quot`; there is no
`Long/parseLong` in scope without a module use, so `(unwrap (parse-long s))`; unit is `()`, not `unit`.
