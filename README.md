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
0.1.5 (the file `VERSION`). Download both files, then:

```
sha256sum -c --ignore-missing SHA256SUMS     # fibc-0.1.5-linux-x86_64.tar.gz: OK
tar xzf fibc-0.1.5-linux-x86_64.tar.gz
echo '(defun main () -> i64 (do (println "hello from fibber") 0))' > hello.fib
fibc-0.1.5-linux-x86_64/bin/fibc --version   # fibc 0.1.5
fibc-0.1.5-linux-x86_64/bin/fibc run hello.fib
fibc-0.1.5-linux-x86_64/bin/fibc build hello.fib -o hello && ./hello
```

The tarball holds one directory, `fibc-VERSION-linux-x86_64/`:

| Path | What |
|------|------|
| `bin/fibc` | the compiler |
| `bin/fibref` | the frozen reference interpreter, which also serves `fibref lsp` to the editor pack; present when the seed that built the release had one beside its `fibc` |
| `share/fibber/lib/` | the standard library source, found beside `bin/` by `fibc` itself |
| `LICENSE`, `README.txt` | the licence (BSD 3-Clause) and a short layout note |

No environment variable is needed: `bin/fibc` finds the library in
`share/fibber/lib` by its own location, so move the unpacked directory as
a whole (`bin/` and `share/` side by side). The code generator, lair, is
written in fibber and links LLVM 21 statically into `bin/fibc`: there is no
`lib/liblair.so`, and LLVM is not needed on your machine; it needs libc,
libm, libstdc++, libgcc_s, libz and libzstd (`ldd bin/fibc` shows those
and nothing else), and a C compiler (`cc`) for `fibc build`. To make a release
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

## Status and development

The active compiler is written in fibber: `compiler/fibc.fib` contains the
front end and emitter, `compiler/lir/` checks lIR, and `compiler/native/`
lowers it through the LLVM-C bindings in `compiler/llvm/`. The runtime
remains lIR source. The Rust tools in `crates/` are frozen as the bootstrap
seed and legacy comparison tools; new language work belongs in `compiler/`
and `lib/`.

The current release and bootstrap seed are **0.1.5**, recorded in `VERSION`
and `SEED`. The bootstrap check is a fixed point: stage 2 builds stage 3,
and both emit byte-identical lIR for the compiler's source. Equality with
an older seed's output is informational because the compiler and embedded
prelude can change. See [the CI workflow](.github/workflows/ci.yml).

Recent compiler and library changes include:

- Scalar `Option` values stored inline as a tag and payload, including in
  collection elements, closure captures and task frames. Options of ordinary
  objects remain nullable pointers; nested options and other payloads retain
  their boxed representation. See [the representation design](docs/design/unboxed-option.md).
- Last-use moves, moves of fields out of dead owned objects, and reuse of
  unique collection shells and arrays. Persistent updates preserve old
  versions when another holder exists. See [the update rules](spec/stdlib.md#25-mutation-and-uniqueness).
- Fusion of a sequence bound by `let` when it has one eligible consumer,
  with tests for effect order and preservation of memoization when reused.
- SIMD lane values, arithmetic, masks, reductions and target-dependent widths
  through `fib.simd`, with native vector lowering. See [the SIMD measurements](docs/shootout/simd.md).
- FastISel for `fibc run -O 0`, plus recorded development-loop timings.
  The compiler server, incremental checking and session work are described in
  [the development-loop design](docs/design/dev-loop.md); that design is not
  a claim that every planned command exists.

The standard library in `lib/` follows Clojure's names and argument shapes,
within fibber's static types and ownership model. Its specification and
remaining work are in [spec/stdlib.md](spec/stdlib.md). The specifications,
case headers and [ROADMAP.md](ROADMAP.md) contain both historical records
and current rules; dated amendments identify changes.

The explicit [`fib.tensor` numerical library](lib/fib/tensor/README.md) adds
typed dense tensors, checked strided views, broadcasting, eager arithmetic,
fused `axpby`, ordered and axis reductions, boolean masks, and matrix multiplication.
Floating arithmetic uses native eight-lane `f32` and four-lane `f64` kernels;
matrix multiplication uses register tiles and reusable packed panels. Explicit `sum-fast`/`dot-fast`
permit reassociated reductions. After rebuilding stage 2, try
`./F run examples/tensor.fib`; see the
library README for API, safety contracts, and reproducible NumPy comparisons.
This is a dense numerical foundation, not NumPy feature or performance parity.

The explicit [`fib.logic` relational library](lib/fib/logic/README.md) adds
finite typed terms, persistent unification with occurs checking, fair sequential
search, and `fresh`/`conde` syntax. The same relation can infer missing values or
enumerate answers; try `./F run examples/logic.fib`. Parallel search remains
future work, with this engine serving as its tested reference.

Its [`fib.logic.fd` extension](lib/fib/logic/README.md) adds compact and sparse
finite integer domains, propagation for all-different and arithmetic constraints,
and smallest-domain-first search. The port of [tsmarsh/sudoku](examples/sudoku/solver.fib)
uses those constraints; build it with `./F build examples/sudoku.fib -I examples -I lib`.
The [finite-domain design note](docs/design/finite-domains-and-sudoku.md) records
the API, provenance, validation cases, and a comparison with the original
Clojure/core.logic implementation.

### Build and validate

With a seed compiler on `PATH` and LLVM 21 development libraries installed:

```sh
fibc build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o /tmp/fibc2
/tmp/fibc2 build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o /tmp/fibc3
/tmp/fibc2 emit -I compiler -I lib compiler/fibc.fib > /tmp/fibc2.lir
/tmp/fibc3 emit -I compiler -I lib compiler/fibc.fib > /tmp/fibc3.lir
cmp /tmp/fibc2.lir /tmp/fibc3.lir
```

`scripts/fetch-seed.sh /tmp/fibber-seed` fetches and verifies the seed named
in `SEED`, and prints its compiler path. Release packaging links LLVM
statically; the local build above links `libLLVM-21` dynamically.

```sh
FIBC=/tmp/fibc2 scripts/gate.sh --quick   # ownership, modules, stdlib sample
FIBC=/tmp/fibc2 scripts/gate.sh --full    # fixed point and full case directories
/tmp/fibc2 cases cases/stdlib --only 4010 4011 4015 -j 2
/tmp/fibc2 run -O 0 program.fib          # fast development code generation
/tmp/fibc2 build program.fib -o program  # optimized native executable
/tmp/fibc2 explain program.fib          # ownership decisions
/tmp/fibc2 emit program.fib             # generated lIR
```

The stage-2 case gate checks the non-passing set against
[scripts/ci-stage2.expected](scripts/ci-stage2.expected), currently one
known failure: the `def`-initialized atom case 1707. Cases marked `open`
are counted separately; they are not passes. A changed non-passing set
fails the gate, including a known failure that starts passing.

The [October 4 decisions](docs/design/decisions-2026-10-04.md) supersede the
interpreter-oracle requirement: the interpreter is now a development
and editor tool. The frozen Rust interpreter does not implement the newer
stage-2 features. The earlier interpreter/compiler comparison rules in
[spec/method.md](spec/method.md) remain historical text pending consolidation;
use the stage-2 gate for current compiler validation. `cargo test --workspace`
checks the legacy Rust tools and requires LLVM 21 for `lair`.

## Performance

These are **recorded measurements**, not a fresh benchmark of the current
checkout. The [quick baseline](scripts/bench/baseline.tsv), dated 2026-10-04,
records a median of three runs on a 28-core host at tree stamp
`97a9ee6a6019c82f`. Outputs are checked against recorded checksums.

| benchmark | seconds | workload |
|---|---:|---|
| num-f64 | 1.46 | scalar floating-point loop |
| num-nbody | 0.59 | bodies stored in a vector, updated per step |
| vec-conj-pop | 0.90 | repeated vector append and pop |
| vec-index | 0.73 | repeated vector indexing |
| vec-sort | 0.43 | sorting and sorting by a key |
| map-assoc-get | 0.28 | map updates and lookups |
| set-conj | 0.23 | set insertion and membership |
| lazy-fused | 0.68 | directly consumed sequence pipeline |
| lazy-bound | 0.03 | sequence pipeline bound through local variables |
| strings | 0.65 | string construction, splitting, joining and search |

The older PERF0 measurements and Rust comparisons are retained in
[ROADMAP.md](ROADMAP.md#performance). They preceded the collection update
work and should not be read as current ratios to Rust. The quick baseline
is a regression reference, not evidence that all collections now match Rust.

The [SIMD shootout](docs/shootout/simd.md) records separate scalar and SIMD
fibber kernels alongside Java and C, with verified output and median-of-five
measurements. On its AVX2 host, explicit SIMD reduced n-body from 5.98 to
1.75 seconds, spectral-norm from 1.27 to 0.52, and mandelbrot from 10.82 to
2.20. Algorithms and data layouts differ between some comparisons; the
report explains those differences and the remaining gaps, including vector
loads/stores, bounds-check overhead, FMA and boxed records.

The [development-loop baseline](scripts/bench/dev-loop.tsv) records median
startup-to-result times of 108 ms for an empty program, 139 ms for hello,
and 332 ms for the medium program at `-O 0` (1,059 ms at `-O 2`). It uses
a different recorded tree, `9deee19b87eb1f7a`.

To measure your checkout without changing the recorded baselines:

```sh
FIBC=/tmp/fibc2 scripts/bench/quick.sh
FIBC=/tmp/fibc2 scripts/bench/dev-loop.sh
```

The scripts serialize benchmark and gate runs through `/tmp/fibsuite.lock`.
Use `--record` only when deliberately replacing a baseline.
