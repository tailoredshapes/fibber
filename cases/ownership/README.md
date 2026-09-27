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
Cases 12, 13, 14, 18, 21, 34 and 40 must be rejected; 15 and 80 are
the permitted cycle leaks; every other case is accept with a clean
audit. 16 shows the decided pattern for coordinated updates (§7); 17
pins the copy-in, copy-out meaning of `&` (§5); 19 and 20 cover weak
references (§6).

Cases 21 to 80 came from the adversarial review of the spec (their
reasoning and count traces are in spec/drafts/PROPOSED_CASES.md); 30 and
35 were withdrawn when D1 removed field places.
