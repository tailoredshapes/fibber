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

## Install

Releases are on the [GitHub releases page](https://github.com/tailoredshapes/fibber/releases):
`fibc-VERSION-linux-x86_64.tar.gz` and `SHA256SUMS`. The 0.0.x releases are
the Rust tools; from 0.1.0 `fibc` is the compiler written in fibber, built by
itself, and stays 0.x until the owner says 1.0.0. The current release is
0.1.3 (the file `VERSION`). Download both files, then:

```
sha256sum -c --ignore-missing SHA256SUMS     # fibc-0.1.3-linux-x86_64.tar.gz: OK
tar xzf fibc-0.1.3-linux-x86_64.tar.gz
echo '(defun main () -> i64 (do (println "hello from fibber") 0))' > hello.fib
fibc-0.1.3-linux-x86_64/bin/fibc --version   # fibc 0.1.3
fibc-0.1.3-linux-x86_64/bin/fibc run hello.fib
fibc-0.1.3-linux-x86_64/bin/fibc build hello.fib -o hello && ./hello
```

The tarball holds one directory, `fibc-VERSION-linux-x86_64/`:

| Path | What |
|------|------|
| `bin/fibc` | the compiler |
| `bin/fibref` | the frozen reference interpreter, which also serves `fibref lsp` to the editor pack; present when the seed that built the release had one beside its `fibc` |
| `lib/liblair.so` | the code generator (lair, with LLVM inside), found by the rpath `$ORIGIN/../lib` |
| `share/fibber/lib/` | the standard library source, found beside `bin/` by `fibc` itself |
| `LICENSE`, `README.txt` | the licence (BSD 3-Clause) and a short layout note |

No environment variable is needed: `bin/fibc` finds `lib/liblair.so` by its
rpath and the library in `share/fibber/lib` by its own location, so move the
unpacked directory as a whole. LLVM is not needed (it is inside
`liblair.so`); the machine needs libc, libm, libstdc++, libgcc_s, libz and
libzstd, and a C compiler (`cc`) for `fibc build`. To make a release
yourself, see `scripts/package.sh`.

**Which CPU the code is for.** A release is built with `FIB_TARGET_CPU=x86-64-v2`
(`scripts/package.sh` sets it unless it is already set), so that the `fibc`
binary in the tarball runs on any x86-64 CPU with those instructions and not
only on the one that built it. `FIB_TARGET_CPU` is read by lair (`crates/lair/src/llvm/target.rs`)
whenever it generates code, so it also applies to the programs you compile: with the
variable unset, empty or `host`, `fibc run` and `fibc build` on your machine generate code
for **your host CPU** (its name and its features), and a program built that way may
not run on an older CPU. To build a program that runs elsewhere, set it:

```
FIB_TARGET_CPU=x86-64-v2 fibc build hello.fib -o hello
```

Any other value is passed to LLVM as a CPU name with no extra features.

## Editor

`editors/vscode/` is a VS Code language pack for `.fib`: a TextMate grammar
(highlighting of comments, strings, numbers, keywords, special forms, the
library's macros, `x:` annotations, reader macros), language configuration
(brackets, comments) and snippets. It has no build step; copy or symlink the
directory to `~/.vscode/extensions/tailoredshapes.fibber-0.1.0` and reload the window.
Since its 0.2.0 it also starts `fibref lsp` for completion, hover and diagnostics;
that needs `npm install` in the directory and a `fibref` on `PATH` (the tarball's
`bin/fibref`, when it has one) or the setting `fibber.fibrefPath`. The release workflow also
attaches the extension as `fibber-vscode-VERSION.vsix` to each release
(`code --install-extension FILE.vsix`). Details: `editors/vscode/README.md`.

## Performance

Measured by `scripts/bench/` (one fibber program per benchmark, each printing a
checksum; Rust twins in `scripts/bench/rust/`; `FIBC=bin/fibc scripts/bench/run.sh`).
The table is the ROADMAP's PERF0 measurement (2026-10-03, released fibc 0.1.0, median of 3,
28-core host, `rustc -O` twins), taken **before** the in-place update of collections
landed; it has not been re-measured since, and the ROADMAP's Performance section has the
profiles and the plan. Every benchmark printed the same checksum as its Rust twin.

| benchmark | fibber s | Rust s | ratio | what it does |
|---|---|---|---|---|
| num-i64 | 2.97 | 2.87 | 1.0 | loop/recur, 1e9 steps |
| num-f64 | 1.50 | 1.51 | 0.9 | f64 series, 2e9 steps |
| recursion | 2.32 | 1.36 | 1.7 | fib 44 + arity-overloaded tail call |
| binary-trees | 2.81 | 2.55 | 1.1 | depth 18, enum tree |
| dispatch | 2.66 | 0.39 | 6.8 | protocol, enum match, closures |
| vec-index | 0.83 | 0.08 | 10.3 | 1e8 `nth` |
| num-nbody | 1.38 | 0.10 | 13.7 | 5 structs in a Vec, `assoc` per body per step |
| strings | 1.08 | 0.09 | 11.9 | str, split, join, index-of |
| lazy-fused | 1.05 | 0.09 | 11.6 | fused range/map/filter/reduce |
| map-assoc-get | 1.68 | 0.14 | 11.9 | `(Map i64 i64)`, 1e6 assoc, 2e6 get |
| vec-sort | 0.92 | 0.07 | 13.1 | sort 2e6, sort-by 1e6 |
| set-conj | 0.84 | 0.05 | 16.7 | 1e6 conj, 1e6 contains? |
| vec-conj-pop | 1.64 | 0.06 | 27.2 | 2e7 conj, 2e7 pop |
| lazy-bound | 1.48 | 0.03 | 49.1 | the same chain, each stage bound by `let` |
| vec-assoc | 3.36 | 0.01 | 333 | 1e7 `assoc` on a unique 1e5 Vec |
| set-disj | 3.38 | under 0.01 | n/m | 2e4 conj then 1e4 disj; quadratic; `dissoc` was changed in `bea0905`, not re-measured |

Scalar code is at Rust's speed; what touches a collection was 7x to 330x slower, because no
update was in place. Since then the in-place primitives, the last-use analysis and the library
over them have landed (spec/stdlib.md §2.5, "When is an update in place"); their effect is to be
measured, not claimed here.

## Status

**The compiler is bootstrapped** (M6, 2026-10-02). `compiler/fibc.fib` is a
fibber compiler written in fibber: reader, expander with a macro runner,
type checker, ownership checker and lIR emitter, about 32,000 lines. The
Rust `fibc` (stage 1) builds it into stage 2; stage 2 builds itself into
stage 3; all three emit byte-identical lIR for the compiler's own source,
and stage 3 emits the same lIR as stage 1 on every program of the case
suite. Native code still comes from `lair` and LLVM through a C interface
(`liblair.so`), the runtime `fib.rt` is lIR text, and the test harnesses are
Rust. The standard library (M7) is a Clojure-shaped library in `lib/`,
implicit in every program; its second tranche is under way. Nothing counts
as implemented until an executable test says so ([spec/method.md](spec/method.md)).

```
cargo test --workspace                          # the full suite (lair needs LLVM 21)
cargo run -p fibref -- cases cases/ownership    # 243 cases
cargo run -p fibref -- cases cases/modules      # programs of several modules, a directory each
cargo run -p fibref -- run   <file.fib>         # result and memory audit
cargo run -p fibref -- explain <file.fib>       # the ownership decisions
cargo run -p fibref -- cases cases/stdlib       # the standard library's cases (header keys: cases/stdlib/README.md)
cargo run -p fibref -- read [--print] <file>..  # the reader's dump or printed forms (spec/bootstrap.md §2)
cargo run -p fibref -- expand|types|own <file>..  # the dumps of the expander, type checker and ownership checker (spec/bootstrap.md §5 to §7)
cargo run -p lair -- cases cases/lir            # 323 lIR cases, JIT and AOT
cargo run -p lair -- run   <file.lir>           # JIT-compile and run main
cargo run -p lair -- build <file.lir> -o out    # native executable
cargo run -p lair -- check <file.lir>           # the checker alone
cargo run -p lair -- fuzz cases/lir --count N   # mutation fuzzer over the accept cases (spec/lir.md §10.1)
cargo run -p fibc -- cases cases/ownership      # every case interpreted and compiled, traces compared (method rule 6)
cargo run -p fibc -- run   <file.fib> [-- a b]  # compile through the JIT and run main; a b are (args)
cargo run -p fibc -- build <file.fib> -o out [-L dir].. [-l lib]..  # native executable, linked with the libraries named
cargo run -p fibc -- gen --seed S --count N     # N generated programs through the same harness (method rule 5)
cargo run -p fibc -- emit <file.fib>            # the lIR module; emit-dump prints it by section (spec/bootstrap.md §8)
```

The compiler in fibber is a program like any other. Build it with stage 1
and use it as `fibc`:

```
fibc build compiler/fibc.fib -I compiler -I lib -L target/debug -l lair -o fibc2
./fibc2 build compiler/fibc.fib -I compiler -I lib -L target/debug -l lair -o fibc3   # it compiles itself
./fibc3 emit compiler/fibc.fib | cmp - <(fibc emit compiler/fibc.fib)               # byte-identical lIR
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
| Cases | [cases/ownership/](cases/ownership/) | 243, all passing under both tools (the language, ownership, closures, threads, macros, the prelude's data structures; the numbering and the findings behind each block are in the case headers) |
| Module cases | [cases/modules/](cases/modules/) | 27 programs of several modules (syntax §5), each a directory with its `main.fib`, all passing both ways |
| Reference interpreter `fibref`: audited heap, reader, expander, types, ownership checker, evaluator | [crates/fibref](crates/fibref) | done (M2, [ROADMAP.md](ROADMAP.md)) |
| Random program generator `fibgen` (method rule 5) | [crates/fibgen](crates/fibgen) | done (M2) |
| Library | [lib/](lib), design [spec/stdlib.md](spec/stdlib.md) | M7, **Proposed**, tranches 0 and 1 done and tranche 2 under way (2026-10-03): the prelude (`Vec`, `Map` and `Set`, `Result`, tasks, files) plus four implicit facades `fib.core fib.seq fib.coll fib.print` of about forty parts: protocols (`Reducible`, `Seqable`, `Lookup`, ...), fused adaptors and memoised lazy seqs, sorting, strings and formatting, `defrecord`, numeric conversions, endless sources, cursors; verified against real Clojure 1.12 where behaviour is Clojure's |
| Standard library cases | [cases/stdlib/](cases/stdlib/) | 912 cases, 881 passing under both tools, 31 `open-` (items of later tranches): reference, law, count (allocation bounds), trap and reject cases per function |
| lIR: the assembler for LLVM IR that `fibc` emits | [spec/lir.md](spec/lir.md) | decided (owner, 2026-09-28; the second M3 pass's additions decided the same day, §14 items 8 to 11) |
| lIR cases | [cases/lir/](cases/lir/) | 323, all passing on both paths (instr: each instruction; mapping: the shapes of types §8; audit: liar's findings re-established; adversarial, the fuzzer's findings among them; verify: one reject case per rule) |
| lIR checker `lir` (no LLVM) and `lair`: JIT, AOT, case harness | [crates/lir](crates/lir), [crates/lair](crates/lair) | done (M3) |
| Compiler `fibc`: `fibref`'s front end lowered to lIR, the runtime `fib.rt`, the rule-6 harness, macros and `def`s through the JIT, `async` as state machines | [spec/compiler.md](spec/compiler.md), [crates/fibc](crates/fibc) | done (M4, decided 2026-09-30): all 191 cases pass interpreted and compiled with matching free traces (`fibc cases cases/ownership`, 2026-10-01), and generated programs run through the same harness (`fibc gen`) |
| The C interface to `lair` (`liblair.so`), used by the compiler written in fibber | [spec/compiler.md §9](spec/compiler.md), [crates/lair/include/lair.h](crates/lair/include/lair.h), [crates/lair/src/capi](crates/lair/src/capi), bindings in [compiler/lair/](compiler/lair) | M6; 22 `lair_*` functions, the list **Proposed**; a test keeps the header equal to the exports; `fibc build FILE -o OUT -L DIR -l lair` links a fibber program against it and writes the rpath. A stopgap: `lair` is to be rewritten in fibber later |
| Bootstrap: the compiler written in fibber | [spec/bootstrap.md](spec/bootstrap.md), [compiler/](compiler) | M6 done as far as the definition goes (2026-10-02, **Proposed**): `compiler/fibc.fib` (commands `emit`, `build`, `run`, `explain`, `emit-dump`) over `compiler/{syntax,expand,macros,types,own,emit,lair,driver}`. Each pass equals its Rust oracle byte for byte on the whole corpus (the reader, expander, `fibref types`, `fibref own` and `explain`, `fibc emit-dump`); stage 1, 2 and 3 emit identical lIR for the compiler itself; open: the Rust test harness cannot yet use stage 3 as the compiler, and 8 reflection-error inputs differ in position between the JIT and the interpreter ([ROADMAP.md](ROADMAP.md)) |
