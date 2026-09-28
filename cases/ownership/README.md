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
112, 113, 114, 118, 119, 121 and 123 must be rejected; 101 to 104 must trap; 15
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
instance the impl's type lacks (123) are rejected.
