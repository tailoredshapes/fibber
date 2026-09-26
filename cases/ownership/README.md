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
Syntax is liar's until spec/syntax.md exists; the cases will be
rewritten into fibber syntax then, with verdicts unchanged.

Cases 01 to 11 are the situations where lexical scope alone is not
enough to decide when memory is freed.
Cases 12 to 14 and 18 must be rejected; 15 is the one permitted leak;
16 shows the decided pattern for coordinated updates (§7); 17 pins the
copy-in, copy-out meaning of `&` (§5); 19 and 20 cover weak references
(§6).
