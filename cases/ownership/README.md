# Ownership cases

Each file starts with a header that fixes the expected verdict:

```
;; spec:   the section of spec/ownership.md that decides this case
;; expect: accept | reject
;; result: value main returns (accept only)
;; audit:  clean | leak-cycle (accept only)
;; error:  text the compile error must contain (reject only)
```

`accept` cases must also pass the memory audit (spec/method.md, rule 2).
The cases are written in fibber syntax as spec/syntax.md defines it;
Appendix A of that file lists all twenty and notes the four (05, 08, 15,
19) whose bodies changed in the rewrite from liar syntax. Verdicts were
not changed.

Cases 01 to 11 are the situations where lexical scope alone is not
enough to decide when memory is freed.
Cases 12, 13, 14, 18, 21, 34, 40, 82, 90, 93, 141, 142, 145, 147, 148,
149 and 150 must be rejected; 15
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

Cases 131 to 152 pin vector patterns and guards, which the owner
allowed on 2026-09-28 (syntax §1.4, §3.3, §3.6; types §2.6, §6.3, §8.3
and the decision record in types §10). They were written from the spec
before fibref implemented it, and are numbered from 131 because 101 to
130 are being added on another branch; the numbers are made contiguous
at the merge. Accept: lengths and literals (131), a rest returned (132),
stored and captured (133), released in a loop (134); guards that fail
with later clauses binding the same names (135), that store their rest
and fail (136), that read a cell an earlier guard wrote (137);
vector-in-struct (138) and struct-in-vector (139) patterns; no rest
built when a later sub-pattern fails (140); tail calls from guarded
bodies (143); forms matched by shape through `(List [..])` (144); `[&
r]` in `let` (146); an `await` in a guard (151); an element that
outlives its vector (152). Reject: a missing length (141), coverage by
guarded clauses only (142), a vector pattern on a `Form` (145), a
refutable vector pattern in `let` (147), `recur` in a guard (148), a
clause after `[& r]` (149), vector patterns mixed with `Vec`'s own
variants (150).
