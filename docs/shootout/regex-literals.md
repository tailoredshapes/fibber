# Regex literals and the immutable `Regex` (batch 6, RXM)

Mechanism: option (a) of the brief, with the expander doing the lifting. `#"re"` reads as `(fib.prelude/re "re")`; after a top-level
form is expanded, `compiler/expand/regex.fib` replaces each such form by a `:private` `def re$N` placed before the form, whose
initialiser `(fib.regex.api/re-pattern "re")` is in the constant grammar (`types.lower.mod/regex-literal-call?`, only when the
library accepts the text), so the existing per-def JIT session (`emit.defs.jit`) runs it at compile time and writes the finished
`Regex` out as static data. Why not a macro (b): a macro runs in a macro-time module that sees only the prelude and the modules
the user requires, and compiling the library into every macro session costs seconds; the compiler is itself a fibber program, so it
links `fib.regex.api` natively and checks the pattern with no JIT. Why (a) works: the `Regex` no longer holds a `Cell`.

## Measurements (many tiny searches, ns per iteration; `/home/tmarsh/.cache/fibber-scratch/B6-RXM/t/b1.fib`)

| case | before (shared cache in the Regex) | after |
|---|---|---|
| `\d+` find on 12 bytes | 92 | 81 |
| `\d+` count on 12 bytes | 35 | 27 |
| `a[ab]{14}c` (no table: 2^14 states) find on 44 bytes, per-search state | 160 | 20382 with a cold lazy DFA; 9068 with the Pike VM that now takes texts under 256 bytes |
| `re-pattern "\\d+"` plus one search | 108717 (two 10000-state tables allocated) | 5315 |
| `re-pattern` of a four-way alternation plus one search | 42157 | 116353 (the tables are built eagerly) |

Per-search state costs nothing when the DFA is a table (the usual pattern), and a great deal for a pattern with no table on a short
text; a long text amortises it (regex-redux, 5M: below). A pattern made at run time pays for building the tables; a literal pays at
compile time. The first measured claim, 25 microseconds for a pattern plus one search, was 108 microseconds on the old library.

## regex-redux (`scripts/shootout/regex-redux`, fibber only, median of 3, output md5 equal)

| size | 15 patterns by `re-pattern` (new library) | 15 patterns as literals | recorded before the batch |
|---|---|---|---|
| small (50000) | 0.03 s | 0.03 s | |
| full (5000000) | 3.04 s | 2.97 s | 3.00 s |

The patterns are built once in this program, so making them at compile time saves microseconds out of seconds: the time is the
scans. The 25000000 run was not repeated.
