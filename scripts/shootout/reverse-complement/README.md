# reverse-complement

Reads the fasta output (the Java fasta twin's, at size N) from stdin as bytes (64 KB blocks) and writes every sequence
reverse-complemented (IUPAC codes `ACGTUMRWSYKVHDBN` map to `TGCAAKYWSRMBDHVN`, both cases to upper case) in 60-column lines,
each preceded by its header line unchanged.

Files: `reverse-complement.fib`, `.java` (class `reverse_complement`), `.c`, `.clj` (a script), `sizes.txt`. The programs
ignore their arguments; `sizes.txt` gives the N of the fasta output to feed. `edge.py` writes two inputs that stress the
block boundaries (a header straddling a 64 KB edge, an empty sequence, irregular line widths, a last line without a newline,
a `>` on the first byte of the second block); all four programs agree with an independent Python reference on both.

## Rules followed
- Same algorithm in all four: each block is complemented into a block of its own (newlines dropped) and kept in a list for the
  current sequence; at the next header (or the end of the input) the list is written back to front through a 64 KB buffer,
  a newline after every 60 residues. No program joins the blocks.
- fibber: `fd-read` / `fd-write-range` from `fib.unix`, `(Array i8)` and `array-set!` on an array held in a cell; no `unsafe`.
  The output buffer is local to `emit-rc` (a helper writing through an `&` array parameter copies the array per call: gaps.md IO-3).
  Java: `FileInputStream` / `FileOutputStream` on the descriptors, `byte[]`. Clojure: the same with type hints, zero reflection
  warnings, `*unchecked-math*`. C: `read(2)` / `write(2)`.

## Gaps hit
docs/shootout/gaps.md: IO-1 (no growable byte buffer), IO-3 (`&` array helper copies per call).
