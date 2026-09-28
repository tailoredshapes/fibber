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
