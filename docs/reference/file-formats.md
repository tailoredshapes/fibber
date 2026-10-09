# File formats

`.fib` holds source. `deps.fib` is project data and `deps.lock` pins dependencies
([package format](../design/packages.md)). `*-spec.fib` files contain behaviour
specs; case headers record verdict/result/audit expectations.

lIR text is compiler output from `fibc emit`, documented in [the lIR spec](../../spec/lir.md).
It is not a frozen 1.x interchange format. C/PTX/WGSL output and the compile
server JSON-line protocol are similarly pre-1.0 surfaces. ADR Markdown fences
are interpreted by the standalone `make adr` runner.

Releases contain `fibc-VERSION-PLATFORM/{bin,share/fibber/lib,LICENSE,README.txt}`.
Keep bin/share together when relocating. VERSION/version.fib and SEED checksums
are release-engineering inputs checked by ADR 0017 and the version check.
