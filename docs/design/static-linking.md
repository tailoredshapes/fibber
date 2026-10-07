# Static linking: `fibc build --static`, link declarations, scratch images, Lambda

Status: STATIC-1 (2026-10-06): built and measured. Where this file and the code disagree, the code is the truth and this file is to be fixed.
The owner: "I will release my code more frequently than I will update my operating system. Being able to deploy in a scratch docker image is kinda cool, and it should make
deploying to things like lambda less confusing." Later stages (not here): a native HTTP/1.1 and TLS 1.3 client over `fib.crypto`, a Lambda runtime library.
What fibber bundles and what it assumes from the platform, as an architecture question: [platform-boundary.md](platform-boundary.md); the rule that keeps native access in few places: ADR 0011.

## 1. What `fibc build --static` is

```
fibc build --static PROGRAM.fib -o program                      the host's architecture
fibc build --static --target aarch64-unknown-linux-gnu P.fib -o program-arm64    arm64 from an x86-64 machine (or either from the other)
```

The result is an ELF executable with no INTERP segment and no dynamic section: `ldd` says "not a dynamic executable". It needs the Linux kernel and nothing on disk. `--static` selects
the **musl row** of the target table (`fibc targets`: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`; a `-gnu` triple of the same architecture means the same row), and
`FIB_STATIC=1` does the same from the environment. The code is the Linux code at the cross-build CPU (`x86-64-v3` on x86-64, with the start-up CPU check of ADR 0008; generic AArch64
with its NEON and FMA baseline), so a static binary runs on any machine of the architecture family, not only the one that built it.

## 2. The toolchain, measured, and why musl

Candidates measured on this machine (Ubuntu, x86-64, `cc` is gcc 15, `aarch64-linux-gnu-gcc` and `qemu-aarch64` installed, no `musl-gcc`, `zig`, `clang`, `lld`):

| Option | Result |
|---|---|
| static **glibc** (`cc -static`) | links and runs what fibber uses today (no warnings for hello or the census program), but 12x larger (hello 970 KB against musl's 77 KB; the census program 1.27 MB against 371 KB), and glibc's static `getaddrinfo`, NSS and `dlopen` load shared libraries at run time, which a `FROM scratch` image does not have: exactly what the DNS and TLS stages will reach for, failing in the field, not at link time. Rejected. |
| **musl** with `ld` (chosen) | musl 1.2.5 built from its pinned tarball with the machine's gcc (`scripts/build-musl.sh`): `libc.a` 2.7 MB, five small objects. The program is linked by `ld -static -nostdlib`, so no libc of the build machine is read. hello is 77 KB, the demo (JSON, tasks, an HTTP server) 469 KB. Works for both architectures from the same x86-64 machine. |
| `zig cc` | would give musl and a cross linker in one download (0.17.0, sha256 `1cbe9df9...e2026`, 57 MB). Not used: the machine has what is needed, and a 57 MB toolchain is the wrong thing to make a dependency. It remains the answer for a machine with no aarch64 `ld` (below). |

Pinned: musl 1.2.5, `a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4` (checked by the script against `https://musl.libc.org/releases/musl-1.2.5.tar.gz`).
Licence: MIT, kept as `MUSL-LICENSE` beside the pieces.

**The toolchain is not a dependency of an ordinary build.** `fibc build` without `--static` is unchanged (it uses `cc` and the system libc). `--static` needs two things and says which is missing:
- the **musl pieces** for the architecture: `crt1.o crti.o crtn.o libc.a libgcc.a` (and `fibshim.o` on x86-64), found in `$FIB_MUSL_DIR/ARCH` (or `$FIB_MUSL_DIR`), else `share/fibber/musl/ARCH` beside the
  compiler (a release built with `WITH_MUSL=1 scripts/package.sh` ships them; 5.7 MB compressed for both). Build them with `scripts/build-musl.sh x86_64 DIR` (needs gcc and make; `aarch64` needs `aarch64-linux-gnu-gcc`). Without them:
  `--static needs the musl pieces for x86_64 (crt1.o ...), not found in DIRS: build them with scripts/build-musl.sh ... and set FIB_MUSL_DIR`.
- a **linker** for the architecture: `ld` on x86-64; `aarch64-linux-gnu-ld`, `ld.lld` or `ld.gold` on PATH for aarch64 (or `FIB_STATIC_LD`): `--static for aarch64 needs a linker for it on PATH (...)` otherwise.
`--emit obj|asm|llvm` with `--target ...-musl` needs neither.

## 3. Native libraries: link declarations and modes

An `extern` may name the native library it lives in: `(extern curl_easy_init :private () -> i64 :lib "curl")` (spec/syntax.md 3.15). Under ADR 0011 only a platform module or a driver writes it.
- **Per module, per reachable extern.** The emitter notes `;; link-lib: curl fib.http.client.ffi` in the lIR when an extern with `:lib` is *reached* (the emitter already declares an extern only where it is called), and `fibc build`
  links the libraries it finds. A program that requires `fib.http.client` and never calls it links with no libcurl present; one that calls it gets `-lcurl` with no flag from the user: the break
  that a program using `fib.http.client` needed `-l curl` is fixed. (An unused module never forced a link: the emitter drops unreachable definitions, so `--gc-sections` is not needed; the static link passes it anyway.)
- **The mode of the build decides.** `dynamic` links `-lNAME` (resolved when the program starts: with `cc`, an rpath for each `-L`); `static` bundles the archive `libNAME.a` (`-l:libNAME.a`: the "uberjar" option).
  `fibc build --link curl=static|dynamic` sets one library; `--link-mode static|dynamic` all; the default is dynamic, and static under `--static` (a static executable has nothing to resolve a dynamic library with:
  asking for dynamic is an error that says so). `FIB_LINK` and `FIB_LINK_MODE` are the same in the environment. `c`, `m`, `pthread`, `dl`, `rt`, `util` are the C library's and never a link word.
  A static archive's own dependencies (libcurl.a needs libssl, libz, ...) are the user's to add with `-l` and `-L`; fibber does not read pkg-config. A project-file setting (`deps.fib`) is NOT done.
- **Errors name the module and the library.** `fibc: module fib.http.client.ffi needs the native library curl (declared by :lib "curl" on its externs): not found; install it (libcurl.so for a dynamic link, libcurl.a
  for --link curl=static or --static) or give its directory with -L DIR`. It is added to the linker's own text when the linker says it cannot find the library; the module is the one that wrote the extern.
- Tests: `compiler/tests/driver/linklib.sh` (a fake library nobody has: required-not-called links and runs, called fails naming module and library, both modes; a real libfibtest built from C: dynamic, bundled, and inside
  a `--static` executable; the refusals), cases 8000 (parses, runs), 8001 and 8002 (rejects). ADR 0011's three rules, each with a plant, check that no other module declares `extern`.

## 4. What failed on musl, and the fixes

Everything else of the 102 C names the runtime and `fib.os` use exists in musl (checked: the census programs, the demo, the Lambda bootstrap and the case suites of section 7 link and run).

| What | Symptom | Fix |
|---|---|---|
| `__x86_get_cpuid_feature_leaf` (glibc 2.33+: the start-up CPU check of ADR 0008 calls it) | undefined at link | `rt/static/cpuid.c`: the same function over `cpuid`/`xgetbv`, compiled by `build-musl.sh` into `fibshim.o`, linked on x86-64 only; aarch64 has no check |
| `mallopt(M_MMAP_THRESHOLD)` (`rt/alloc.lir`, the large-block cache) | undefined at link | `emit.os` drops the call for a musl target, as it does for Darwin; musl's malloc maps blocks of 128 KiB and more on their own already; and it maps and unmaps a group for every block from about 2 KiB, so `emit.os` also sets the cache's floor to 4 KiB there (allocator.md 9: the `strings` benchmark's 24,339 mmap calls) |
| thread stack: musl's default for a thread is **128 KiB** (glibc's: 8 MiB from `ulimit -s`) | a task recursing 40,000 deep dies of SIGSEGV | `rt/thread.lir` `fib.thread-attr`: every thread (task and pool worker) is created with an explicit 8 MiB stack. Case 8003 recurses in a task; with the stack set to 64 KiB it fails with status 139 (the planted fault), with 8 MiB it passes |
| soft-float on aarch64 (`strtod` wants `__trunctfdf2`) | undefined at link | `libgcc.a` of the cross gcc ships with the pieces |
| `-lm -lpthread` | not needed (musl has them in libc.a) | the static link line has none |
| `FIB_ALLOC_DECAY_MS`, the cache's `madvise`, `clock_gettime(CLOCK_MONOTONIC_COARSE)` | work unchanged (musl uses the vDSO in static binaries) | none |
| signal alternate stack | the runtime installs no signal handler and no alternate stack | none (platform-boundary.md 1) |
| `getentropy`, `pipe2`, `accept4`, `poll`, `fmemopen`, `mkdtemp`, `strftime` | exist in musl | none |
| `getaddrinfo` without NSS | HTTP-1 calls it (`fib.os.net/resolve-host`, the platform layer, blocking); `fib.os.net/connect-tcp` itself still takes numeric addresses; DNS was libcurl's before | later stage: musl's resolver reads `/etc/resolv.conf` and `/etc/hosts` itself, no NSS, so a client can use `getaddrinfo` from the scratch image provided those files are mounted |

Note: **libc `malloc` is still the allocator under every fibber program.** The runtime's free lists and large-block cache sit on top of it (`malloc`, `calloc`, `free`, `malloc_usable_size` are in the census of a hello world).
musl's malloc is the slower of the two; section 6 measures what that costs.

## 5. Run in a scratch image and as a Lambda custom runtime

`scripts/static-demo.sh` builds and runs everything below (it removes its images, tag `fibber-static-demo:*`, at the end). The program is `examples/static-demo/demo.fib`: file read, JSON parse, four tasks, and an HTTP server
(`fib.http.server`: native sockets, no libcurl).

```
FROM scratch
COPY demo /demo
COPY data.json /data.json
ENTRYPOINT ["/demo"]
CMD ["batch", "/data.json"]
```

Measured: section 7 (binary and image sizes, the run output, arm64 under `docker run --platform linux/arm64`, which works through binfmt/qemu-user).

**Lambda custom runtime, without AWS.** `examples/static-demo/bootstrap.fib` is a minimal Runtime API loop (`GET /2018-06-01/runtime/invocation/next`, `POST .../ID/response`) over raw sockets (`fib.os.net`), as a demo and NOT a library: the
library has no HTTP client without libcurl yet. It is built `--static` for both architectures and run in a scratch image against `examples/static-demo/fake-runtime-api.py` (python3 `http.server`, three events). The AWS Lambda
Runtime Interface Emulator was not fetched and no AWS account was used. A real Lambda wants `bootstrap` at the root of the zip, executable, and the architecture of the function (`x86_64` or `arm64`) matching the build; that is all the static
binary adds to the usual: no `provided.al2023` library to match.

## 6. Release engineering

`scripts/package.sh` ships the compiler tarball as before. Decision: the musl pieces are **optional**, behind `WITH_MUSL=1` (`share/fibber/musl/x86_64/`, and `aarch64/` when `aarch64-linux-gnu-gcc` is installed), because they are 5.7 MB
compressed for the two architectures (the runtime is lIR compiled per program, so libc is all static linking needs) and a release that does not want them is unchanged. The release flow, the `SEED` and CI are untouched; no release was cut. The
pieces are built from the pinned tarball, with the licence beside them (`MUSL-LICENSE`). Not done: building them in `release.yml`, and an aarch64-host tarball.

## 7. Measurements

Commands: `scripts/static-demo.sh`, `scripts/bench/static-compare.sh`, `FIB_STATIC=1 FIB_STATIC_CASES=1 fibc cases DIR`.

Machine: x86-64 Linux, shared with other work (the benchmark waited on the suite lock; deltas under about 5% are noise). Both builds use the same CPU (x86-64-v3) and the same lIR.

**Sizes** (stripped of nothing; `ls -l`):

| Program | dynamic (glibc) | static (musl) |
|---|---|---|
| hello world | 28,048 B | 77,672 B |
| census (json, task, atom, sql) | 320,296 B | 371,344 B |
| demo (json, 4 tasks, HTTP server), x86-64 / aarch64 | | 469,160 B / 516,960 B |
| Lambda `bootstrap`, x86-64 / aarch64 | | 253,368 B / 299,256 B |
| `FROM scratch` image, `docker image ls` | | 469 kB (amd64), 517 kB (arm64) |

**Run output.** `docker run --rm fibber-static-demo:amd64` (and `--platform linux/arm64` for the other image, under binfmt/qemu-user) printed

```
file /data.json: 51 bytes, JSON object
compact: {"name":"fibber","tags":["static","musl"],"n":3.5}
sum of squares below 1000000 over 4 tasks: 333332833333500000
```

`serve 8080` answered `curl localhost:18080/json` with `HTTP/1.1 200 OK`, `content-type: application/json` and `{"static":true,"libc":"musl"}`. The bootstrap, run in a scratch image on both architectures against the fake Runtime API, answered the
three events (`{"greeting":"hello ada","sum":10,"runtime":"fibber, static musl"}`, ...) and the fake saw all three responses.

**The case suites against the static build** (`FIB_STATIC=1 FIB_STATIC_CASES=1 fibc cases DIR -j 2`: each case built with `--static`, run with `FIB_TRACE=1`, judged by its own header, leak audit included):

| Directory | Result |
|---|---|
| cases/ownership | 354 cases: 354 pass |
| cases/modules | 32 cases: 32 pass |
| cases/stdlib | 1318 cases: 1296 pass, 2 fail, 20 open (the open cases fail as their label says, as in the dynamic run) |

The two failures: **1707** (`a def atom holds what the program puts in it`: `leaks=2`) fails the same way as a *dynamic* executable (`FIB_STATIC_CASES=1` without `--static`), so it is a difference between the JIT run and a built executable
(a `def` atom alive at exit), not musl's; and **7080** (`tensor vector exp, log, tanh within ulp bounds of libm`: result 256) is a real musl difference: the case compares the library's vector math with whatever libm is linked, and musl's `exp`,
`log` and `tanh` are not as accurate as glibc's, so a bound set against glibc fails against musl (passes with `FIB_STATIC_CASES=1` alone). A program that calls libm scalar functions gets glibc's last-ulp results dynamically and musl's statically.

**Benchmarks** (`scripts/bench/static-compare.sh`, median of 3, the programs of `quick.sh` plus three multi-threaded ones; same answer in every pair):

| benchmark | dynamic | static | delta |
|---|---|---|---|
| num-f64 | 1.662 s | 1.739 s | +4.7% |
| num-nbody | 0.775 s | 0.738 s | -4.8% |
| vec-conj-pop | 0.964 s | 1.089 s | +13.0% (+9.0%, n=7) |
| vec-index | 0.421 s | 0.452 s | +7.3% |
| vec-sort | 0.499 s | 0.526 s | +5.5% |
| map-assoc-get | 0.301 s | 0.340 s | +13.2% (+0.5%, n=7) |
| set-conj | 0.246 s | 0.271 s | +10.1% |
| lazy-fused | 0.944 s | 0.895 s | -5.2% |
| lazy-bound | 0.050 s | 0.050 s | -1.1% |
| **strings** | 0.732 s | 1.417 s | **+93.5%** (+89.6%, n=7) |
| binary-trees | 1.460 s | 1.600 s | +9.6% (+6.2%, n=7) |
| atom-contention | 1.082 s | 1.089 s | +0.6% |
| parallel-closure | 1.078 s | 1.084 s | +0.5% |

Most programs are within noise to +10%. `strings` is nearly twice as slow, and `strace -c` says why: it concatenates strings of up to 100 KB, and musl's allocator gives such blocks back to the kernel every time (24,339 `mmap` and 14,304 `munmap`
calls, 0.13 s of system time) where glibc's sliding threshold keeps them (17 `mmap`, 5 `munmap`). The runtime's large-block cache is the answer (docs/design/allocator.md) but starts at 128 KiB and, on musl, its `mallopt` is gone; the blocks here sit below it.
**So libc `malloc` is still used by every program and is the one place musl costs real time.** Options, not done: start the cache lower on musl, or link a better malloc into the static pieces. `perf` is not permitted on this machine, so this is
the system-call count, not a profile.

## 8. Notes for later stages

A scratch image has: no `/etc/ssl/certs` (a TLS client must carry its CA roots: embed a bundle in the binary or mount one; there is no system store to read), no NSS and no `/etc/nsswitch.conf` (musl's resolver needs `/etc/resolv.conf` and optionally
`/etc/hosts`, which the container runtime provides), no `/usr/share/zoneinfo` (every time-zone conversion is UTC unless the program carries its own tz data or a `TZ` rule string; `gmtime_r` and the HTTP date are unaffected), no `/etc/passwd` (`getpwnam`
is not used), no shell and no `/tmp` (`fib.os/temp-directory` returns `$TMPDIR` or `/tmp`: mount a tmpfs where a program writes temporary files), `/dev/null` and `/proc` only as the runtime provides them. TLS 1.3 itself: nothing in the static build forbids it; the
entropy it needs is `getentropy` (works). A Lambda runtime library will want: the Runtime API client (HTTP/1.1, keep-alive), the environment (`AWS_LAMBDA_RUNTIME_API`, `_HANDLER`, `LAMBDA_TASK_ROOT`), the 10 MB-class responses, and `/tmp` (the only
writable path, 512 MB).

## 9. Not done

A boot of the binary as PID 1 under qemu-system (no qemu-system on this machine: platform-boundary.md 4); `zig cc` as a toolchain; a `deps.fib` project setting for link modes; static builds of the compiler itself; macOS (no static executables there); riscv64 and wasm
(the rows exist, `--static` refuses them); the AWS Lambda Runtime Interface Emulator; any use of AWS.

## HTTP-2 note: name resolution without libc

`fib.http` used `getaddrinfo`, which in a static musl binary needs `/etc/hosts` and `/etc/resolv.conf` in the image (and under glibc, NSS and shared libraries a static binary cannot load). HTTP-2 added a native
resolver (`fib.dns`, docs/design/dns.md) that the client uses by default: it reads `/etc/resolv.conf` and `/etc/hosts` itself and speaks DNS over UDP and TCP, with documented fallbacks when the files are
absent (no nameserver means `127.0.0.1`). A static binary alone in a `FROM scratch` image with only a bind-mounted `resolv.conf` resolves and fetches (`scripts/static-fetch-demo.sh`). `(with-system-resolver o)` keeps
`getaddrinfo` for hosts whose names live in NSS only.
