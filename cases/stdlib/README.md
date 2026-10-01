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
(`no case matches 999-`), never an empty pass. A run ends with the counts
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

A case lists the facades it needs, because no module is implicit until the
plan's step M flips the list:

```
(ns main (:use fib.core fib.seq fib.coll fib.print))   ; or the subset it needs
```

Three things a case may not do before the flip, each decided by a probe in
the tranche 1 plan: write `/` on integers (it is `Div`'s method in every
module that `:use`s `fib.core`; write `quot` once R3 has landed); write
`list` or match `(empty)` and `(cons ..)` in a module that `:use`s `fib.coll`
(its `empty` method is the `List` variant's name until R2 renames the
variants `Empty` and `Cons`); name a part (`fib.seq.vseq`) where a facade
will do.

## Numbering

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

## Kinds

The kind is the first word of the slug after the number.

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
more than 12 names. A row of the delivered tranche that no case covers is
listed by `cargo test -p fibref --test stdlib_table -- --ignored`, which the
final gate of the tranche runs without `--ignored`. The names are the first
cell of the row, as written, without the code span and without `(new)`.

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
  the end; 017 a seq that refers to itself is a leak cycle; 018 (reject) integer `/` until
  `Div` has integer instances; 019 the protocols with no library instance take a user one;
  020 the support modules against values computed outside fibber; 021 and 022 (`count-`) the
  allocations of a walk.
* 900 to 911 (`open-`) the failing programs of §5.5, one per item still open: S1 L20,
  S2 L21, S3 L22, S4 L23, S5 L24, S6 L26, S7 L1, S8 L15, S10 C9, S12 E14, S15 L29, S16 L28
  (S9, S11, S13, S14, S17 and S18 are other packages' rows). Each program was refused or
  trapped as its header says when it was written; its `result` is worked out by hand
  for the day the item lands, not run.
