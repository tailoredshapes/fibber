# Contributing

Read [the method](spec/method.md) and [repository guidance](CLAUDE.md). Claims need
executable evidence; open/pending and skipped checks are not passes. Report a
spec/code disagreement before changing either rule. Keep build output out of git.

Install GNU Make 4+, the release seed named in SEED and LLVM 21 development
libraries. macOS uses `gmake` and Homebrew `llvm@21`. The seed fetcher verifies
checksums. `make help` lists targets; `BUILDER=/path/to/fibc` overrides the seed.

```sh
make -j8 quick
make doc-examples
make adr
make -j8 gate
```

Use focused cases/specs for the affected behaviour. The full gate also checks
self-compilation to a fixed point, pass goldens and the audited case suite.
`gmake mac-check` is the separate native Mac gate. Static/WASI stages report a
skip when their optional toolchains are missing. Release checks are `make release`.

Documentation examples use `fib run`, `fib check`, `fib frag`,
`fib reject "diagnostic"`, or `fib case` fences. A following `text out` fence
pins exact stdout. `fib case` delegates verdict/result/audit headers to the
existing case harness. Unmarked fences in dated design records are sketches.
Tutorial/guide pages declare `examples: required`. Regenerate source references
with `make doc-reference`; commit the generated diff. `make doc-examples` checks
examples, local links and reference equality. Python 3 is used for these checks,
as for the existing WASI test suite.

The [public documentation site](https://tailoredshapes.github.io/fibber/) is built
from these same Markdown files. See [website setup](website/README.md) for
`make docs-site`, local previews and automatic publishing from `main`.

Never rewrite pushed history. A behavioural change needs cases with expected
verdicts set before implementation; update goldens only for an intended change
and inspect their diff. See [the docs map](docs/README.md) for current guides and
historical design records.

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
known failure: the `def`-initialized atom case 1707
([known limits](docs/policy/limits.md)). Cases marked `open`
are counted separately; they are not passes. A changed non-passing set
fails the gate, including a known failure that starts passing.

The [October 4 decisions](docs/design/decisions-2026-10-04.md) supersede the
interpreter-oracle requirement: the interpreter is now a development
and editor tool. The Rust interpreter is retired, and a port of its interpreter and
memory audit to fibber is scheduled. The current validation rules are consolidated in
[spec/method.md](spec/method.md); use the stage-2 gate for compiler validation.
There is no cargo anywhere in the build or the gate.
