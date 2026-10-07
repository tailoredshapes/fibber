# 0011. Native access is confined to drivers and the platform layer

Status: accepted
Date: 2026-10-06
Source: the owner, 2026-10-06 (STATIC-1: static executables, scratch images, Lambda; "what the language bundles versus assumes from the platform is an architectural decision");
docs/design/static-linking.md; docs/design/platform-boundary.md; spec/syntax.md 3.15 (`extern` and its `:lib` option).

## Context

A fibber program should run on a Linux with a libc, in a `FROM scratch` image with none, and later on a raw VM or no operating system at all. That only works if
the places that touch the platform are few and named. Today `extern` (a C function by name) appears in modules that have nothing to do with the platform: the
number formatter calls `snprintf`, the JSON module calls `strtod` and `memcpy`. Each is one more place
a port to musl, to direct system calls or to a freestanding target has to find and change, and each ties a library to libc by the back door. A library that needs a
native library (libcurl is the first) used to need the user to know it and write `-l curl`; a program that merely required the module and used none of it did not need it
and, once the emitter pruned unused code, did not reach it either.

## Decision

1. Every module has a scope and a function. What it needs beyond that goes through a language abstraction (a protocol, the platform interface of
   docs/design/platform-boundary.md), never by reaching into the operating system or declaring `extern` itself.
2. Only a module that **is** the wrapper of the native thing may contain `extern` or a native-library declaration: the **platform layer** (`fib.os` and what it
   wraps, `lib/platform/`, the runtime `lib/prelude.fib` and `rt/`) and a **driver** (a module whose whole job is to wrap one native library: today none; the libcurl driver `fib.http.client.ffi` was the first and was removed by HTTP-1 when the HTTP client became native over `fib.os.net`). The compiler (`compiler/`) is a host tool, not a library; it is outside this rule for now.
3. A driver's native library is declared with the extern option `:lib "NAME"` and linked by the **mode of the build**, not the module: dynamic (`-lNAME`, resolved
   when the program starts) or bundled into the executable (the static archive `libNAME.a`: the "uberjar" choice). `fibc build --link NAME=static|dynamic` sets one library,
   `--link-mode static|dynamic` all of them, and `--static` implies static for all (an executable with no loader has nothing to resolve a dynamic library with).
   A library is linked only when an extern that names it is reached from the program; a missing library is an error that names the module and the library.

## Consequences

- Porting the platform (musl, direct system calls, freestanding) means reading `fib.os`, `lib/platform/`, the runtime and the drivers, a list that is checked, not
  remembered.
- The violators below are the known set to migrate: each moves to a platform-layer function (`fmt/libc` and `json/libc`: number formatting and parsing and `memcpy` belong
  in `fib.os.memory` or a primitive; `math/libm`: `sqrt` is an LLVM intrinsic; `log/file` migrated by MERKLE-1: `rename` is `fib.os.files`' `rename-file`; `http/server/date` migrated by HTTP-1: the date is computed in fibber; `test/arch/rules`: `system` belongs in `fib.os.process`). The allow-list shrinks as they move and can only shrink: an entry that no
  longer matches is itself a finding.
- A new module cannot add an `extern` without either being a platform module, being named a driver here (a change to this ADR) or failing the gate.

## Governance

```fibber fitness
(rule "no module outside the platform layer and the drivers contains an extern (the known violators are allowed, and only those)"
  (allowing (grep-live repo ["lib/**.fib" "!lib/fib/os/**" "!lib/platform/**" "!lib/prelude.fib"] "(extern ")
            ["lib/fib/fmt/libc.fib:(extern" "lib/fib/math/libm.fib:(extern"
             "lib/fib/json/libc.fib:(extern" "lib/fib/test/arch/rules.fib:(extern"])
  (plant "lib/fib/zz-plant.fib" "\n(extern puts (ptr) -> i32)\n"))

(rule "no module outside the platform layer and the drivers declares a native library"
  (grep-live repo ["lib/**.fib" "!lib/fib/os/**" "!lib/platform/**" "!lib/prelude.fib"] ":lib \"")
  (plant "lib/fib/zz-plant.fib" "\n(extern sqrt (f64) -> f64 :lib \"m\")\n"))
```

### What this does not check

`compiler/` (a host tool; its externs are the compiler's own libc and LLVM bindings); the programs under `cases/` and `examples/`, which are the tests and the users;
`lib/fib/os/` and `lib/platform/` themselves (the platform layer is where `extern` is meant to be: whether it is *minimal* is docs/design/platform-boundary.md's question, not
a rule yet); a native call made through a macro expansion or through `dlopen` rather than a top-level `extern`; the `rt/` runtime's own `declare`s (lIR, not fibber source).
