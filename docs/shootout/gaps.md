## k-nucleotide and reverse-complement (IO part)

Seed compiler v0.1.4 (`scripts/fetch-seed.sh`), library of this tree. Times are one run on a shared machine unless stated.

### IO-1. No growable byte buffer, no array concatenation, no block copy

What is missing: a way to build one `(Array i8)` from the 64 KB blocks that `fd-read` returns, or to grow an array
geometrically. `array-push!` is the only growth primitive and, from 32 elements on, it allocates `len + 1` elements and copies
(spec/types.md §2.13.1, `fib.array-room`), so a byte-at-a-time append is quadratic. `array-copy` is a slice, not a blit; there
is no `array-concat`, no `array-copy-into`, and `fd-read` itself copies each block once more (`array-copy a 8 len` in lib/fib/unix.fib).

Smallest reproduction (growing an array by pushes; 2x the pushes costs 4x):

```clojure
(defun grow (n: i64) -> i64
  (let [c (cell (array 0 (trunc i8 0)))]
    (do (dotimes (i n) (array-push! &c (trunc i8 i))) (array-len @c))))
```
`(grow 20000)` takes 4 ms, `(grow 40000)` 15 ms; 125 million pushes (the THREE sequence of the full k-nucleotide input) would
take hours.

What Java/C use: `ByteArrayOutputStream` / `readAllBytes` / `System.arraycopy`; `realloc`.

Workaround in the programs (not a distortion: it is cheap, and both measure the same work): reverse-complement keeps the
complemented residues of a sequence as a list of 64 KB arrays and walks the list backwards, so it never joins them;
k-nucleotide sums the lengths of the blocks, allocates the result with `(array total 0)` and copies element by element.

Recommendation: library, in `fib.unix` or a new `fib.bytes`: `(array-copy-into! &dst dst-off src src-off n)` (a block copy that
is one `memcpy` in the runtime), and a `ByteBuf` (`bytes-push`, `bytes-append` with doubling capacity) or `array-push!` with
geometric growth for every length, not only below 32; plus `(fd-read-all fd)`.

### IO-2. `format` / `printf` are not implemented; and `%.3f` is not one rule

`(format "%s %.3f" "A" 30.295)` is `unbound name format` (stdlib.md row `format`, tranche 4). k-nucleotide prints percentages
with three decimals. Worse for a three-way comparison: Java's `%.3f` rounds the *shortest decimal representation* of the double
half up, C's `printf` rounds the exact binary value, so they disagree on exact ties. On the 200000-residue input that
`scripts/shootout/k-nucleotide` was tested with (the 1-mer count 49885 of 200000, 24.9425 %), Java and a half-up
`floor(x*1000+0.5)` print `24.943` and C prints `24.942`. All four programs therefore compute the percentage in exact integer
arithmetic, `v = (count*200000 + total) / (2*total)`, printed as `v/1000 "." v%1000` (three digits): rounded half up on the exact
rational, identical in every language and equal to the published outputs wherever there is no exact tie.
Recommendation: library function `(format-f64 x digits)` with one stated rule (exact binary value, round half to even as C
and Rust do, or Java's); the `format` macro after it.

### IO-3. A helper that writes through an `&` array parameter copies the array on every call

```clojure
(defun put (&buf: (Array i8) &pos: i64 b: i64) -> unit
  (do (array-set! &buf @pos (trunc i8 b)) (set! pos (+ @pos 1))))
```
called once per output byte from a loop that owns `buf` in a cell: `fibref explain`/profile say `fib.array-slice` is 99.8% of the
run (the 64 KB buffer is copied per call): reverse-complement of a 2.5 MB input took 2.80 s, against 0.012 s once the write is
inlined in the loop that owns the cell (spec/stdlib.md §5 "The cause": `&` copy-in acquires, so the first write of every `&` call
copies). A programmer meets this at once with any buffered writer. Recommendation: language change (C2/C3/C4 of the stdlib
spec, an `&x` that is the only mention shares without acquiring) or a library `ByteBuf` type whose methods are the in-place writes
(see IO-1). The program inlines the loop.

### IO-4. `(assoc m k (+ 1 (get m k 0)))` in one expression is 3.4 times slower than binding the count first

Same loop, 1.25 million updates of pseudo-random keys in a `Map i64 i64`: 1480 ms for the nested form, 440 ms when
`(let [c (get m k 0)] (assoc m k (+ c 1)))`. `m` is not unique while the `get` is evaluated inside the argument list, so
the assoc copies the path. The program uses the second form. Recommendation: compiler (evaluate the arguments of `assoc`
that do not mention the result before taking the collection), and a library `(update m k f)` / `(inc-in m k)` whose in-place
case is the one the program wants.

### IO-5. Java class names

A Java public class cannot be called `k-nucleotide`; the twins use non-public classes `k_nucleotide` and `reverse_complement`
in files `k-nucleotide.java` / `reverse-complement.java` (`javac` accepts that; run with `java -cp DIR k_nucleotide`). The
Clojure twins are scripts (`clojure.main k-nucleotide.clj`), since `-m` needs a namespace that maps to a file name with
underscores.
