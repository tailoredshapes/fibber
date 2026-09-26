# fibber

A Lisp with memory safety and no garbage collector. Successor to
[liar](https://github.com/tsmarsh/liar).

```
fibber source → fibber → lIR → LLVM IR → native
```

**Memory model: borrow first, count second.** Values that stay inside
the scope that created them live and die with that scope, with no
bookkeeping. Values that escape it (returned, stored, captured by an
escaping closure, sent to another thread) are reference counted. The
compiler decides which is which; the programmer writes neither
lifetimes nor retain/release.

## Status

Specification. Nothing here is implemented yet, and nothing counts as
implemented until an executable test says so.

| Part | Where | State |
|------|-------|-------|
| Method: how claims are checked | [spec/method.md](spec/method.md) | draft |
| Ownership model | [spec/ownership.md](spec/ownership.md) | draft |
| Ownership test cases | [cases/ownership/](cases/ownership/) | draft |
| Type system | spec/types.md | not started |
| Reference interpreter (the executable spec) | — | not started |
| lIR (hardened, from liar) | — | not started |
| Compiler | — | not started |
