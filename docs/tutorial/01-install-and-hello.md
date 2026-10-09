---
examples: required
---

# Install and hello

Use the platform instructions below for Linux
x86-64 or macOS arm64. Linux requires glibc 2.33 and an AVX2/FMA CPU; Apple
Silicon is the Mac target. Keep `bin` beside `share` when moving the installation.
Put its `bin` directory on PATH. Save the following as `hello.fib`.

```sh
fibc run hello.fib
fibc build hello.fib -o hello
./hello
```

`run` uses a JIT and prints main's result after the program's own output. A built
executable uses main's integer result as its exit status. Building needs `cc`.
[Next: values](02-values-and-collections.md).

```fib run
(defun main () -> i64 (do (println "hello from fibber") 0))
```

```text out
hello from fibber
0
```

## Platform setup and release layout

Releases are on the [GitHub releases page](https://github.com/tailoredshapes/fibber/releases):
`fibc-VERSION-linux-x86_64.tar.gz` and, from 0.1.7, `fibc-VERSION-darwin-arm64.tar.gz`
(Apple Silicon), with `SHA256SUMS`. The 0.0.x releases are
the Rust tools (tag `seed-1`, docs/rust-legacy.md); from 0.1.0 `fibc` is the compiler written in fibber, built by
itself. The owner decides when to publish 1.0 after executable acceptance checks;
see the [stability proposal](../../docs/policy/stability.md). The current release is
0.1.13 (the file `VERSION`). Download your platform's tarball and `SHA256SUMS`, then:

```
sha256sum -c --ignore-missing SHA256SUMS     # fibc-0.1.13-linux-x86_64.tar.gz: OK  (macOS: shasum -a 256 -c)
tar xzf fibc-0.1.13-linux-x86_64.tar.gz       # or fibc-0.1.13-darwin-arm64.tar.gz
echo '(defun main () -> i64 (do (println "hello from fibber") 0))' > hello.fib
fibc-0.1.13-linux-x86_64/bin/fibc --version   # fibc 0.1.13
fibc-0.1.13-linux-x86_64/bin/fibc run hello.fib
fibc-0.1.13-linux-x86_64/bin/fibc build hello.fib -o hello && ./hello
```

The darwin-arm64 binary is ad-hoc signed, not notarised: if macOS blocks a downloaded copy, remove the quarantine
attribute (`xattr -d com.apple.quarantine fibc-0.1.13-darwin-arm64/bin/fibc`). `fibc build` needs the system C compiler
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

**Kubernetes jobs.** `make k8s-apply`, then `make k8s-gate` (or `k8s-quick`, `k8s-mutants NAME=..`, `k8s-job TARGET=..`) runs the Make gate as a Job in the
`fibber-ci` namespace of the local k3s; `K8S_ARCH=arm64` runs it on a native aarch64 Linux node. Topology, quotas, how to add a node: docs/design/build.md, section 7.

**Building and testing on macOS (Apple Silicon).** The build is GNU Make 4 or later: macOS ships GNU Make 3.81 as `/usr/bin/make`, which the
Makefile refuses with a message, so `brew install make` and run `gmake`. Also `brew install llvm@21` (keg-only; `llvm` alone is 22) and the
Xcode command line tools. `gmake -j8 mac-check` is the gate for the Mac (mk/mac.mk): the full gate less the tools and stages that need Linux, each
named with its reason in `compiler/tests/expected-macos.txt`, plus the machine checks of `scripts/mac-check.sh`; `MAC_QUICK=1` is the short set.
The scripts assume GNU coreutils and bash 4; on a Mac `scripts/portable/` stands in (a `timeout`, `flock`, `nproc`, `sha256sum`, `sed -i` and `date +%N`
that work there, and a `ulimit -v` that succeeds), which the Makefile puts on `PATH` itself and `. scripts/portable/env.sh` does for a script run by hand.
`scripts/lint-portable.sh` fails the gate on any other Linux-only tool or bash 4 feature that is not marked `# linux-only: reason`.

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

**Vector width (AVX-512).** `f64xn`, `:native` and `(native-lanes T)` stand for the width the CPU table prefers: 256 bits for every AVX2 CPU, 512 for
`x86-64-v4`, Sapphire Rapids, Granite Rapids and Zen 4/5 (`compiler/types/targets.fib`; Skylake-X and the client AVX-512 cores stay at 256). The numeric library
(`fib.tensor`) is written over that width: on a host with AVX-512, a program built for the host gets 512-bit kernels (1.6 to 1.7 times the matmul and dense-layer speed on
Sapphire Rapids, 10 percent on Zen 4: docs/shootout/avx512-2026-10-06.md); a release binary built for `x86-64-v3` keeps the 256-bit ones. `FIB_VECTOR_BITS=128|256|512`
overrides the width for one run (a width the CPU lacks is split into the registers it has: correct, slower). Design: docs/design/avx512.md.
