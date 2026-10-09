---
examples: required
---

# Targets and deployment

Use `fibc targets` for the compiler table and the
[platform matrix](../policy/platform-support.md) for the evidence behind it.
Cross-emission does not prove execution on the destination. `--target TRIPLE`
selects a target; cross-linking needs its toolchain/sysroot. C builds use
`--via c --cc CC`; `--emit c` emits only source and retains the C backend's
tail-call limitations. Static Linux builds use `--static` and musl.
A scratch image lacks CA certificates, NSS and timezone files. WASI needs a
sysroot and runtime; bare wasm needs host imports. PTX/WGSL are kernel targets,
not general application runtimes. See [WASM](../design/wasm.md),
[C backend](../design/lir2c.md) and [static linking](../design/static-linking.md).

```fib run
(defun main () -> i64 (do (println "hello from fibber") 0))
```

```text out
hello from fibber
0
```

## Deploying: static binaries, scratch images, Lambda

`fibc build --static program.fib -o program` makes an executable with no dynamic loader and no shared library (musl, linked statically): it needs only the Linux kernel, so it runs in a `FROM scratch` Docker
image and as an AWS Lambda custom-runtime `bootstrap`, whatever the host's libc. `--target aarch64-unknown-linux-gnu` (or `x86_64-...`) builds the other architecture from the same machine. A hello world is 77 KB; a program with JSON,
tasks and an HTTP server, 469 KB, and its scratch image 469 kB.

```
fibc build --static examples/static-demo/demo.fib -o demo
printf 'FROM scratch\nCOPY demo /demo\nENTRYPOINT ["/demo"]\n' > Dockerfile && docker build -t demo . && docker run demo batch /data.json
```

`--static` needs the musl pieces for the architecture (`libc.a`, `crt1.o`, ...): `scripts/build-musl.sh x86_64 DIR` builds them from a pinned musl source (needs gcc and make) and `FIB_MUSL_DIR=DIR` finds them; a release built with
`WITH_MUSL=1 scripts/package.sh` ships them in `share/fibber/musl/`. An ordinary `fibc build` needs none of this. For aarch64 from an x86-64 machine you also need an aarch64 `ld` (`binutils-aarch64-linux-gnu`) or `ld.lld`.
A module that needs a native library declares it (`(extern f ... :lib "NAME")`); `fibc build` links it only when the program reaches it, dynamically by default and bundled with `--static`, `--link curl=static` or `--link-mode static`,
and a missing library is an error that names the module and the library. A scratch image has no CA certificates, no NSS and no time-zone data: see docs/design/static-linking.md (the measurements, what musl needed, the notes for TLS and DNS)
and docs/design/platform-boundary.md (what the runtime assumes of the platform, and the tiers beyond: no libc, no operating system). `scripts/static-demo.sh` builds the demo into scratch images for both architectures and runs a Lambda
`bootstrap` against a local fake of the Runtime API.
