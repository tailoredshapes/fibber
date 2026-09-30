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
audit. lIR, the assembler the compiler will emit, is specified,
implemented and checked on both its paths. Nothing counts as
implemented until an executable test says so.

```
cargo test --workspace                          # the full suite (lair needs LLVM 21)
cargo run -p fibref -- cases cases/ownership    # 184 cases
cargo run -p fibref -- cases cases/modules      # programs of several modules, a directory each
cargo run -p fibref -- run   <file.fib>         # result and memory audit
cargo run -p fibref -- explain <file.fib>       # the ownership decisions
cargo run -p lair -- cases cases/lir            # 323 lIR cases, JIT and AOT
cargo run -p lair -- run   <file.lir>           # JIT-compile and run main
cargo run -p lair -- build <file.lir> -o out    # native executable
cargo run -p lair -- check <file.lir>           # the checker alone
cargo run -p lair -- fuzz cases/lir --count N   # mutation fuzzer over the accept cases (spec/lir.md §10.1)
cargo run -p fibc -- cases cases/ownership      # every case interpreted and compiled, traces compared (method rule 6)
cargo run -p fibc -- run   <file.fib> [-- a b]  # compile through the JIT and run main; a b are (args)
cargo run -p fibc -- gen --seed S --count N     # N generated programs through the same harness (method rule 5)
```

`lair` links LLVM 21 statically through llvm-sys: set
`LLVM_SYS_211_PREFIX` to an LLVM 21 install that has `llvm-config`
(apt.llvm.org's `llvm-21-dev`; see `.github/workflows/ci.yml`).

| Part | Where | State |
|------|-------|-------|
| Method: how claims are checked | [spec/method.md](spec/method.md) | decided |
| Ownership model | [spec/ownership.md](spec/ownership.md) | decided |
| Syntax | [spec/syntax.md](spec/syntax.md) | decided |
| Type system and ownership checker | [spec/types.md](spec/types.md) | decided |
| Cases | [cases/ownership/](cases/ownership/) | 184, all passing (128–149: vector patterns and guards; 150–153: copy-in at call entry; 154: IEEE float comparisons; 155–161: colour parameters in impl heads; 162–165: no forwarding of a captured `&` parameter; 166–168: spin-waits and `swap!` contention on the fair executor; 169: the native `Show` and `Hash` instances on scalars; 170–173: findings of `fibc gen`; 174–176: the state machine of `async` and its executor; 177: a `dyn` over a native instance; 178: the texts of `show` on floats and `str`; 179–182: the prelude's `Map` and `Set`; 183–184: `str-from-bytes`, `str-join`, `str-chars`, `char->str`; 185–186: `read-file`, `write-file`, `args`, `println`) |
| Module cases | [cases/modules/](cases/modules/) | 6 programs of several modules (syntax §5), each a directory with its `main.fib`, all passing both ways |
| Reference interpreter `fibref`: audited heap, reader, expander, types, ownership checker, evaluator | [crates/fibref](crates/fibref) | done (M2, [ROADMAP.md](ROADMAP.md)) |
| Random program generator `fibgen` (method rule 5) | [crates/fibgen](crates/fibgen) | done (M2) |
| Library | [lib/prelude.fib](lib/prelude.fib) | M5 in progress: `Vec` (a 32-way trie), `Map` and `Set` (an HAMT), `List`, iterators, string building and characters, tasks, `println`, `eprintln`, files and `args` |
| lIR: the assembler for LLVM IR that `fibc` emits | [spec/lir.md](spec/lir.md) | decided (owner, 2026-09-28; the second M3 pass's additions decided the same day, §14 items 8 to 11) |
| lIR cases | [cases/lir/](cases/lir/) | 323, all passing on both paths (instr: each instruction; mapping: the shapes of types §8; audit: liar's findings re-established; adversarial, the fuzzer's findings among them; verify: one reject case per rule) |
| lIR checker `lir` (no LLVM) and `lair`: JIT, AOT, case harness | [crates/lir](crates/lir), [crates/lair](crates/lair) | done (M3) |
| Compiler `fibc`: `fibref`'s front end lowered to lIR, the runtime `fib.rt`, the rule-6 harness, macros and `def`s through the JIT, `async` as state machines | [spec/compiler.md](spec/compiler.md), [crates/fibc](crates/fibc) | done (M4, decided 2026-09-30): all 184 cases pass interpreted and compiled with matching free traces, and generated programs run through the same harness (`fibc gen`) |
