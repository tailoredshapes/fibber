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
