# The stack: its size, its guard, and what an overflow says

Decided with the owner's ruling of LANG-2: "Silent exit is very bad." A program that recursed past its stack used to end with SIGSEGV and no word
(status 139 under `fibc run` and for an executable, nothing on standard error); the Cedar library's author met it at about 40,000 frames of a
left-deep chain, which is far below what 8 MiB holds of a small frame, because frames of real functions are hundreds of bytes.

## The rule

1. **An overflow is a fatal failure and it says so.** Every thread that runs the program has a guard below its stack and a signal stack of its own
   (`sigaltstack`). One handler for SIGSEGV and SIGBUS reads the bounds of the faulting thread's stack, kept at the start of its signal stack. A fault
   within one MiB of the end of the stack is `trap: stack overflow` on standard error, then `abort`: status 134, the status of every trap
   (docs/adr/0001, 0009).
2. **A fault that is not a stack's is never taken for one.** The handler writes `fatal signal N at 0xADDR`, puts the signal's default action back and
   returns, so the instruction faults again and the process dies of the signal itself (status 139 for SIGSEGV, 135 for SIGBUS, a core where the system
   writes one). A memory bug in `unsafe` code or in a library's `extern` is therefore visible, and distinguishable from a recursion that was too deep.
3. **It cannot be caught.** Inside a `try` the overflow is on the fatal list (docs/adr/0009): the stack that a handler or an unwinder would run on is
   gone, and a `finally` would run on the same exhausted stack. The handler does not return into the program. What `try` guarantees is unchanged: it
   catches traps; this is not one the program can survive. In a task it is not isolated either (docs/adr/0001): `join` never answers it, the process ends.
4. **The handler is async-signal-safe.** It calls `write`, `sigaltstack`, `abort` and `sigaction` and nothing else, and uses no allocation: its buffer is on
   the signal stack. It works whether the overflow was in generated code or in the runtime's C library calls (`memcpy` and the others run on the same stack).

## The main stack

The stack of the process's first thread is whatever `ulimit -s` says (8 MiB by default), and it varies from system to system, so the depth a program may
reach did too. The runtime now runs the program (`main`, and the initialisers of the `def`s) on a thread of its own whose stack it makes:

- **Size: 256 MiB**, reserved and committed as it is touched (an `mmap`'d thread stack: a program that recurses little uses a few pages), with a guard of 1 MiB
  below it. `FIB_STACK_MB` sets it, in MiB: `FIB_STACK_MB=1024` for a deeper recursion, `FIB_STACK_MB=0` for the process's own stack (the old behaviour: the
  limit is `ulimit -s`, and the guard of the kernel's stack is the one the handler reads). A value that is not a whole number from 0 to 1048576 is a
  fatal error that names the variable (`trap: FIB_STACK_MB is not a whole number of MiB from 0 to 1048576`): a silent default would hide a misspelling.
- A system that will not make the thread (an address-space limit under `ulimit -v`, `vm.overcommit_memory=2`) runs the program on the process's own stack,
  as `FIB_STACK_MB=0` does, and the guard bounds come from `RLIMIT_STACK` (unlimited counts as 1 GiB). That is the one silent fallback, and it is a
  fallback to the old behaviour, not a failure.
- The first thread of the process waits in `pthread_join` and is named `fib-parked` (Linux `prctl(PR_SET_NAME)`), so that `fib.os.process`, which refuses
  a `fork` while another thread exists (it counts `/proc/self/task`), does not count it.
- `fibc run` is the same: the JIT'd `main` makes its stack the same way, so a program run by `fibc run` and the executable of `fibc build` reach the same
  depth. The compiler itself is a fibber program and has the same stack (its own deep recursions over a long left-deep chain of an expression are the same case).
- Signal handlers the process had before are put back when the program returns (a JIT'd handler would otherwise point into code that is gone).

Tasks and the pool's workers keep 8 MiB (`fib.thread-stack`), with a guard of 64 KiB: a thousand tasks must not reserve a thousand times 256 MiB. A program
that recurses deeply in a task is told so by the same message; it can run the deep part in `main`.

Measured (a function that adds one to the result of itself: `(if (= n 0) 0 (+ 1 (depth (- n 1))))`, 16 bytes a frame, x86-64 Linux, glibc, `ulimit -s 8192`):

| | before | after |
|---|---|---|
| deepest recursion that completes | 523,612 frames | 16,724,624 frames |
| one past that | SIGSEGV, status 139, no output | `trap: stack overflow`, status 134 |

## Cost

One `pthread_create` and `pthread_join` per run, one `sigaltstack` and 64 KiB of `malloc` per thread that runs the program, and one `sigaction` per signal at
start. Nothing on the call path: a call costs what it did. Measured (2000 runs of a hello world in rounds of 500, alternating the two builds, on a machine at a
load of 13, so about 0.05 ms of noise): 0.57 ms a run before and 0.68 ms after, about 0.1 ms more; the executable grows from 28.2 KB to 37.0 KB (the guard, the
handler and the thread start). `FIB_STACK_MB=0` skips the thread. A program that runs for a second does not see it.

## The targets

| target | the program's stack | what an overflow says | checked |
|---|---|---|---|
| Linux glibc, `fibc run` (JIT) and `fibc build` | 256 MiB thread | `trap: stack overflow`, 134 | cases 8350-8354 (JIT), `compiler/tests/stack/stack.sh` (AOT) |
| Linux musl (`--static`) | the same | the same | `stack.sh` with `FIB_MUSL_DIR` set: main overflows with the message, and a deep recursion runs under `FIB_STACK_MB=64` |
| macOS (Darwin, arm64 and x86-64) | the same; `emit.os` adapts the structures that differ (`stack_t`, `siginfo_t`, `struct sigaction`) and the numbers (SIGBUS is 10) | the same: a guard page of a thread stack faults as SIGSEGV or SIGBUS, both are handled | **not run on a Mac in this package**: written to the ABI, to be run by `scripts/mac-check.sh` and `compiler/tests/stack/stack.sh` there |
| wasm32-wasi | the host's | the runtime has no signals and no threads (`emit.wasi` leaves the guard out). node: `compiler/tests/wasm/run.mjs` catches V8's `RangeError: Maximum call stack size exceeded` and prints `trap: stack overflow (Maximum call stack size exceeded)`, status 134. wasmtime: its own error (`wasm trap: call stack exhausted`, status 134), which the program cannot change. A host of the program's own (the JS host of `--export` libraries) sees the engine's `RangeError` | `wasm.sh` check `stack` (node); wasmtime run by hand |

The JS backend of lIR (`compiler/js`, for the programs of `cases/lir`) is not the fibber runtime and is unchanged: it ends a `RangeError` of the call stack as
a SIGSEGV, as the native lIR does.

## What else ends a process without a word

Searched in `rt/` and the library (every `exit`, `_exit`, `abort`, `pthread_exit` and signal):

- `abort` is reached only through `fib.fatal-bytes` (which prints) and the wasm shim's `pthread_exit` (never reached: wasm has no thread);
  `fib.fail-task` ends a task's thread after printing `trap in task:`. `_exit` is `fib.os.process` `child-run`'s, the forked child's normal end.
- Out of memory is `trap: out of memory` (a failed `malloc` is checked in `fib.alloc`); the kernel's OOM killer is SIGKILL and nothing can print:
  the shell shows status 137. A thread that cannot be started is `trap: spawn: cannot start a thread`.
- A SIGSEGV/SIGBUS that is not a stack's now says `fatal signal N at ADDR`. SIGFPE and SIGILL are not handled and not changed by this package (integer division by zero is a checked trap,
  `integer / by zero`; the start-up CPU check of docs/adr/0008 prints its own message before it exits).
- SIGPIPE: not touched by this package, and not checked. The compiler driver ignores it (`driver.native/ignore-sigpipe`) so that `fibc emit | head` ends with
  status 0; what a program built with `fibc build` does on a write to a closed pipe is the system's default (death by the signal, status 141 in a shell): the one
  silent signal that remains, by the convention of pipes, and listed here so that it is not mistaken for this bug.
