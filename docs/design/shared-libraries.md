# Shared libraries: `fibc build --shared`, the C ABI of exports, the generated header

Status: stage N1 of the native-embedding plan (`docs/design/fibber-native-embedding.md`, section 7). Implemented and tested on Linux x86-64
(`compiler/tests/shared/shared.sh`, in the gate); tried by hand on an Apple Silicon Mac (`compiler/tests/shared/shared-mac.sh`, which the gate does NOT run).

## 0. Read this first: a `--shared` library is not safe for a host yet

N1 makes the library and its C interface first class. It does not make a call into the library safe. That is stage N2, and nothing here
changes the runtime's trap or threading behaviour. Until N2 lands:

- **(Changed 2026-10-22: docs/design/stack-check.md 6. A library catches in each export and reports `fib_status` / `fib_error`; a stack overflow is one of those failures. The three points below are what they were before.)**
- **A trap in fibber code aborts the host.** A trap (the study's case is `nth` out of range) calls `abort()`: the host
  process dies (exit 134 under the study's `traps-host.c`). There is no error code to return and nothing for the host to catch.
- **Host threads (fixed after this document was first written).** The study (`fibber-native-embedding.md` 1.2) called one export from two threads at once
  and the process crashed with SIGSEGV in 20 of 20 runs. The cause was the runtime's unlocked small-object fast path, which a thread the runtime did not start
  never switched off; a library is now multi-threaded from its first export call (docs/design/host-threads.md; `compiler/tests/driver/host-threads.sh`: 4 and 16
  host threads, cold and warm, 12 runs each). What is still unsafe is listed there: sharing fibber values between host threads, `dlclose` while a thread is inside
  the library, `fork` in a host, and a callback that re-enters an export during the start.
- **Deep recursion overflows the host's stack.** The executable's `main` installs a stack guard; the library path does not, so a recursion that is fine
  in `fibc run` can segfault the host.
- The runtime calls `mallopt(M_MMAP_THRESHOLD, 131072)` on the first call, which changes the host's malloc policy for the whole process.

Therefore `--shared` prints no promise of safety: the usage text, the generated header and this document all say so. A host that embeds a
library today accepts these four risks (the native app of the plan calls the renderer from one thread, with inputs it has checked).

## 1. Use

```
fibc build X.fib -I DIR.. --shared -o libX.so [--header X.h] --export NAME[=SYMBOL] ..  [-L dir -l lib]
fibc build X.fib --emit obj --header X.h --export NAME[=SYMBOL] .. -o X.o                    # the object only, a host that links it itself
```

- `--shared` links the object of the program with the system C compiler as a shared library: `cc -shared` on Linux (`libX.so`), `cc -dynamiclib` on macOS
  (`libX.dylib`). The name is the one of `-o`; the build does not guess the extension.
- `--export NAME[=SYMBOL]` is as before: the function `NAME` of the program becomes the C symbol `SYMBOL` (default `NAME`). A name with `-` is a legal
  symbol for `dlsym` but not for C or Swift: write `--export sum-squares=sum_squares`. `--header` refuses a symbol that is not a C identifier and says what to write.
- `--header FILE.h` writes the C header of the exports, and `module.modulemap` in the same directory (for Swift: section 4). It goes with `--shared` or
  `--emit obj` (a header on an executable is refused).
- `-L dir` and `-l lib` are passed through, as for an executable (each `-L` is also an absolute rpath).
- Refused, with the reason on standard error and exit status 2: `--shared` with no `--export` ("a shared library is the functions a host can call, and this
  build names none"), `--shared` with `--emit`, `--header` without an export. Exit status 4 and a message naming the parameter: an export whose type does
  not cross (section 3.1), a name that is no function, a generic function, a function with an `&` parameter.
- There is no `main` in a library: the program needs none (`fibc build` already dropped the requirement for `--export`). The runtime starts on the first call of any export.
- `--shared` builds for the host. With `--target` it is refused (use `--emit obj` and link where the target runs), as `build` of an executable is.

## 2. Decisions

1. **Link with the system `cc`, in the existing link code.** The executable link already needs `cc`; `native.link` gets `link-shared` beside `link`. A
   linker bundled in the compiler would be a second dependency to keep in step with the platforms. The object is written to `OUT.lair.o` beside the output and removed.
2. **Only the exports are visible, once** (since 2026-10-25; spec/lir.md 4.3, decision 20). The object marks everything but the export wrappers `internal`
   (the study's `nm` showed it), and that is the whole guard: the lIR's linkage and visibility are the contract, so the link adds no version script and no
   `-exported_symbols_list` any more (it did until 2026-10-25, from a symbol list handed to lair beside the module, "the second guard"; a runtime symbol
   that forgets `internal` now shows in `nm -D`, which the gate checks). The gate checks `nm -D --defined-only` against the declared names plus `fib_status` and `fib_error`, the two symbols every library has (docs/design/stack-check.md 6; the names are provisional until the owner confirms them).
3. **Position independent.** The host machine is created with the PIC relocation model (`compiler/llvm/target.fib`, `reloc-of`); the gate checks the library has no text relocation (`readelf -d`: no `TEXTREL`).
4. **A wrapper per export, made at export time, in lIR (`compiler/emit/export.fib`).** The exported symbol is a C-convention function with the name the
   user gave; it first makes sure the runtime is up (`fib.export-ready`), converts, calls the fibber function, converts back. The wrapper is the only
   place that knows the ABI; the header is generated from the same signatures (`emit/exporthdr.fib`), so a prototype cannot differ from the code. The
   name of the symbol is the one given with `--export`, whatever the lowering does (stable across versions).
5. **A string never lends or hands over memory.** A `str` parameter is copied into a fresh `str` for the duration of the call; a `str` result is copied into the host's buffer.
   No allocator, ownership or lifetime rule crosses the boundary (the study's recommendation for N1; handles come later).
6. **The length of a string result is returned, not its negation.** The study proposed "the byte count, or the negated size needed". A single non-negative
   value is simpler to use (`if (n > cap) retry`) and cannot be mistaken for an error code; truncation is explicit (section 3.3).
7. **A `bool` is a byte.** `bool` is `i1` in lIR, which has no C ABI of its own; the C type is `uint8_t` holding 0 or 1, converted in the wrapper.
8. **`--header` also works with `--emit obj`**, for a host that links the object itself and wants the same prototypes.
9. **`wasm32` keeps what it had.** Int, float, ptr and unit exports produce the same object as before this change (checked: identical `.o` for a four-export program).
   `str` exports are refused for wasm32 (the length is an `i64`, a BigInt to JavaScript; the wasm host passes bytes and a length itself, `docs/design/wasm.md` section 5).

## 3. The ABI

### 3.1 Types

| fibber | C (header) | notes |
|---|---|---|
| `i8` `i16` `i32` `i64` | `int8_t` `int16_t` `int32_t` `int64_t` | as they are |
| `f32` `f64` | `float` `double` | as they are |
| `bool` | `uint8_t` | 0 or 1; any non-zero byte in a parameter is true |
| `ptr` | `void*` | a raw pointer; the host owns what it points to |
| `unit` (result only) | `void` | |
| `str` parameter | `const uint8_t* NAME, int64_t NAME_len` | two C parameters |
| `str` result | trailing `uint8_t* out, int64_t out_cap`, and an `int64_t` return | the length needed |

Everything else is refused when the library is built (`char`, `keyword`, `Vec`, records, closures, `Result`, a `unit` parameter): the message names the
parameter and lists what crosses. Parameter names in the header are the fibber names with every character that C does not allow turned into `_` (a leading
digit or a C or C++ keyword gets a `_`); a clash with another parameter, with the `_len` of one, or with `out`/`out_cap` adds a `_`.

### 3.2 A `str` parameter

- The bytes are UTF-8 (the library does not check), **not NUL terminated**; `NAME_len` counts them. An embedded NUL is a byte.
- `NAME_len` below zero is treated as 0. With length 0 the pointer is not read: `NULL` is fine.
- The library copies the bytes during the call, calls the function, and releases its copy (unless the function took it, an owned parameter: then the function does).
  The host may reuse or free its buffer when the call returns.

### 3.3 A `str` result: the caller's buffer, the length, truncation and retry

The function receives `(uint8_t *out, int64_t out_cap)` after its other parameters and **returns the length in bytes of the string** (never negative).

- At most `min(length, max(out_cap, 0))` bytes are written to `out`: the first bytes of the string. Nothing is written past `out_cap`. **No NUL is written.**
  When `out` is `NULL`, or `out_cap` is zero or negative, nothing is written (and the length is still returned: that is the way to ask the size first).
- So the string was returned whole if and only if the result is `<= out_cap`. Otherwise it was **truncated**: the host has the first `out_cap` bytes,
  and a cut can fall inside a multi-byte UTF-8 sequence, so a truncated result is not a valid string.
- **The retry rule.** The host calls with the buffer it has; if the result is above `out_cap` it allocates a buffer of that length and calls again. The function is called
  again from the start (the library keeps nothing between the calls), so a function that is not deterministic may return a different length the second time: the host loops until the
  result fits (the tests' hosts allow two calls for a deterministic function). `compiler/tests/shared/host.c`, `solid-host.c` and `main.swift` are three
  implementations.
- A function with a `str` result and no parameters is `int64_t banner(uint8_t* out, int64_t out_cap)`.

Why not return a pointer the host frees: it would put an allocator and an ownership rule on the boundary (whose `free`? the host's libc may not be the library's), which is what N1 avoids.
The cost is a second call and a second run of the function when the buffer was too small; a host that knows a bound passes it and calls once.

### 3.4 What is not done

- **Vectors, arrays, records, enums, `Option`/`Result`**: refused. A host that needs an array passes a `ptr` and a count (as `dot` and `iota` do in the tests) and the function reads it with `unsafe`.
- **Handles** (a library-side table of objects named by an `int64_t`, with release): not done; stage N3.
- **Callbacks** (the host gives a function pointer, with `user` data, and a cancel flag): not done; stage N3.
- **Errors**: there is no status code and no last-error message. A function that can fail returns a value that says so (the renderer returns an
  `{"ok":false,..}` JSON), and a trap aborts (section 0). Stage N2 (traps become a status) and N3 (`fib_last_error`).
- **Call safety**: section 0. Stage N2.
- **An ABI version symbol** (`fib_abi_version`) so a host can refuse a mismatched library: not done. It would be an export the program did not name; the gate's "only the declared symbols" check would learn it.
- **Shutdown** (`dlclose` of a library that started threads): not exercised.
- **The object cache**: `--shared` goes through the whole-program `--emit obj` path (the same as the manual route of the study), not the partitioned, cached
  one `build` uses for executables; a large library takes the seconds the study measured (the renderer: 12 s here, 3 s for the kernels' size).
- **Windows**, and a `.a` (static library): not done.
- **Swift wrappers** (a class per handle, `throws` from status codes): the module map only; wrappers are N3.

## 4. The header and the module map

For the gate's kernels library (`compiler/tests/shared/kernels.fib`), `--header kernels.h` writes (abridged):

```c
/* kernels.h: generated by `fibc build --shared --header` from .../kernels.fib. Do not edit. */
/* The ABI (docs/design/shared-libraries.md): ... */
#ifndef FIB_KERNELS_H
#define FIB_KERNELS_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

int64_t add(int64_t a, int64_t b);
double dot(void* xs, void* ys, int64_t n);
/* fibber: sum-squares */
int64_t sum_squares(int64_t n);
int64_t greet(const uint8_t* name, int64_t name_len, uint8_t* out, int64_t out_cap);
int64_t banner(uint8_t* out, int64_t out_cap);
...
#ifdef __cplusplus
}
#endif
#endif
```

and, beside it, `module.modulemap`:

```
module kernels {
  header "kernels.h"
  export *
}
```

The module is named after the header file (`kernels.h` is module `kernels`). Swift: `swiftc -I DIR -L DIR -lkernels -Xlinker -rpath -Xlinker DIR main.swift` with `import kernels`
(`compiler/tests/shared/main.swift`). The header is valid C11 with `-Wall -Wextra -Werror -pedantic` and valid C++17 (the gate compiles it both ways); it needs only `<stdint.h>`.

## 5. Platform notes

- **Linux x86-64.** `cc -shared OBJ -o libX.so -Wl,-soname,libX.so -Wl,--version-script=LIST -lm -lpthread`. `readelf -d`: SONAME `libX.so`, NEEDED only `libc.so.6` on the test machine
  (the gate accepts `libc`, `libm` and `libpthread`; an older glibc may list the last two). The runtime needs `pthread_*`, `madvise`, `mallopt`,
  `malloc_usable_size`, `snprintf`, `strtod` and libm. The host links with `-L DIR -lX -Wl,-rpath,DIR` or `dlopen`s the file.
- **macOS aarch64.** `cc -dynamiclib OBJ -o libX.dylib -install_name @rpath/libX.dylib -Wl,-exported_symbols_list,LIST` (symbols carry the leading underscore in the list and in `nm`, not in C). A host links with `-rpath`
  to the directory or to `@loader_path`. The deployment target is not set: see 6.2. `libLLVM` is a dependency of `fibc`, not of the library.
- **Both.** The library is compiled for the machine that builds it (`FIB_TARGET_CPU` picks another CPU, as for executables; a release sets x86-64-v3 for the AVX2 floor). A host on an older CPU than the build machine's would fault.
- **Calls.** The study measured an empty call at about 2 ns and `dlopen` at 0.05-0.3 ms; the wrapper adds a once-only flag check (`fib.export-ready`) and, for strings, one allocation and copy per `str` parameter and one copy for a result.

## 6. Tests

### 6.1 The gate (Linux)

`compiler/tests/shared/shared.sh STAGE2 [refusals|kernels|renderer]` is registered twice, as the repo registers its tool checks (`mk/tools.mk`, `scripts/gate-budget.tsv`):
the tool `sh-shared` (`refusals kernels`, about 4 s) and the stamp `shared-renderer` (`mk/checks.mk`, about 20 s; it says `SKIPPED: fib-solid is not at ...` in its stamp, and in the report, when the
sibling repository `../fib-solid` (or `SOLID=DIR`) is missing, as the wasm and static stages say it for a missing toolchain). Run by hand, a missing fib-solid makes the script print `SKIPPED` loudly and exit 77.

- **refusals**: no export, `--shared` with `--emit`, `--header` on an executable, a symbol that is not a C identifier, a `Vec` parameter, a `Vec` result, a name that is no function.
- **kernels** (`kernels.fib`: integers of each width, `f32`, `f64`, `bool`, `ptr` with a length, `unit`, `str` in, `str` out, two `str` parameters, a `str` result with no parameter, an owned `str` parameter, the empty string): the library and header built with
  `--shared --header`; the header's every prototype byte for byte, valid C11 and C++17; `module.modulemap` byte for byte; `nm -D --defined-only` equal to the declared names and `fib_status`, `fib_error`; no `TEXTREL`; SONAME; no object or list left beside the output;
  `host.c`, compiled against the **generated** header, prints what `oracle.fib`'s `main` prints under `fibc run` (14 lines) and checks the length rule, truncation (and that nothing is written past `cap`), `cap` 0, a `NULL` buffer, a negative `cap`, an exact fit,
  the retry rule, a string cut by `len`, an embedded NUL, `NULL` with length 0, a negative `len`, independent results, and that 400000 calls with 4 KB strings do not grow the process (a leak or a double release fails it).
- **renderer** (`solid-lib.fib`: `render(text: str) -> str` over fib-solid's `solid.cli/answer`): `libsolid.so` (1.6 MB, 12 s) with its header; only `render` is exported; `solid-host.c` calls it for three models (`(cube 1 2 3)`, `(difference (cube 10 10 10) (sphere 6))`, and an unclosed form, which answers an error JSON) and the JSON is byte for byte what
  `fibc run` of fib-solid's `demo/render-main.fib` prints (`cmp`); each needs the retry (the first buffer is 64 bytes).

### 6.2 The Mac (by hand)

`compiler/tests/shared/shared-mac.sh FIBC` (a stage 2 built on the Mac from this tree) builds the same `kernels.fib` as `libkernels.dylib` and, with `SOLID=DIR`, the renderer as `libsolid.dylib`; checks the install name (`otool -D`), `nm -gU` against the
declared names, the module map and the header; compiles `main.swift` / `solid-main.swift` with `swiftc -I DIR` and `import kernels` / `import solid`; and compares the output with `fibc run`. **The gate does not run it** (the gate runs on Linux; `mk/mac.mk`'s `mac-check` lists `sh-shared`
as skipped in `compiler/tests/expected-macos.txt`, with the reason). 
Result, run on the M1 Ultra (macOS, Swift 6.4, Homebrew llvm@21; a stage 2 built there from this tree by the 0.1.17 seed in 120 s; everything under `~/fibber-a64-scratch`, nothing installed, no process left): all 14 checks `ok`, last line `shared-mac: ok`:
the dylib builds with no linker warning, its install name is `@rpath/libkernels.dylib`, `nm -gU` shows exactly the 16 exports (and, since 2026-10-22, `_fib_status` and `_fib_error`), the header is valid C11, `swiftc -I DIR` with `import kernels` compiles `main.swift`, the Swift tool's own checks (retry, truncation, nil buffer, 100000 string calls) hold and
its 14 lines equal `fibc run`'s; the renderer dylib exports only `render`, and a Swift tool's JSON for the three models (3988 bytes) is byte-identical to `fibc run` of `render-main.fib`. The macOS deployment-target warning the study met did not appear in this run (not investigated further).

### 6.3 Planted faults

Each was put in the compiler, a stage 2 built from it, and `shared.sh refusals kernels` run: 

| planted fault (in the compiler) | what the gate said |
|---|---|
| the bytes copied in for a `str` parameter are one short (wrong length in the lowering) | `host.c` checks fail (`greet needs 13 bytes`, truncation, NULL buffer ..), and the host's lines differ from `fibc run` (`greet=hello, worl!`, `count_bytes=5`) |
| the result is the number of bytes copied, not the length needed | `host.c: every check held` fails (truncation and retry) |
| the last export's wrapper is dropped (a missing export) | `nm -D` differs from the declared names, `host.c` does not link (`ld returned 1 exit status`), the host's lines differ |
| `fib.export-ready` made external, the version script as it is | the gate **holds**: the version script hides it (the second guard of decision 2 doing its work) |
| `fib.export-ready` made external and the version script not hiding the rest (a symbol left visible) | `nm -D --defined-only` shows `fib.export-ready`: `exactly the exports` fails |
| the `str` parameters are never released (a leak) | `the process does not grow` fails (400000 calls of 4 KB strings) |
| an owned `str` parameter is also released by the wrapper (a double release) | `host.c` crashes (exit 139) and its lines differ |


## 7. Where the code is

| file | what |
|---|---|
| `compiler/emit/export.fib` | the signatures of the exports and the wrapper (the ABI) |
| `compiler/emit/exporthdr.fib` | the C header and the module map |
| `compiler/emit/compile.fib` | `compile-library` uses them; `Emitted` carries the signatures |
| `compiler/driver/shared.fib` | the flags' checks, the build, the header files |
| `compiler/native/link.fib`, `api.fib` | `link-shared`: `cc -shared` / `-dynamiclib`; `build-shared-module` from the module's trees (no symbol list since 2026-10-25) |
| `compiler/fibc.fib`, `driver/args.fib`, `driver/commands.fib` | `--shared`, `--header` (FIB_SHARED, FIB_HEADER), the usage text |
| `compiler/tests/shared/` | the gate: `shared.sh`, `kernels.fib`, `oracle.fib`, `host.c`, `solid-lib.fib`, `solid-host.c`; by hand: `shared-mac.sh`, `main.swift`, `solid-main.swift` |
