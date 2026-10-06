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
`fibc-VERSION-linux-x86_64.tar.gz` and, from 0.1.7, `fibc-VERSION-darwin-arm64.tar.gz`
(Apple Silicon), with `SHA256SUMS`. The 0.0.x releases are
the Rust tools (tag `seed-1`, docs/rust-legacy.md); from 0.1.0 `fibc` is the compiler written in fibber, built by
itself, and stays 0.x until the owner says 1.0.0. The current release is
0.1.7 (the file `VERSION`). Download your platform's tarball and `SHA256SUMS`, then:

```
sha256sum -c --ignore-missing SHA256SUMS     # fibc-0.1.7-linux-x86_64.tar.gz: OK  (macOS: shasum -a 256 -c)
tar xzf fibc-0.1.7-linux-x86_64.tar.gz       # or fibc-0.1.7-darwin-arm64.tar.gz
echo '(defun main () -> i64 (do (println "hello from fibber") 0))' > hello.fib
fibc-0.1.7-linux-x86_64/bin/fibc --version   # fibc 0.1.7
fibc-0.1.7-linux-x86_64/bin/fibc run hello.fib
fibc-0.1.7-linux-x86_64/bin/fibc build hello.fib -o hello && ./hello
```

The darwin-arm64 binary is ad-hoc signed, not notarised: if macOS blocks a downloaded copy, remove the quarantine
attribute (`xattr -d com.apple.quarantine fibc-0.1.7-darwin-arm64/bin/fibc`). `fibc build` needs the system C compiler
(`cc`; Xcode's command line tools on a Mac) and a normal environment (it fails under a bare `env -i`: give it a `PATH`).
The tarball holds one directory, `fibc-VERSION-PLATFORM/`:

| Path | What |
|------|------|
| `bin/fibc` | the compiler |
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

**Which CPU it needs.** `fibc` needs a 2013+ x86-64 CPU with AVX2 and FMA (x86-64-v3), or Apple
Silicon/ARMv8, and on Linux glibc 2.33 or later. fibber supports only instruction sets with guaranteed tail calls and
fused multiply-add (docs/adr/0008). On an older x86-64 CPU `fibc`, and every x86-64 program it
builds, stops at start with `trap: this program needs x86-64-v3 (AVX2, FMA); this CPU lacks: ..`
instead of crashing later with an illegal instruction.

**Which CPU the code is for.** A release is built with `FIB_TARGET_CPU=x86-64-v3`
(`scripts/package.sh` sets it unless it is already set), so that the `fibc`
binary in the tarball runs on any x86-64 CPU with those instructions and not
only on the one that built it. `FIB_TARGET_CPU` is read by lair (`compiler/llvm/target.fib`)
whenever it generates code, so it also applies to the programs you compile: with the
variable unset, empty or `host`, `fibc run` and `fibc build` on your machine generate code
for **your host CPU** (its name and its features), and a program built that way may
not run on an older CPU. To build a program that runs elsewhere, set it:

```
FIB_TARGET_CPU=x86-64-v3 fibc build hello.fib -o hello
```

Any other value is passed to LLVM as a CPU name with no extra features. A CPU below
x86-64-v3 (`x86-64`, `x86-64-v2`) still builds, but it is outside the supported set: there
`simd/fma` is a compile-time warning and a run-time trap (use `simd/muladd` for portable code).
`fibc targets` lists the code generation targets and which are supported; riscv64 is parked
(LLVM has no `tailcc` for RISC-V) and `fibc build --target` refuses it without `--allow-unsupported`.

## Catching traps

A trap (an index out of range, an integer overflow, `(trap msg)`, `unwrap` of `nil`) aborts the program unless a
`try` of `fib.ex` is active on the thread; then it unwinds to that `try`, and every object the unwound frames owned
is freed (the audit stays clean):

```clojure
(ns main (:use fib.core fib.ex))

(defun main () -> i64
  (try (nth [1 2 3] 10)
       (catch e :when (= (ex-kind e) (some "index")) -1)
       (finally (println "done"))))
```

`throw` throws an `(ex-info ..)`-shaped exception; with no `try` active it is the trap of its message. Out of memory,
a stack overflow, a thread that cannot start and a trap inside a `finally` that runs while unwinding stay fatal. Only
a program that catches pays for it: the others compile exactly as before (docs/adr/0009, docs/design/exceptions.md).

## Projects and dependencies

A project is a directory with a `deps.fib`; libraries come from git commits (or local directories). `fibc new`
makes one:

```
fibc new hello                  # hello/deps.fib, src/hello/core.fib, src/main.fib, specs/hello-spec.fib
cd hello
fibc deps add acme/util --git https://github.com/acme/util.git --tag v1.2.0
fibc run src/main.fib           # resolves, fetches, writes deps.lock; no -I needed
fibc test                       # runs specs/*-spec.fib with the project's roots
fibc deps tree
```

`deps.fib` is fibber data in the shape of Clojure's `deps.edn`:

```clojure
{:name "hello" :version "0.1.0"
 :paths ["src"]
 :deps {acme/util {:git/url "https://github.com/acme/util.git" :git/tag "v1.2.0"}
        acme/json {:git/url "https://github.com/acme/json.git" :git/sha "<40-hex commit>"}
        mine/x    {:local/root "../x"}}
 :aliases {:test {:extra-paths ["specs"]}}}
```

Every command that compiles (`run`, `build`, `test`, `emit`, `explain`) finds `deps.fib` in the working directory or
above, resolves it against `deps.lock` (commit it: it pins every commit and its git tree id) and adds the project's
`:paths` and each library's to the module roots. The same library wanted at two commits is an error naming who
asked for what (choose with `:override`); a tag that moved since it was locked is refused until `fibc deps update
NAME`; two libraries defining one module are refused. `--locked` refuses a stale lock, `--offline` never fetches,
`--frozen` is both. Checkouts are cached read-only under `~/.cache/fibber/git` (`$FIBBER_HOME`) and verified on
every build. **A dependency's macros run at compile time** (like Rust's proc-macros): depend only on code you
trust. Resolution itself runs only `git`. Design and limits: docs/design/packages.md.

## Editor

`editors/vscode/` is a VS Code language pack for `.fib`: a TextMate grammar
(highlighting of comments, strings, numbers, keywords, special forms, the
library's macros, `x:` annotations, reader macros), language configuration
(brackets, comments) and snippets. It has no build step; copy or symlink the
directory to `~/.vscode/extensions/tailoredshapes.fibber-0.3.0` and reload the window.
Versions 0.2.x also started `fibref lsp` for completion, hover and diagnostics; the Rust
`fibref` is gone, so since 0.3.0 the pack is the grammar, configuration and snippets only
(a language server, `fibc lsp`, is planned: docs/design/dev-loop.md). The release workflow also
attaches the extension as `fibber-vscode-VERSION.vsix` to each release
(`code --install-extension FILE.vsix`). Details: `editors/vscode/README.md`.

## Status and development

The active compiler is written in fibber: `compiler/fibc.fib` contains the
front end and emitter, `compiler/lir/` checks lIR, and `compiler/native/`
lowers it through the LLVM-C bindings in `compiler/llvm/`. The runtime
remains lIR source (`rt/`). The Rust tools were retired on 2026-10-05: they are
in git (tag `seed-1`), and docs/rust-legacy.md says how to build them again,
what each gave and what was lost. New language work belongs in `compiler/`
and `lib/`. The passes are checked against golden outputs recorded from the
Rust oracles (`compiler/tests/golden/`).

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
- Exclusive views: `with-view` lends a writable window over an `Array` that the
  checker proves nothing else can reach, so a loop writes in place with no
  copy and no per-element uniqueness test; `with-tiles` hands disjoint windows
  to tasks. Windows run at about the speed of the array loop at `-O 2`.
  See [the design](docs/design/exclusive-views.md).
- aarch64: `--target TRIPLE` (or `FIB_TARGET_TRIPLE`) emits objects and
  assembly for Linux, macOS and iOS triples. On an Apple M1 Ultra the cross-built
  compiler runs, builds itself, and reaches the same fixed point as on x86;
  `scripts/package.sh` builds a macOS tarball. No darwin-arm64 release has been
  published, and iOS and aarch64 Linux have not been run. See
  [the aarch64 design](docs/design/aarch64.md) and [the first numbers](docs/shootout/aarch64.md).
- GraphQL lives in its own repository, lacewing: `ssh://git@localhost:2222/tailoredshapes/lacewing.git`, pulled in with `fibc deps add`
  (see Projects and dependencies).

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
Fused `t/dense` layers apply bias and an activation as each output tile is
stored, and `t/softmax` and `t/layernorm` work row by row. On the recorded
single-thread runs, matrix multiplication is within about 1.15x of NumPy with
OpenBLAS, a float32 MLP forward pass is at parity with it, and softmax and
layernorm are faster than NumPy; reductions along the last axis are still
slower ([the comparison](docs/shootout/tensor.md)). This is a dense numerical
foundation, not NumPy feature or performance parity.

The explicit [`fib.logic` relational library](lib/fib/logic/README.md) adds
finite typed terms, persistent unification with occurs checking, fair sequential
search, and `fresh`/`conde` syntax. The same relation can infer missing values or
enumerate answers; try `./F run examples/logic.fib`. Parallel search remains
future work, with this engine serving as its tested reference.

Its [`fib.logic.fd` extension](lib/fib/logic/README.md) adds compact and sparse
finite integer domains, watched propagation for all-different and arithmetic
constraints, batched model setup, and smallest-domain-first search. The port of [tsmarsh/sudoku](examples/sudoku/solver.fib)
uses those constraints; build it with `./F build examples/sudoku.fib -I examples -I lib`.
The [finite-domain design note](docs/design/finite-domains-and-sudoku.md) records
the API, provenance, validation cases, and a comparison with the original
Clojure/core.logic implementation.

The [`fib.os` library](lib/fib/os/README.md) provides typed file and descriptor
operations, directories, TCP sockets, polling, clocks, environment variables,
process identity, executable discovery, system information, secure entropy, and
bounded native memory streams. Shared errors and selected platform backends
keep libc flags, layouts, and symbols out of application code. Linux is tested
natively; the Darwin backend ran on Apple Silicon during the aarch64 work (see
[its design](docs/design/aarch64.md)), but is not yet part of any automated
gate. See the [OS design record](docs/design/os.md).

The explicit [`fib.http` library](lib/fib/http/README.md) provides shared HTTP
messages, a Ring-style HTTP/1.1 server with a fixed native worker pool, and a
Hato-style client backed by libcurl. It includes binary bodies, repeated headers,
query and form encoding, keep-alive, chunked requests, verified HTTPS on the
client, timeouts, and asynchronous requests. Build the
[server example](examples/http-server.fib) with `./F build examples/http-server.fib -I lib`;
the [client example](examples/http-client.fib) also needs `-l curl`.
See the library README for API contracts and the current scope.

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
and editor tool. The Rust interpreter is retired, and a port of its interpreter and
memory audit to fibber is scheduled. The earlier interpreter/compiler
comparison rules in [spec/method.md](spec/method.md) remain historical text
pending consolidation; use the stage-2 gate for current compiler validation.
There is no cargo anywhere in the build or the gate.

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
