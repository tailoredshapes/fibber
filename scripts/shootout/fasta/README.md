# fasta

The game's fasta: write `>ONE` (the 287-byte ALU sequence repeated to 2N bytes), `>TWO` (3N random bytes from the 15-symbol IUB
table) and `>THREE` (5N random bytes from the 4-symbol Homo sapiens table), 60 columns per line. The random numbers come from the
LCG `last = (last * 3877 + 29573) mod 139968` (seed 42, carried from TWO into THREE); a symbol is the first table entry whose
running probability sum exceeds `last / 139968.0` (the last entry when none does). All four programs use the same f64 running sums.

Files: `fasta.fib`, `fasta.java`, `fasta.c`, `fasta.clj` (`clojure.main -m fasta N`; prints its in-program time to stderr), `sizes.txt`
(`small|full N <n> <md5 of stdout>`).

Rules followed
- One 64 KB byte buffer, filled in place and written when it holds more than 65475 bytes; the same in every language.
- fibber: the buffer is an `(Array i8)` in a `cell`, written through `&buf` with `array-set!`; output is `fib.unix/fd-write-range`
  on fd 1 in a loop that handles short writes. No `unsafe`, no raw pointers.
- Java: `byte[]` and `System.out.write(buf, 0, n)`. C: `char[]` and `write(2)`.
- Clojure: type hints, zero reflection warnings, `*unchecked-math* :warn-on-boxed` with zero boxed-math warnings; `byte-array`,
  `long-array` for the position, `loop`/`recur`. Arithmetic does not overflow a long, so unchecked vs checked changes nothing.
- Output is identical in the four languages at N = 1000, 2501, 1000000 and 25000000 (md5s in the report); `N=1000` TWO starts
  `cttBtatcatatgctaKggNcataaaSatgtaaaDcDRtBggDtctttataattcBgtcg`, as the game's published output does.

Gaps met: none that block the benchmark. Small papercuts: the unit value is written `()` (there is no `unit` value name), and a
program reads its arguments with `(args)`, which spec/stdlib.md documents only through `*command-line-args*` (open, not yet a
constant).
