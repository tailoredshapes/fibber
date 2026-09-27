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

Specification plus the test infrastructure that will judge it. Nothing
counts as implemented until an executable test says so.

```
cargo test --workspace                    # 485 tests
cargo run -p fibref -- cases cases/ownership   # 20 cases, all pending until an interpreter exists
```

| Part | Where | State |
|------|-------|-------|
| Method: how claims are checked | [spec/method.md](spec/method.md) | draft |
| Ownership model | [spec/ownership.md](spec/ownership.md) | draft |
| Ownership test cases | [cases/ownership/](cases/ownership/) | draft |
| Syntax | [spec/syntax.md](spec/syntax.md) | draft under adversarial review; 16 open decisions |
| Type system and ownership checker | [spec/types.md](spec/types.md) | draft under adversarial review; 17 open decisions |
| Audited heap and case harness (`fibref`) | [crates/fibref](crates/fibref) | done: 485 tests, adversarially tested |
| Reference interpreter (evaluator over the heap) | — | not started |
| lIR (hardened, from liar) | — | not started |
| Compiler | — | not started |
