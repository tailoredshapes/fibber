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

**Goal: a self-hosting language.** A fibber compiler written in fibber
that compiles itself. See [ROADMAP.md](ROADMAP.md) for the milestones on
the way; a small working language is one of them, not the destination.

## Status

The reference interpreter runs: every case passes with a clean memory
audit. Nothing counts as implemented until an executable test says so.

```
cargo test --workspace                          # the full suite
cargo run -p fibref -- cases cases/ownership    # 78 cases
cargo run -p fibref -- run   <file.fib>         # result and memory audit
cargo run -p fibref -- explain <file.fib>       # the ownership decisions
```

| Part | Where | State |
|------|-------|-------|
| Method: how claims are checked | [spec/method.md](spec/method.md) | decided |
| Ownership model | [spec/ownership.md](spec/ownership.md) | decided |
| Syntax | [spec/syntax.md](spec/syntax.md) | decided |
| Type system and ownership checker | [spec/types.md](spec/types.md) | decided |
| Cases | [cases/ownership/](cases/ownership/) | 78, all passing |
| Reference interpreter `fibref`: audited heap, reader, expander, types, ownership checker, evaluator | [crates/fibref](crates/fibref) | working; see [ROADMAP.md](ROADMAP.md) M2 |
| Library | [lib/prelude.fib](lib/prelude.fib) | what the cases need |
| lIR (hardened, from liar) | — | not started (M3) |
| Compiler `fibc` | — | not started (M4) |
