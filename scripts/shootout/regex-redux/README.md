# regex-redux

The Benchmarks Game's regex-redux: read the FASTA file on stdin, strip the header lines and the newlines with a
regex, count the matches of nine IUPAC variants of `agggtaaa|tttaccct` (leftmost-first, non-overlapping), apply
five substitutions one after another (`tHa[Nt]`, `aND|caN|Ha[DS]|WaS`, `a[NSt]|BY`, `<[^>]*>`,
`\|[^|][^|]*\|`), and print the nine counts and the three lengths (input, stripped, final).

Files: `regex-redux.fib` (on `fib.regex`, the library of `lib/fib/regex/`), `regex_redux.java` (`java.util.regex`;
the file has an underscore because a Java class name has no hyphen), `regex_redux.clj` (namespace `regex-redux`,
`java.util.regex` through a `Matcher`, type hints, zero reflection warnings), `regex-redux.c` (PCRE2 with its JIT,
opened with `dlopen` because the container has `libpcre2-8.so.0` and no header), `sizes.txt`.

## Rules followed

- The same algorithm in all four: one regex per step, the counts by a find loop, the substitutions by replace-all
  on the result of the previous one. No language reads the text into anything but one string or buffer.
- The size is the Game's: N = 5000000 is a 50833411-byte file (the other two stdin benchmarks use 25000000).
  The expected output, which is the published one, is in `sizes.txt`: the counts 356 1250 4252 2894 5435 1537 1431
  1608 2178, then 50833411, 50000000 and 27388361.
- Output of the four is byte-identical (md5 in `sizes.txt`).
- fibber: `fib.unix` `fd-read` in 64 KB blocks for the input, no `unsafe`. `re-count` is the find loop without
  the `Match` objects, as Java's `while (m.find()) count++` makes none.
- Clojure: `(re-pattern ..)` and `Matcher.find`/`replaceAll`, as the published entries do; no arithmetic beyond
  `inc`; the in-program time goes to stderr.

## Gaps hit

See `docs/shootout/gaps.md`, section RE.
