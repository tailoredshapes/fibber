# fibber

A Lisp with static types, memory safety and no garbage collector. Fibber combines
macros, ownership checking and native code for applications and numerical programs.
Its compiler is written in Fibber. Successor to [liar](https://github.com/tsmarsh/liar).

**Borrow first, count second.** Values inside their creating scope live and die
with it. Escaping values are reference counted. The compiler chooses; programmers
write neither lifetimes nor retain/release.

```text
Fibber source → Fibber compiler → lIR → LLVM IR → native
```

Visit the [Fibber website](https://tailoredshapes.github.io/fibber/) for searchable
documentation and a quick start. Start with the [tutorial](docs/tutorial/01-install-and-hello.md),
[documentation map](docs/README.md) or [examples](examples/README.md).
For a practice-first course, [Learn Fibber the Hard Way](docs/hardway/README.md)
has 16 exercises, deliberate failures and a native-command capstone.
Collections, JSON/JSON Schema, LZ4, HTTP/DNS, OS services and database/crypto
contracts are in the library. SIMD, tensors, autodiff, GPU protocols and relational
search are explicit modules. See the [library index](docs/reference/library/INDEX.md).
TLS is client-only and **UNAUDITED**; read [security and limits](docs/policy/limits.md).

## Install

The current release and bootstrap seed are **0.1.13**, recorded in VERSION and SEED.
Download your platform's archive and SHA256SUMS from
[GitHub releases](https://github.com/tailoredshapes/fibber/releases):

- `fibc-0.1.13-linux-x86_64.tar.gz`: glibc 2.33+, AVX2 and FMA (x86-64-v3).
- `fibc-0.1.13-darwin-arm64.tar.gz`: Apple Silicon; published since 0.1.7.

```sh
sha256sum -c --ignore-missing SHA256SUMS  # macOS: shasum -a 256 -c SHA256SUMS
tar xzf fibc-0.1.13-linux-x86_64.tar.gz   # substitute darwin-arm64 on a Mac
export PATH="$PWD/fibc-0.1.13-linux-x86_64/bin:$PATH"
fibc --version
```

Keep `bin/` and `share/` together when relocating. Building programs needs `cc`
(Xcode command-line tools on Mac). LLVM is bundled in release binaries.
See [platform setup](docs/tutorial/01-install-and-hello.md#platform-setup-and-release-layout)
for system dependencies, Mac quarantine handling and source builds.

Save this as `hello.fib`:

```fib run
(defun main () -> i64 (do (println "hello from fibber") 0))
```

```text out
hello from fibber
0
```

```sh
fibc run hello.fib
fibc build hello.fib -o hello
./hello
```

`run` prints main's result; an executable uses it as its exit status.

## Working with Fibber

- [Projects and dependencies](docs/guide/packages-and-dependencies.md),
  [modules](docs/guide/modules-and-namespaces.md), [ownership](docs/guide/ownership-and-borrowing.md).
- [Errors and traps](docs/guide/errors-traps-and-try.md),
  [concurrency](docs/guide/concurrency-and-parallelism.md), [testing](docs/guide/testing.md).
- [Tooling](docs/guide/tooling.md): `check`, `explain`, `serve`, `--server`, `repl`, `lsp`;
  [VS Code setup](editors/vscode/README.md), [CLI reference](docs/reference/cli.md).
- [Deployment](docs/guide/targets-and-deployment.md): static musl, C, wasm and cross targets;
  [platform evidence](docs/policy/platform-support.md).
- [Contributing](CONTRIBUTING.md): `make help`, `make quick`, `make gate`, `make doc-examples`.

The self-hosting check is a fixed point: stage 2 builds stage 3 and both emit
identical compiler lIR. Maintained stage-2 goldens, audited cases, behaviour specs
and executable ADRs provide additional evidence ([method](spec/method.md)). Rust
tools are retired at tag `seed-1` ([history](docs/rust-legacy.md)).

Fibber remains pre-1.0. The owner approves release promises alongside executable
acceptance checks; [stability](docs/policy/stability.md) records the proposal.
The [roadmap](ROADMAP.md), [changelog](CHANGELOG.md) and dated
[performance records](docs/shootout/baseline-2026-10-04.md) retain development history.
