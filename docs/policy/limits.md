# Known limits and security boundaries

This page describes current limitations; it does not certify security.

- **TLS is UNAUDITED, client-only TLS 1.3.** It needs an explicit crypto provider,
  certificate roots and an HTTPS transport factory. No factory means a typed
  unsupported error, never a plaintext downgrade. Server TLS is a design;
  TLS sessions are not Send ([TLS design](../design/tls.md),
  [Send proposal](../design/tls-send.md)).
- Crypto is supplied by provider protocols and external audited-library drivers;
  the core avoids handwritten cryptographic primitives (ADR 0012). Provider
  contract tests are not a third-party security audit.
- Dependency macros run at compile time. A lock pins source but does not sandbox
  it. Trust dependencies, native drivers and their toolchains.
- `unsafe`/FFI can bypass the language's safety checks. Application code should
  use typed driver APIs. Strong-reference cycles can leak.
- Out of memory, stack overflow, thread-start failure and a trap during an
  unwinding `finally` remain fatal. Structured cancellation is not complete.
- The expected case failure is `cases/stdlib/1707-a-def-atom-holds-what-the-program-puts-in-it.fib`:
  consult [the authoritative list](../../scripts/ci-stage2.expected) for its
  current name/verdict. Open cases are not passes.
- The library specification includes unbound planning rows, listed in
  [ADR 0023's exceptions](../adr/0023-unbound-rows.tsv). It is not an export reference.
- Windows is not supported. macOS has a separate native gate; it is not exercised
  by the default Linux gate. iOS is cross-emission only; RISC-V is parked.
- Scratch images lack certificates, NSS and timezone data. Provision those
  explicitly when required. Static and WASI checks may skip without toolchains.
- External database, GraphQL, HOCON and GPU drivers are separate repositories;
  their installation, contract checks and credentials are not bundled in core.

See [platform support](platform-support.md), [stability](stability.md) and
[security reporting](../../SECURITY.md).
