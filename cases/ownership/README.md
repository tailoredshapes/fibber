# Ownership cases

Each file starts with a header that fixes the expected verdict:

```
;; spec:   the section of spec/ownership.md that decides this case
;; expect: accept | reject | trap
;; result: value main returns (accept only)
;; audit:  clean | leak-cycle (accept only)
;; error:  text the compile error must contain (reject only)
;; trap:   text the run-time trap must contain (trap only)
```

`accept` cases must also pass the memory audit (spec/method.md, rule 2).
`trap` cases must type-check and pass the ownership checker, then trap
at run time with a message containing the `trap` text. A trap aborts
the program (spec/types.md §2.11), so the objects live at it are not
leaks; the audit still fails the case on a use-after-free, double free
or negative count before the trap, or on a live object holding a
reference to a freed one at it. A trap case's file name contains
`-trap-`, as a reject case's contains `reject`.
The cases are written in fibber syntax as spec/syntax.md defines it;
Appendix A of that file lists all twenty and notes the four (05, 08, 15,
19) whose bodies changed in the rewrite from liar syntax. Verdicts were
not changed.

Cases 01 to 11 are the situations where lexical scope alone is not
enough to decide when memory is freed.
Cases 12, 13, 14, 18, 21, 34, 40, 82, 90, 93, 105, 107, 108, 109,
112, 113, 114, 118, 119, 121, 123, 125, 126, 127, 138, 139, 142, 144,
145, 146 and 147 must be rejected; 101 to 104 must trap; 15
and 80 are the permitted cycle leaks; every other case is accept with a
clean audit. 16 shows the decided pattern for coordinated updates (§7); 17
pins the copy-in, copy-out meaning of `&` (§5); 19 and 20 cover weak
references (§6).

Cases 21 to 80 came from the adversarial review of the spec (their
reasoning and count traces are in spec/drafts/PROPOSED_CASES.md); 30 and
35 were withdrawn when D1 removed field places.

Cases 81 to 95 are the findings of the adversary of spec/method.md rule
4, which attacked the running interpreter given only the spec; each
header's comment says what the adversary found. Five needed decisions
by the owner (2026-09-27, recorded in spec/types.md §10): 82 (`weak` of
an `Option` is rejected), 86 (`range` takes one or two arguments), 87
(`(Weak (dyn P))` is supported), 89 (an object that had a weak
reference is copied on update) and 90 (`dyn` of a scalar is rejected);
94 pins the arithmetic decided with them (types §2.12). 81 also stands
for the decided note that `swap!` may not terminate under contention
(ownership.md §7). Some of them test the reference implementation
(syntax, reflection, the calling convention) rather than a section of
ownership.md; their `spec:` line cites the chapter that decides them.

Cases 96 to 99 are findings of the random program generator of
spec/method.md rule 5 (`crates/fibgen`), rewritten by hand from its
minimised programs: 96 to 98 a self-named `fn` literal whose value flows
on through a `do`, a branch or a `let` body, which types §6.5 makes an
escaping heap closure whatever its self calls; 99 stack lifetimes that
do not nest (a step's temporary ending before a later `let` binding,
types §6.11, §8.2). The seeds their headers name are those of the
generator as it was then; it has since stopped generating products
that overflow (types §2.12), which changed the programs of many seeds.
100 pins annotated `let`, `loop` and `plet` bindings (syntax §1.5).

Cases 101 onwards pin the owner's decisions of 2026-09-28 (spec/types.md
§10, "Decided on lifting the v1 restrictions"). 101 to 104 are the
first `trap` cases: overflow of `+` at `i8` with objects live at the
abort, the minimum of `i64` divided by -1, division by zero on a
spawned thread, and `rem` of the minimum of `i32` by -1 (types §2.12).
105 rejects a float literal of width `:f16` built by a macro (syntax
§1.1). 106 to 112 cover `(dyn P :send)` (types §2.15, §5.1): a
heterogeneous vector of sendable dynamic values through `pmap` (106),
a weak reference to one and an atom holding one crossing threads
(110), and the explicit conversion to `(dyn P)` (111) are accepted;
hiding a value that reaches a cell (107), capturing a plain `(dyn P)`
in a spawned closure (108), a generic function hiding a cell holder
(109) and converting a `(dyn P)` back to `(dyn P :send)` (112) are
rejected. 113 to 116 cover private names (syntax §5, §3.20): a
program naming a private prelude helper (113) or matching a variant of
the prelude's private trie enum (114) is rejected; a program's own
private definitions, a public name equal to a private prelude one and
`(var fib.prelude/x)` (115), and a macro whose expansion reaches a
private helper through `var` whatever the use site binds locally (116)
are accepted. 117 to 123 cover supertraits and default methods (types
§4.1): an `(Ord t)` bound that lets a body use `=`, and impls completed
by the built-in defaults of `Eq` and `Ord` (117), supertrait and
default methods through a `dyn`, with an upcast (120), and a default
whose names resolve where its protocol is defined whatever the program
shadows (122) are accepted; an `impl Ord` without an `impl Eq` (118),
one whose context does not entail its `Eq` impl's (119), a default that
makes a `:borrow` parameter escape (121) and a default that needs an
instance the impl's type lacks (123) are rejected. 124 to 127 cover
colour parameters on structs and enums (types §1.3): one definition
holding a sendable closure in one instance and a local one in another,
through generic functions too (124), is accepted; a local instance
crossing a thread (125), a colour argument used covariantly, which
would let a local closure be read back as sendable (126), and a local
closure given where the annotation fixes `:send` (127) are rejected.

Cases 128 to 149 pin vector patterns and guards, which the owner
allowed on 2026-09-28 (syntax §1.4, §3.3, §3.6; types §2.6, §6.3, §8.3
and the decision record in types §10). They were written from the spec
before fibref implemented it (numbered 131 to 152 on their branch and
renumbered at the merge). Accept: lengths and literals (128), a rest
returned (129), stored and captured (130), released in a loop (131);
guards that fail with later clauses binding the same names (132), that
store their rest and fail (133), that read a cell an earlier guard
wrote (134); vector-in-struct (135) and struct-in-vector (136)
patterns; no rest built when a later sub-pattern fails (137); tail
calls from guarded bodies (140); forms matched by shape through `(List
[..])` (141); `[& r]` in `let` (143); an `await` in a guard (148); an
element that outlives its vector (149). Reject: a missing length (138),
coverage by guarded clauses only (139), a vector pattern on a `Form`
(142), a refutable vector pattern in `let` (144), `recur` in a guard
(145), a clause after `[& r]` (146), vector patterns mixed with `Vec`'s
own variants (147).

Cases 150 to 153 pin the owner's decision of 2026-09-28 that the copy-in
of an `&` argument happens at call entry, after all of the call's
arguments have been evaluated (ownership.md §5; syntax §2, §3.13; types
§6.6 and the decision record in types §10). They were written before
fibref implemented it. All accept with a clean audit: the rule-5
generator's minimised program, whose forwarded cell a later argument
writes (150); the same call on a `let` cell, which copies in and must
agree with it (151); a later argument writing the variable through a
local closure that captures it (152); two `&` arguments with a later
argument writing the first one's variable (153).

Case 154 pins the owner's decision of 2026-09-28 that the built-in
`Eq` and `Ord` instances of the scalar types define every comparison
directly (types §2.12, §8.12 and the decision record in types §10):
floats compare as IEEE 754, every comparison with a NaN false except
`!=`, directly and through a generic `Ord`-bounded function, while a
user `impl Ord` that gives only `<` takes the defaults for `<=` and
`>=`, which are true on a NaN.

Cases 155 to 161 pin the owner's decision of 2026-09-28 that a colour
variable in an `impl` head is rigid in the method bodies, and that a
head may give a colour instead (types §1.3, §4.1, §5.4; syntax §3.10;
the decision record in types §10). They were written before fibref
implemented it. Reject: a body storing a closure over a cell into
`self`'s cell field at the rigid colour `k`, which the old rule
("treated as `local`") accepted and which failed the audit with
`SharedCell` (155); `self` passed as a `(Hook :local)` in a rigid body
(157); the instance for `(Hook :local)` used on a `(Hook :send)` (159).
Accept: a new `Hook` joined with `self`, and a `Self` result built from
`self`'s closure, at both colours (156); the body of 157 under a
`(Hook :local)` head (158); an enum under a `:send` head spawning
`self`'s closure (160); a rigid body storing a sendable closure into
`self`'s cell field, used at both colours (161).

Cases 162 to 165 pin the owner's decision of 2026-09-28 that an `&`
parameter that another argument of the same call captures is not
forwarded at a call in tail position but copied in and written back,
which makes the call ordinary (ownership.md §5; syntax §3.13; types
§6.6, §6.10 rule (b) and the decision record in types §10). They were
written before fibref implemented it. All accept with a clean audit: a
closure over the parameter passed beside it (162, 11; forwarded it gave
77), the same closure bound by a `let` before the call (163, 11), a
million-deep self recursion that forwards the parameter, with a closure
over it called before the call rather than passed to it, still a tail
call (164), and the capturing closure at a call not in tail position,
unchanged by the decision (165, 21).

Cases 166 to 168 close the known gap of M2: a spin-wait on an atom
that another thread sets hung under the earlier executor, which ran a
spawned thread to completion before its spawner continued. The
reference interpreter's executor is now fair and still deterministic
(types §8.8, "The reference interpreter's schedule": threads switch at
scheduling points, round robin with a quantum). All accept with a
clean audit, and each has one result under every fair schedule: a
spawned thread spinning until main sets the atom it waits on (166, 7);
two threads handing a token back and forth through an atom ten times
each while main spins until both are done (167, 349525); two threads
each adding 1 twenty times with a `swap!` whose `f` outlasts a quantum,
so that under this schedule the other thread's update lands while `f`
runs and the compare fails and retries (168, 40).

Case 169 pins the native `Show` and `Hash` instances (types §2.12) on
the integers, `bool`, `char`, a field-less enum, a keyword, `str`, the
floats and unit, as the reference interpreter computes them: `show`
gives a fresh `str` each time; `hash` of an integer is its value, of a
`bool` or `char` its code, of a field-less enum its variant index, of a
keyword the FNV-1a of its name and of a `str` of its bytes, of a float
the bits of its value as an `f64`, of unit 0. The result folds every
text's length and every hash (169, 1209), so a compiler that differs
in any text or hash fails it. `show` of a float and of a `str` are not
exercised: their text is undecided (compiler.md §8 item 11).

Cases 170 to 173 pin what `fibc gen` (compiler.md §5: generated
programs through the rule-6 harness) found in its first 660 seeds,
each reduced by hand to the shape that fails. The interpreter was
right every time; the compiler was not. An `async` inside a method of
a protocol named `Rank` (170, 9): the task's resume function named the
body's code with every `R` replaced by the result type. A generic
function whose inferred bound is met by a `(dyn Score)` (171, 12): the
bound is discharged by the receiver's own vtable (types §3.3), not by
an instance. A division by the literal `0` (172, trap): lIR refuses a
constant zero divisor, so the trap is emitted alone, as a call that
does not return. `trap` where a value is needed (173, trap): what the
lowering emits after a call that does not return goes into blocks
nothing reaches, which the builder now discards.

Cases 174 to 176 pin the state machine an `async` body compiles to
and its executor (types §8.8; compiler.md §8 item 3), all accept with
a clean audit: a loop that awaits a fresh, pending task on every turn,
with a string, the loop's variables and the awaited result live across
each await (174, 64); one task awaited by two other tasks and joined by
main, so its waiter list holds two registrations and every count on
it is released (175, 24); a scope-local struct made before an await
and read after it, whose scope is the task's and which therefore
lives in the task's frame when compiled (176, 7).

Case 177 pins a `dyn` over a native instance (types §2.12, §8.5): a
`(dyn Hash)` over a `str` beside one over a struct, in one vector; the
str's slot is a function the compiler emits around the native method
(177, -680: the FNV-1a of "abc" folded by `rem`, plus 5).

Case 178 pins the texts of `show` on floats and on `str` (types
§2.12, Decided 2026-09-30): a `str` shows as itself, unquoted; a float
as the shortest decimal that reads back at its width, positional,
with `.0` when integral, and `NaN`, `inf`, `-inf`; eleven texts each
checked against the expected one, their lengths summed (178, 75).

Cases 179 to 182 pin the prelude's `Map` and `Set` (M5; types §2.13,
syntax §4.5), an HAMT over the keys' hash, all accept with a clean
audit: a map literal, `assoc` of new and existing keys, `get`, `count`,
and the trie growing past one node, keys 0, 32 and 1024 sharing their
low five bits (179, 1738); keys whose hashes all collide, told apart by
`Eq` in one bucket, and `str` keys (180, 479); `dissoc` down to the
empty map, `map-put!` and `map-del!` through `&`, `for-each` over the
entries, `contains?` (181, 1927); a `Set` with `conj`, `set-contains?`,
`disj`, `count` and `for-each` (182, 1029).

Cases 183 and 184 pin the string building of M5 (syntax §4.3, §4.5):
`str-join` over a vector of parts, `str-chars` decoding one-, two-,
three- and four-byte characters, `char->str` encoding them back, and
`str-from-bytes` as the inverse of `str-bytes`, every text checked
(183, 129495); and `str-from-bytes` of bytes that are no UTF-8, a trap
with the same message on both sides (184).

Cases 185 and 186 pin the program's surroundings (M5; syntax §4.3,
§4.5): `write-file` and `read-file` round-tripping a text under /tmp,
`nil` for a path that cannot be read and `false` for one that cannot
be written (185, 124); and `(args)`, none under the harness so their
count is 0, after a `println` that the harness's compiled side reads
past (186, 0; `fibc run FILE -- a b` gives 2, `crates/fibc/tests/cli.rs`).

Case 187 pins the text of `show` on floats where the first `%.*e`
that reads back is not the shortest nearest text (types §2.12): exact
ties, which go up and not to even (2^-25, 2^-24, 222507385850720.125,
1125899906842624.25 and, at f32, 2^-12 and 2097152.25), powers of two,
whose rounding interval is narrower below than above (2^-44, 2^-77,
2^-1017 and, at f32, 2^90), and 0.1 + 0.2; twelve texts, each hit
counting 1 (187, 12; the runtime that took the first `%.*e` that read
back scored 1).

Case 188 pins `read-file`'s failures (syntax §4.3): `nil` for a
directory, which opens and then fails to read, for a path that does not
exist and for a path with a NUL in it (which names no file, not the
file before the NUL), `(some "")` only for a file that is really empty,
a repo file read whole from the repo root or a crate's directory, and
`write-file` of a NUL path false (188, 111111; the runtime that sized
the read with ftell crashed on a directory where lseek says LONG_MAX
and answered `(some "")` where it says 0).

Case 189 pins the `strtod` and `strtof` externs that the reader's
number.fib declares (syntax §3.15), in the interpreter as compiled:
overflow to infinity, `2.5e-3`, 0.1 at f32 against f64, a tie and a hair
above a tie at f32, trailing garbage and the end pointer's offsets, no
number, -Infinity, nan, a null end pointer, `1e` and `.5.`, and 1e39 at
both widths (189, 9250558984191; the interpreter that had only `write`
stopped at `extern strtod is not available`, and one that rounded
through f64 for strtof scored 8 less).

Case 190 pins that `(alloc n)` is `n` zero bytes in the interpreter as
compiled (syntax §3.15, **Decided**, owner, 2026-10-01): it stores a
non-zero byte in every position of blocks of 1 to 65536 bytes (the
8-byte store-i64, free, alloc, load-i64 pattern; sizes that are not a
multiple of eight; a page and more; four blocks held together and
recycled), frees them, allocates the same sizes again and counts the
bytes that read zero, so each non-zero byte lowers the result from 196631
(190, 196631; compiled with plain `malloc` it gave between 196 and 231
over four runs, the allocator's own links and the program's old bytes
being what a freed block comes back with).

Case 192 is the normal path of `println` (syntax §4.5, M6): 64 short
lines and one of 81920 bytes (more than a pipe holds), written by the
prelude's loop of write(2) calls, result 81984 (192). The traps
(`println: write failed` on a full device, a write cut short and then
failing) and the byte-for-byte output, whole and seven bytes at a time,
are checked by `crates/fibc/tests/cli.rs` and
`crates/fibref/tests/run_io.rs`, which a case cannot do; they also run
the standard-error twin of the case, because a case cannot use `eprintln`
(under the rule-6 harness the compiled trace shares standard error, and
the text of an `eprintln` was glued to a trace record, `err 0F 274`, and
spoiled the comparison). There is no case 191: `(args)` with a word that is
not UTF-8 needs a command line, which the case harness does not give, so
`crates/fibc/tests/cli/args.rs` and `crates/fibref/tests/run_io.rs` are
the evidence for it (case 186 has the empty `(args)`).

Case 193 pins that a raw `ptr` is uncounted wherever it is held (types
§8.1, `ptr` is a scalar): one 24-byte block whose first word is 1000, as
an object's count is, goes into each of 17 kinds of container (a struct
on the stack and on the heap, an enum variant, a generic struct, a `Vec`
grown past a leaf, a vector pattern with a rest, an `Array`,
`array-set!`, an escaping closure, an `Option`, a `Cell`, a `dyn`,
`set-field!` on a shared struct, a `Map`, a generic function, a `Vec`
of structs, and a task that holds it in its frame across an `await` and
returns it). The word is read while the container lives and after it is
dropped; each reading is 1 when the word is still 1000, two bits for
each probe, so the result is 4^17 - 1 = 17179869183 and a probe that
retained or released lowers it in its own bits (compiled with `ptr`
taken for an object pointer, each of the 17 had a reading off and the
result was 10339289685). An `Atom` of `ptr`, a `spawn` that returns one
and an `async` that captures one are rejected by the checker, a `ptr`
not being `Send` (types §5.3), so no probe holds one.

Case 194 pins the float bit casts (syntax §4.3, types §2.12):
`f64->bits`, `bits->f64`, `f32->bits` and `bits->f32` keep every bit,
in the interpreter and compiled. Eighteen probes, each a function that
compares what the casts made with a pattern or a float written in the
case (the patterns were packed by Python's `struct`, not by these casts),
set their own bit of the result, 2^18 - 1 = 262143 when all hold: 1.0,
0.1, -0.0, 5e-324, the maximum, the infinities and a negative value at
both widths, NaN payloads (quiet, negative, signalling) at both widths,
400 patterns of a xorshift generator at run time, with and without the
exponent forced to all ones, and NaNs through the fields of a struct,
a generic function and a `Vec`, and macros that read the bits of a
float literal and build a float literal from bits at expansion time
(the interpreter's macro evaluator, and the JIT `fibc` expands with).
The signalling `f32` NaN is the probe that matters: the interpreter
holds an `f32` widened to `f64`, and the hardware conversion quiets it
(an interpreter that widened with `as` answered 151551 here, probes 12,
13, 15 and 16 failing).
