# cases/stdlib

The cases of the standard library (`spec/stdlib.md` §8.1). Each one is a
fibber program with a header that fixes its verdict (`spec/method.md`, rule
3), run in the reference interpreter and compiled, with equal results and
free traces (rule 6):

```
fibref cases cases/stdlib --only 240- 241-     # the interpreter, your block only
fibc   cases cases/stdlib --only 240- 241-     # interpreter and compiled, your block only
```

`--only` takes name prefixes; a prefix that matches no case is exit 2
(`no case matches 999-`), never an empty pass. A prefix ends with the dash: `100-` is
the cases numbered 100 and not the ones numbered 1000 to 1009. A run ends with the counts
line, `N cases: .. pass, .. fail, .. pending, .. header error`, which must
read `0 fail, 0 pending, 0 header error`; when an `open-` case ran there is
also `, N open`, and each open case is a row of its own that names its items.

## Layout

The directory is flat. A case is `NNN-slug.fib`, or a directory
`NNN-slug/main.fib` with its modules beside it when it needs several files
(`cases/modules/README.md`). `support/` has no `main.fib`, so it is not a
case: it holds the modules the generated cases share. The library itself is
`lib/fib/`, found by the header key `roots` (`;; roots: ../../lib support`)
until the embedded copy of `lib/` is rebuilt into every binary.

The four facades are the implicit modules since the flip (step M of the plan),
so a case needs no `:use` of them; the cases written before it list the facades they
use, which stays valid (an explicit `:use` of an implicit module is the same module):

```
(ns main (:use fib.core fib.seq fib.coll fib.print))   ; or the subset it needs
```

Two things a case may not do: write `/` on integers (it is `Div`'s method in
every module, and means a ratio; write `quot` for the
truncated quotient), and name a part (`fib.seq.vseq`) where a facade will do.
The `List` variants are `Empty` and `Cons`, so `list` and a match on them work
in a module that `:use`s `fib.coll`, whose `empty` is a method.

## Numbering

Tranche 1 numbered its cases with three digits; the numbers 000 to 999 are used up. Tranche 2
numbers its cases with **four digits, `1000` to `5999`, a block per package**
(spec/stdlib.md §8.2.1 item 1). The number is the whole run of digits before the first dash, so
`1000-x` and `100-x` are two numbers, and the test `cases_are_named_and_labelled_alike` reads it
that way (a unit test fixes it).

Tranche 1 (three digits):

| Block | Package | Block | Package |
|---|---|---|---|
| 000-049, 900-949 | P0 | 050-099 | L1 |
| 100-119 | L2 | 120-139 | L3 |
| 140-159 | L4 | 160-199 | L5a |
| 200-239 | L5b | 240-319 | L6 |
| 320-379 | L7 | 380-419 | L8a |
| 420-479 | L8b | 480-499 | L9 |
| 500-549 | L10a | 550-599 | L10b |
| 600-649 | L11 | 650-699 | L12 |
| 700-749 | PX | 750-759 | R5 |
| 760-779 | R6a | 780-799 | R6b |
| 800-807 | R7 | 810-817 | R8 |
| 820-839 | R9 | 840-869 | V1 |
| 870-879 | M | | |

No two cases share a number (the test `cases_are_named_and_labelled_alike`).

Tranche 2 (four digits; the package ids are those of the tranche 2 plan, spec/stdlib.md §8.2.1):

| Block | Package | Block | Package |
|---|---|---|---|
| 1000-1049 | Z0 | 1100-1199 | X1 |
| 1200-1299 | X2 | 1300-1399 | X3 |
| 1400-1499 | X4 | 1500-1599 | X5 |
| 1600-1649 | X6 | 1650-1699 | X7 |
| 1700-1749 | X8 | 1750-1849 | X9 |
| 1850-1949 | X10 | 1950-1999 | X11 |
| 2000-2049 | X12 | 2050-2099 | X13 |
| 2100-2199 | P1 | 2200-2249 | Y1 |
| 2250-2349 | Y2 | 2350-2449 | Y3 |
| 2450-2549 | Y4 | 2550-2649 | Y5 |
| 2650-2749 | Y6 | 2750-2799 | Y7 |
| 2800-2849 | Y8 | 2850-2949 | Y9 |
| 2950-2999 | Y10 | 3000-3059 | Y11 |
| 3060-3119 | Y12 | 3200-3249 | Z2 |
| 3500-3599 | integration and the flips | 4000-4049 | in-place update (PERFB2 L7): persistence cases 4000-4007 (`scripts/mutant-unique.sh` must kill each), 4008-4009 audit, 4010-4016 counts |
| 5000-5999 | mutation additions, fifty a package | | |

A package writes only in its block. A case that waits for another package's work stays in the block of
the package that wrote it and names the item or package it waits for in `open:`; when that lands the case
flips and loses its `open-` and its `open:` line, and keeps its number.

### Conventions of tranche 2

* Every case keeps `;; roots: ../../lib support` until tranche 2's gate: the binaries embed the
  library of an older tree, and the working tree's `lib/` is the one under test. (The `support/` root
  is where the `tl` modules are.)
* **`open:` takes an item of spec/stdlib.md §7 or a package id**, from the list in the test
  (`ALLOWED_OPEN`, `crates/fibref/tests/stdlib_table/cases.rs`). Tranche 2 added the items `L3b L7 L8 L14
  L16 L17 L19 E2 E8 E11 B5` and the packages `X3 X4 X5 X6 X7 X8 X9 X10 X11 X12 P1 Y3 Y4 Y5 Y11 Y12`. A
  case never waits for a package of an earlier wave (X1, X2, Y1, Y2), and one that needs an id the list
  lacks asks for it. The label is a promise to flip: the day the item lands the case passes, the run
  fails, and the commit that lands the item removes the label.
* A macro that fails at expansion is a `reject` case whose header expects `macro NAME failed: NAME: message`.
  A case for a fibber macro uses it in head position only, and a library macro's body uses prelude
  names (`vec-count`, `vec-nth`) until E2 (spec/stdlib.md §6.3).
* A `ref-` case draws at most 300 seeded inputs. `tl.rng` has `(rng-vec r len bound)` for a vector of integers of
  a given length, and a length drawn per input is `(rng-below r n)`: `(rng-vec r (rng-below r 20) 100)`.
* **The `some` row.** The row `some` (`(some pred c)`, tranche 2) is the name of `Option`'s constructor, so
  its token is in nearly every case and says nothing. `stdlib_table` therefore counts a case as
  calling the row only when its code has `(some a b)`, a call of exactly two operands; `(some x)` in a
  pattern or as the constructor never covers it, and a case that lists `some` in `covers:` and has no
  such call fails `every_case_covers_real_rows_it_calls`. The case that covers `some` is read by the lead
  by hand besides: it must show a predicate result that is truthy and one that is not, and `some` over an
  `Option` payload.
* The rows that are syntax have no token of their own and are judged by `crates/fibref/tests/stdlib_table/syntax.rs`:
  `@x`, `~x` and `~@x` by that prefix in front of a form; `#(..)` and `#{..}` by the dispatch in the code;
  `:k` (the row of `(:k m)`: a name in `covers:` has no space, so the cell says `:k`) by a keyword right after an open parenthesis; `->Name` by any token `->Upper` (the record
  `Point` has `->Point`); `print-method` by the protocol `Debug`, so the case that covers it defines `(impl Debug ..)`.
  Write the case with the syntax in its code, in a string it does not count.

## Kinds

The kind is the first word of the slug after the number, and **the kind
words are reserved**: `ref`, `law`, `count`, `bound`, `rule`, `open` (and
`reject` and `trap`, below) mean their kind wherever they begin a slug. A
unit case about counting is not called `NNN-count-of-nothing.fib`: that
slug is a `count-` case, so the tests demand its `allocs: <= N` header and
`a_stdlib_count_bound_one_below_the_count_fails` lowers it by one; write
`NNN-the-count-of-nothing.fib`. The test `cases_are_named_and_labelled_alike`
and both `allocs.rs` tests read the kind the same way (the characters
after the number up to the next `-` or `.`), so the rule has no judgement in
it: a slug is a `count-` case exactly when its first word is `count`, and
every `count-` case has an `allocs` header. (A slug that merely begins with
the word, as `204-count-of-nothing-is-zero`, `254-count-of-a-map-calls-f` and
`500-count-str-chars` did, is a finding of the first test and a panic of the
second; the three are now `the-count-of-..`.)
A `count-` case whose count is 0 is allowed: nothing can be lowered below 0,
so `allocs.rs` skips the lowered half for it, and
`a_bound_of_zero_passes_when_nothing_allocates_and_fails_when_something_does`
shows with fixtures that a bound of 0 does fail a program that allocates.

| Kind | What it is |
|---|---|
| none | a unit case: a function or an interface, with its checks |
| `ref-` | the library function against a naive reference over seeded inputs |
| `law-` | a property over generated values |
| `count-` | a tight `allocs: <= N`, `N` the measured count |
| `bound-` | a deliberately loose `allocs:` bound |
| `rule-` | a row of §5 as a program, with `;; error:` or `;; result:` |
| `open-` | fails today for the items of its `;; open:` header |

`reject-` and `trap-` in a slug say what a case that expects `reject` or
`trap` shows, as in `cases/ownership`.

## Header

```
;; spec:   stdlib §4.4 (rows map filter)           the section that decides the case
;; expect: accept                                   or reject / trap, as method.md rule 3
;; result: 0                                        the integer main returns (a bit mask of failed checks for ref- and law-)
;; audit:  clean                                    or leak-cycle
;; allocs: <= 4                                     count- and bound- cases; the A lines of the free trace
;; roots:  ../../lib support                        library roots, relative to the case's directory
;; covers: map filter remove                        the Clojure-name cells of the rows of §4 this case calls
;; open:   L20                                      open- cases only: the §7 items that excuse its failure
```

`covers` names a row only if the case's code calls the row's spelling: the
test `stdlib_table` reads the table of `spec/stdlib.md` §4 and fails when a
covered name is no row, when the code never calls it, or when a case covers
more than 12 names. A row of the delivered tranche (`TRANCHE` in the test, 1 today) that no case
covers fails `every_row_of_the_tranche_is_covered`. The rows of the tranche being
written are listed by `cargo test -p fibref --test stdlib_table -- --ignored`
(`every_row_of_the_next_tranche_is_covered`, which fails until the last row is
covered); the gate of the tranche raises `TRANCHE` and removes that test. The
names are the first cell of the row, as written, without the code span and
without `(new)`.

`open` is for `accept` cases. A case with `open:` is judged as usual and then:
it fails as the label says (the checker refuses it, it traps, it answers
differently, with the two tools agreeing and a clean audit): **OPEN**, listed
with its items, counted apart, not a failure, never a pass. It passes: a
**failure**, "the item landed: remove `open`", so the label cannot outlive its
reason. It fails because the two tools disagree, or the audit finds an error:
a failure, whatever the label says. The items allowed in an `open:` line are a
list in the test (`open_cases_are_allowed`): the §7 ids still open and the
package ids of the plan a case may wait for.

## Support modules (`support/tl/`)

* `tl.rng`: `(rng seed)`, `(rng-next r)` any `i64` (xorshift64 over `shl`, `shr`,
  `bit-xor`: never traps), `(rng-below r n)` in `[0, n)`, `(rng-vec r len bound)`.
* `tl.check`: `(checks)`, `(check c ok)`, `(checks-result c)` (0 when every check
  held, else the mask of the first 62 failures, bit i the i-th check; a later
  failure sets bit 62), `(checks-made c)`, `(check-detects c planted)`.
* `tl.gen`: `(gen-char r)`, `(gen-char-of r width)`, `(gen-str r n)`: UTF-8 with
  scalars of every width, never a surrogate.
* `tl.model` (V1): the key types `Kc Kh Kd Kw` (hash id mod 7, `(id mod 4) << 20`, a
  chain, `id << 59`) with `mk-*`/`id-*` for them, for `i64` and for `str`; `run-map-seq`,
  `run-set-seq`, `run-drain` (a seeded run of a Map or Set against an id-indexed model:
  0 or the failing step); `Cover`, to assert what a run reached; `each-entry` and
  `each-elem` (the prelude's `map-each` and `set-each`).

A case never prints to compare: it computes a result.

## What each kind must show

A test that cannot fail is worse than none.

* **ref- and law-**: at most 300 seeded inputs per file (memory); the file's
  last check is a *planted fault*: the same comparison against a reference that is
  wrong in one way (an off-by-one, a swapped argument) runs in a second `Checks`,
  and `(check-detects c planted)` fails the case when the planted run found
  nothing. The seed is a constant in the file; a failure is a bit in the result.
* **count-**: `allocs: <= N` with `N` the measured count. The tests
  `a_stdlib_count_bound_one_below_the_count_fails` of `crates/fibref/tests/allocs.rs`
  and `crates/fibc/tests/allocs.rs` lower `N` by one for every `NNN-count-*.fib`
  and require the failure (method.md rule 3). The objects a `def` allocates are not in
  the trace (`cases/ownership/README.md`); keep a literal under 500 elements.
* **rule- and open-**: the program is the page's own (§5.1 to §5.5, Appendix A) and the
  header the page's error text.

## The cases of P0

* 000 `each-while` on a `Vec`, an `Option`, a `VSeq`, an `LSeq` and a `List`, with early
  exit; 001 a user tree gets `count size nth last to-vec` from one `each-while`; 002 a
  lazy node is realised once; 003 (trap) a seq that forces itself; 004 `to-lseq` of
  `to-lseq` stays lazy; 005 `conj` keeps a `Vec` a `Vec` and conses onto a `VSeq`; 006
  `seq` and `rest` of a `Vec`; 007 `truthy?` and `payload`; 008 `resolve`; 009 `zero`
  and `one` at six widths; 010 a user `Div` instance; 011 `to-str` and `debug` of a
  string; 012 `empty` of a `Vec`; 013 `get` of a `Vec`; 014 two facades through aliases
  and uses; 015 (reject) a part that uses one below it closes a cycle; 016 (trap) `nth` past
  the end; 017 a seq that refers to itself is a leak cycle; 019 the protocols with no library
  instance take a user one; 020 the support modules against values computed outside fibber;
  021 and 022 (`count-`) the allocations of a walk; 023 (trap) `payload` of nil; 024 `Step` and
  the prelude's tuples.
* 870 to 877 (the flip, step M): 870 a program with no `ns` and no `:use` sees `count map sort str get
  into println`; 871 its own `map`, `first` and `get` shadow the library's; 872 (a directory) a used
  module's `first` beats the library's; 873 (reject) `/` in a `Num` generic is `Div`'s, so at `i64` the
  quotient is a ratio; 874 `(range a b)` is a `Range`; 875 `for-each` with a function value is `run!`;
  876 the vector and map literals and `def` initialisers with the library implicit; 877 the function of
  873 at floats.
* 900 to 911 (`open-`) the failing programs of §5.5, one per item still open: S1 L20,
  S2 L21 (the collections half: `(m k)` and `(v i)`; the keyword half is case 1000, `open:` L14), S3 L22, S4 L23, S5 L24, S6 L26, S7 L1, S8 L15, S10 C9, S12 E14, S15 L29, S16 L28
  (S9, S11, S13, S14, S17 and S18 are other packages' rows). Each program was refused or
  trapped as its header says when it was written; its `result` is worked out by hand
  for the day the item lands, not run.

## The cases of Z0 (tranche 2)

* 1000 (L14, flipped by X6; its accept and reject cases are 1601-1612) `(:k x)`: a keyword in call position reads a field of a struct and the value of a
  `(Map keyword v)`, and `(map :age ps)` takes the keyword where a function is expected. This is the
  keyword half of S2; case 901 keeps the collections (`(m k)`, `(v i)`).

## Mutation review (`fibmut`)

`crates/fibmut` is the tool of the mutation reviews (plan MUT0; `spec/bootstrap.md` §3
kept the inputs of the reader's two reviews and lost the tool, so they cannot be
rerun). It mutates one library module, runs the cases that exercise it against each
mutant, and tells how every mutant ended. A mutant no case kills is a case the library
lacks; the procedure below turns it into one. The code is std only; it needs a built
`fibref` (or `fibc`) and runs from the repository root:

```
cargo build -j2 -p fibmut
fibmut --module lib/fib/seq/protocols.fib --only '00*,01*,02*' --bin PATH/fibref
fibmut --module lib/fib/core/num.fib --only '05*,06*,07*,08*,09*' --max 40 --tool fibc --bin PATH/fibc
fibmut --module lib/fib/seq/protocols.fib --only '00*,01*,02*' --lines 42 --ops const   # one line again
fibmut --module lib/fib/seq/protocols.fib --list                                        # the sites, run nothing
```

`--only` takes comma separated patterns over the case names (`*` and `?` match; a
pattern with neither is a prefix, as the tools' own `--only`; one that matches no case is
an error); without it every case of `--cases` (default `cases/stdlib`) is used. `--max N`
(default 150) runs a seeded sample (`--seed`, default 1) of the module's mutants, the
same sample for the same seed; `--timeout` (default 20 s) and `--vmem` (default
4000000 KB) limit each process; `--out DIR` writes the report and each survivor's diff;
`--verbose` lists every mutant with the case that killed it. `fibmut --help` has the rest.

**It never edits `lib/` or `cases/`.** The library and the cases are copied into a
temporary directory laid out as the repository is, so the relative `roots:` of a header
find the copy; the mutant is written there, `FIB_LIB` names the copy (a case with no
`roots` still loads it), and every process is run `ulimit -v`, `timeout` and `nice`, one
at a time, in that directory. Before any mutant:

1. the **baseline** runs every selected case on the unmutated copy; a case that does not
   pass (an `open-` case, a case that needs more `--vmem`) is dropped and listed with its
   reason. The cases that start threads fail at 4000000 KB (`657-pmap-keeps-the-order-of-its-source`
   passes at 8000000): they are dropped unless `--vmem` is raised or 0;
2. the **control** runs them again with the module made unreadable. The cases that then fail
   are the ones that load the module; with none, every mutant would survive and the run stops.

Each mutant is then checked to compile by a program that only loads the module (`invalid`
when it does not, no case run) and run against the loading cases, stopping at the first that
fails; the case that killed goes first for the next mutant. A module with no `ns` form has no
compile check: a mutant that does not compile is then killed by a case, as `compile`.

| operator | edit (only in code: a body, not a signature, a type, a pattern or quoted data) |
|---|---|
| `cmp` | `<` and `<=`, `>` and `>=`, `=` and `!=` change places |
| `arith` | `+` to `-`, `-` to `+`, `*` to `+` |
| `bool` | `true` and `false`, `and` and `or` change places; `(not x)` becomes `x` |
| `const` | an integer literal `n` to `n+1`, `n-1` and `0`, in its width and in range |
| `branch` | `(if c a b)` to `(if c b a)` and to `(if (not c) a b)` |
| `clause` | one clause of a `match` or a `cond` is deleted (most such mutants are `invalid`: not exhaustive) |
| `swap` | `(f a b)` of two variables to `(f b a)`; not for `+ * = != bit-and bit-or bit-xor`, whose result cannot change |
| `stmt` | a `(set! ..)`, or a form of a `do` before the last, becomes `()` |
| `exit` | a `true` or `false` in a tail of the callback of `(each-while c (fn ..))` flips |

The report counts the mutants killed by `result` (a wrong answer), `trap`, `audit`,
`allocs`, `compile` (a case that the checker refuses), `failed`, `timeout` and `crash` (the
tool died: a finding of its own), then survived and invalid, and the same by operator.
A survivor is printed with its diff.

For a survivor (plan MUT-P): (1) show it is not equivalent, with an input on which the two
programs answer differently; (2) write a case that kills it (`ref-` or `law-` when it was in
generic code); (3) rerun that line (`--lines N --ops OP`) and show it killed; (4) an
equivalent one is listed with the argument, and code no input reaches is deleted.
