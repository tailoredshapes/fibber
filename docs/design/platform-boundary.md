# The platform boundary: what fibber bundles and what it assumes

Status: design and census (STATIC-1, 2026-10-06). The census is measured (`nm -u` of the objects, the `declare`s of `rt/*.lir`, the `extern`s of `lib/`); the tiers B2 and C
are an evaluation, not code. Where this file and the code disagree, the code is the truth and this file is to be fixed.

The owner's question: fibber should run in a `FROM scratch` image, on a raw VM, and one day be what an operating system is written in. What the language bundles and what it
assumes from the platform is therefore an architecture decision. ADR 0011 makes the rule executable: `extern` lives only in the platform layer (`fib.os`, `lib/platform/`, the
runtime) and in drivers (one module per native library); every other module reaches the platform through the interface below or not at all.

## 1. The platform interface

The runtime (`rt/*.lir`, emitted into every program) and `fib.os` need these services, and nothing else of the platform (the census: 102 distinct C names in `rt/` and `lib/`, of
which 47 are plain system calls and 18 are pure functions that need no platform at all). Each row: the service, the Linux system call it is today, and the libc function that wraps it.

| Service | Linux syscall | libc function(s) today | Used by |
|---|---|---|---|
| **memory pages in/out** | `mmap`, `munmap`, `mremap`, `brk`, `madvise` | `malloc` `calloc` `realloc` `free` `malloc_usable_size` (`mallopt` on glibc), `madvise` | every object; `rt/core.lir` free lists and `rt/alloc.lir` large-block cache sit ON TOP of libc malloc |
| **clock** | `clock_gettime` (vDSO) | `clock_gettime` (CLOCK_MONOTONIC, _COARSE, REALTIME) | `fib.os.time`, the cache's decay, `sys-clock-now` |
| **sleep** | `nanosleep` | `nanosleep` | `rt/sys.lir` |
| **entropy** | `getrandom` | `getentropy` | `fib.os.random` |
| **threads** | `clone`, `exit`, `set_tid_address`, TLS (`arch_prctl`) | `pthread_create` `pthread_detach` `pthread_exit` `pthread_attr_*` | tasks, the pool |
| **locks and waiting** | `futex` | `pthread_mutex_*` `pthread_cond_*` `sched_yield` | run queue, joins, `fib.lock` (atoms use their own spinlock) |
| **thread-local slot** | TLS | `pthread_key_create` `pthread_getspecific` `pthread_setspecific`, `__errno_location` | the current task (traps), the exception key, errno |
| **processor count** | `sched_getaffinity` / `/sys` | `sysconf(_SC_NPROCESSORS_ONLN)` | pool size |
| **fd I/O** | `read` `write` `close` `openat` `lseek` `dup` `dup2` `pipe2` `fcntl` `poll` `ioctl(TCGETS)` | the same names, `isatty` | `fib.os.io`, `println` (`write`), `rt/io.lir` (also `fopen` `fread` `fwrite` `fclose` `ferror` `ftell` `setbuf` `fmemopen`: the C stdio, which is a libc service, not a syscall) |
| **files and directories** | `mkdir` `rmdir` `unlink` `rename` `readlink` `getcwd` `chdir` `getdents64` | the same, `opendir` `readdir` `closedir` `realpath` `mkdtemp` | `fib.os.files`, `fib.os.directories` |
| **sockets** | `socket` `bind` `listen` `accept4` `connect` `send` `getsockopt` `setsockopt` `getsockname` | the same, `inet_pton` `inet_ntop` (pure) | `fib.os.net`; addresses are numeric (no resolver is called) |
| **processes** | `clone` `execve` `wait4` `getpid` `getppid` `uname` `alarm` | `fork` `execvp` `waitpid` `getpid` `getppid` `uname` `alarm` `system` | `fib.os.process`, the compiler's link step |
| **exit and trap** | `exit_group`, `tgkill`(SIGABRT) | `_exit`, `abort` | `trap` writes its message then `abort` (status 134) |
| **environment, arguments** | the initial stack (`argv`, `envp`) | `getenv` `setenv` `unsetenv`, `main(argc, argv)` | `args`, `fib.os.environment` |
| **signals, trap handlers** | none installed | none | the runtime installs no handler: a trap is an explicit call, stack overflow is the kernel's SIGSEGV; the compiler ignores SIGPIPE (`signal`) |

Pure functions the runtime and libraries take from libc but that need no platform (a port implements them once, or fibber does, or LLVM provides them): `memcpy` `memcmp` `memset`
`strlen` (LLVM emits them), `strerror`, `snprintf` (`%f`, `%.17g`: `fib.fmt`, `fib.json`), `strtod` `strtof`, `trunc` `truncf` `sqrt` (LLVM intrinsics), `gmtime_r` `strftime`
(the HTTP date), `inet_pton` `inet_ntop`. Not used anywhere: `dlopen`, `getaddrinfo` (no DNS), `setlocale`, `iconv`, `mmap` directly, `signal` handlers, `getpwnam`.

Where it is cheap, the interface already goes through one place: the runtime's C functions are one list (`runtime-c-functions` in `compiler/types/targets.fib`, checked against the
`declare`s of `rt/` by `compiler/tests/native/targets-emit.sh`), each target row says which its C library lacks (`libc-lacks`), and the OS differences of the runtime are one
module, `compiler/emit/os.fib` (`sys-for`: Darwin's names, musl's missing `mallopt`). A later tier is a new row and a new case in `sys-for`, not a rewrite. What is NOT
one place: `fib.os`'s own `extern`s (spread over six files, which is where ADR 0011 says they belong) and the six violators listed in ADR 0011. This package does not move them.

## 2. The three tiers

**A. Hosted libc (today's default build).** Assumes: a Linux or macOS with a dynamic loader and the platform's libc (glibc 2.33+ for the start-up CPU check's
`__x86_get_cpuid_feature_leaf`), `cc` to link, and for any native library a development package. A port supplies: nothing, it is the reference.

**B. Static executable on Linux.**

*B1, musl (built: `fibc build --static`, docs/design/static-linking.md).* Assumes the Linux kernel's system-call interface and nothing on disk: no loader, no shared library, no `/etc`.
It links musl's `libc.a` (2.7 MB, 1.2.5, pinned and checked by sha256) and `libgcc.a` (soft-float routines aarch64 needs for `strtod`), so every row of the table above is musl's
implementation. The runtime needed three changes for it (thread stack size, no `mallopt`, a CPUID shim); the case suites pass against it with two named exceptions (docs/design/static-linking.md 7). A port supplies the musl pieces for its architecture.

*B2, direct system calls, no libc at all (Go-style; evaluated, NOT built).* The interface above is 102 names (counting the Darwin spellings): about 45 are system calls with a thin wrapper (a freestanding
`syscall` primitive and a table, a day of work), about 18 are pure (LLVM or fibber code replaces them), and the rest is what libc really provides and fibber would have to write:
1. **a general allocator** (`malloc` `calloc` `realloc` `free` `malloc_usable_size`): the runtime's own allocator is NOT a replacement. `rt/core.lir` keeps per-size free lists and `rt/alloc.lir` a
   large-block cache, but both draw from and return to libc `malloc` (the census of a hello world shows `malloc` `calloc` `free` `malloc_usable_size` `mallopt`; `fib.alloc-slow` is `malloc`).
   A syscall-only tier needs a malloc over `mmap` (a few hundred lines; the design of docs/design/allocator.md already has the size classes);
2. **threads** (`pthread_*`, 14 names): `clone` with a stack and TLS, `futex` mutexes and condition variables, thread-local slots, `exit`. This is the largest piece and the riskiest (TLS setup per architecture, the
   detached-thread exit that frees its own stack). A single-threaded B2 would drop tasks to run inline; the language's concurrency story depends on threads, so that is a tier decision;
3. **C stdio** (`fopen` `fread` `fwrite` `fclose` `ferror` `ftell` `setbuf` `fmemopen`, used by `rt/io.lir` and `fib.os.memory-stream`): replace by `read`/`write` over fds and a memory buffer in fibber (small);
4. **`opendir`/`readdir`** (`getdents64` plus a parse), `realpath`, `mkdtemp`, `getenv`/`setenv` (the environment is the initial stack), `execvp` (PATH search), `system`, `errno` (a TLS slot).
What is lost: `getaddrinfo` and NSS (fibber calls neither today: sockets take numeric addresses, and libcurl, the only resolver, is a driver B2 cannot link), `dlopen` (nothing calls it; a
driver for a shared library cannot exist at B2, only a static one), locale (nothing uses it; `fib.fmt` formats numbers itself or through `snprintf`, which B2 replaces), `iconv`, the
vDSO if the port does not parse the auxiliary vector (`clock_gettime` then costs a system call: `fib.lc-now` was written to avoid exactly that). Verdict: B2 is feasible and buys little
over B1 on Linux (musl is already small and static: a hello world is 77 KB, the demo of docs/design/static-linking.md 469 KB), and it is the stepping stone to C: every
row B2 implements is a row C reuses. Do B2 when a target has a kernel with the Linux ABI but no libc (a unikernel, a minimal VM with only the binary), not before.

**C. Freestanding (no operating system).** Assumes: a CPU and memory. A port supplies:
- **a page provider** `(pages-get n)` and `(pages-put p n)` (and a way to learn the memory map): the allocator of B2, item 1, runs on it instead of `mmap`;
- **a trap handler**: `trap` and `abort` call it (halt, reset, or report on the console); a stack overflow and an invalid access are the CPU's faults, handled by the port, because the runtime installs none;
- **a console** `(console-write bytes)`: `println` and `eprintln` (today `write` on fd 1 and 2) go there; there are no files, no sockets, no processes, and `fib.os` is absent (a program that requires it does not build:
  the ADR 0011 rule is what makes that error a link-time list rather than a hunt);
- **a clock** (a monotonic counter) if `fib.os.time` and the cache's decay are wanted; the cache can be off (`FIB_ALLOC_DECAY_MS=0`);
- **no threads assumed**: `(spawn ..)` runs the task inline or on a port's scheduler; the atoms' spinlock and `cmpxchg` need only the instruction set;
- an entry point and a stack (`_start`), `memcpy`/`memset`/`memcmp` (compiler-rt or fibber's own), and the soft-float routines the target needs (libgcc's role in B1).
The language itself needs no more: ownership, borrowing, reference counts, tail calls and vectors are code generation, and `fib.core`, `fib.seq`, `fib.coll`, `fib.json`, `fib.tensor` are pure. Which of
them allocate is the real constraint at C, not the platform: nearly everything allocates, so C without the allocator of B2 item 1 is not a fibber.

## 3. Direction

1. Done here: B1, with the rule that gets the other tiers cheap (ADR 0011) and the single list of runtime C names.
2. Next, in this order, each a package of its own: move the six violators of ADR 0011 into `fib.os` (so the platform layer is the whole list); a `fib.os`-level `syscall` backend for Linux beside the libc one
   (`fib.os.backend` is already the seam: `lib/platform/darwin/fib/os/backend.fib` is the second implementation); an allocator over pages in the runtime (lIR) with a page provider; then B2 on top; then C by a
   port that supplies the three things above.
3. Not decided here, for the owner: whether tasks at tier C are inline or a cooperative scheduler; whether the allocator stays in lIR or moves to fibber (self-hosting favours fibber, speed favours lIR for now).

## 4. A PID-1 binary for a minimal VM

A `fibc build --static` binary needs from its parent only what a PID 1 gets from the kernel: argv, an environment, fds 0-2 (the kernel opens `/dev/console` for init when the initramfs has it), and nothing on disk. It runs as PID 1
in a container already (every `docker run` of the scratch image in docs/design/static-linking.md is a PID 1: the demo prints, serves HTTP and exits 0 there). What PID 1 adds is the program's job, not the compiler's: reaping
orphaned children (`waitpid -1` in a loop), mounting `/proc` and `/dev` if it needs them (`mount` is a system call fibber does not wrap yet), and never exiting (the kernel panics when init exits).
A boot test with `qemu-system` (kernel plus an initramfs holding only the binary) was NOT done: this machine has no `qemu-system-*` (only `qemu-user`), the user cannot use `/dev/kvm`, and a kernel image was not fetched. The claim
is therefore "a static binary has no dependency a minimal initramfs would lack" (checked: `ldd` says not a dynamic executable; `readelf -l` has no INTERP segment), and the boot itself is unproven.
