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

## Results (RE agent, 2026-10-04; flock'd, /usr/bin/time, median of 3 at full and 5 at small, fibber under `ulimit -v 16000000`)

elapsed seconds / user seconds / peak RSS in MB. Clojure: whole process, and in parentheses the in-program time of the work.

| size (fasta N) | fibber | Java | Clojure | C (PCRE2 JIT) | fib/Java | fib/Clojure |
|---|---|---|---|---|---|---|
| small, 50000 | 0.03 / 0.02 / 8.4 | 0.15 / 0.35 / 53 | 0.47 (0.111) / 1.54 / 120 | 0.01 / 0.01 / 3.7 | 0.20 | 0.06 wall, 0.27 in-program |
| full, 5000000 | 3.00 / 2.59 / 423 | 7.66 / 7.58 / 1237 | 9.91 (9.43) / 10.72 / 1280 | 1.71 / 1.62 / 101 | 0.39 | 0.30 wall, 0.32 in-program |
| 25000000 (once) | 14.68 / 12.63 / 2107 | 36.97 / 35.64 / 5503 | 46.89 (46.02) / 46.44 / 5925 | 9.09 / 8.68 / 496 | 0.40 | 0.31 wall, 0.32 in-program |

Start-up (hello world, median of 5): fibber 0.00 s, Java 0.01 s, Clojure `clojure.main -e nil` 0.22 s (user 0.63), C 0.00 s.

Where fibber loses to C (1.75x at full): the profile (`scripts/bench/tools/prof.sh`) of the full run is 36% the DFA scan loop
(`lib/fib/regex/dfa.fib`, one byte, one class lookup and one table lookup per character, plus a runtime call for the byte, RE-2 in
the gaps), 15% memcpy/memset of the result strings, 9% the byte copy loops, 8% span vector reads, 7% the backward scan for the
start of each match, 2% the 50 MB read. PCRE2's JIT scans the literal prefixes with vector instructions.
Phases at full (ms): read 116, strip 324, the nine counts 78 each (700), the five substitutions 196 + 202 + 490 + 576 + 331.
