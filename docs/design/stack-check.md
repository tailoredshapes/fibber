# The stack check: a stack overflow in a catching program is a caught trap

Status: design (2026-10-22), written before the code; sections 9 and 10 are filled in with measurements when they exist. Branch `stack-guard`.
Every claim says how it was established: `RAN` = a command run in this session, `READ` = a file read.

## 0. The owner's decision, and the rule it reverses

docs/design/stack.md section 3 ("It cannot be caught") and ADR 0009 ("Fatal and never caught: .. stack overflow") were rulings of the owner. On 2026-10-22 the owner
decided that the status quo is "miserable for meat and agent alike" and **authorised revisiting the rule**: a stack overflow in a program that can catch is to be a
trap that unwinds, like `index out of range`. This document reverses the rule *for the code the compiler emits in a catching program*; ADR 0027 records the reversal and
ADR 0009 / stack.md point to it. What the old rule protected is kept as the record in section 2, because it is still true of the path it was about.

The three places it hurts (READ docs/design/live-incremental.md 11, host-threads.md 5, shared-libraries.md 0; RAN in the study of live-incremental 11):

1. `fibc run --live`: every other trap is returned to the client as an error; an overflow ends the session (restart about 10 s, 0.7 s with a standby).
2. A `--shared` library: a deep recursion on a host thread runs off the *host's* stack (exit 139), the library has no guard at all.
3. An ordinary program: `trap: stack overflow`, status 134. That is a clean failure and stays as it is (section 7).

## 1. The rule now

1. **Test the stack before using it.** A function of a catching program, on entry, compares its stack pointer with the limit of its thread. Below the limit it
   takes the failure return of the catching convention (ADR 0009 mechanism b) with the message `stack overflow`: the frames above it unwind normally, every
   owned object is released once, the audit stays exact. The overflow is an ordinary trap: caught by `try` (an exception whose message is `stack overflow`), returned as an error reply by a
   live session, returned as a status by a library export.
2. **The guard page and its handler stay as the backstop** for code that is not checked (section 8). They are unchanged: `trap: stack overflow`, abort.
3. **Where the check is, is a property of the program**: a program that catches (it calls `catch-run`, it is a live program, it is a library) has the check unless
   `FIB_STACK_CHECK=0`; a program that never catches has none (section 7). The emitted code of a program that does not catch is byte for byte what it was.

## 2. Why the old rule existed, and what of it is still true

The old rule was about a fault taken *by the hardware at the guard page*. The reasons it was fatal (READ stack.md 3; live-incremental.md 11, RAN there):

- The stack that a handler, a `finally` or an unwinder would run on is exhausted; the signal handler runs on a signal stack of its own and cannot return into the program.
- If the handler *did* return to a recovery point, the frames between are **abandoned, not unwound**, and the fault can be anywhere: inside `malloc` (the glibc arena of
  the thread stays locked), inside a runtime spinlock (`fib.lock`, the atoms), in the middle of `fib.drop`/`fib.retain` or a free-list push (a count or a list head
  half written, and the next allocation takes a block twice: host-threads.md 1). A recursion that allocates in each frame faults inside an allocation a good part of the
  time. The handler can know the address is in the guard, not what the thread was doing.

Both are still true of a fault, so **nothing recovers from the signal handler**. The new rule never faults: the check is made at a point the compiler chose, in code it
generated, with the runtime's invariants intact (no lock held, no count half written, the frame's owned objects known to the plan), and with room left on the stack
for the unwinding itself (a failure return is a few instructions and the releases the plan lists). The owner's rule is reversed because the premise (the overflow can
only be seen as a fault) is false for emitted code.

## 3. The per-thread limit, read without a call

lIR has no thread-local globals, and the JIT (ORC/LLJIT, RAN: `compiler/llvm/orc.fib`) has no TLS support for JIT'd objects; `pthread_getspecific` per call was measured
at 4x the cost of the whole call of a tiny function (RAN, `fib(40)` in C: 0.16 s without, 0.67 s with the call; a thread-local variable 0.24 s, a plain global 0.21 s).
So the limit is found **from the stack pointer itself**, in a global table, with no thread identity:

- `(stack-addr)`, a new lIR form (i64, no operand; `llvm.stacksave` cast to an integer) is the stack pointer after the frame of the function is allocated, so the
  frame's own size is already subtracted. It is machine code `mov %rsp,%rax` (RAN: llc of the check on x86-64, no frame pointer is made necessary).
- `fib.stk` is a table of 4096 words (32 KiB). The word for the 2 MiB block `b = sp >> 21` is at index `b & 4095`. A word is **one 64-bit value**, so that a racing
  thread can never read half of one: `(lo_page << 10) | npages`, where `lo_page` is `lim >> 12` and `npages` the number of 4 KiB pages from `lim` to the end of the
  usable stack inside that block (at most 512). The check is
  `d = (sp >> 12) - (w >> 10); if d >= (w & 1023) goto slow` : about nine instructions, one load, no call (RAN: llc output for the model function).
- A block that no thread has registered holds 0: `npages` is 0, `d >= 0` is always true, the **slow path** runs. `fib.stack-slow(sp)` finds the thread's record
  (a bound pair kept under a pthread key `fib.stk-key`, set when a thread enters the runtime), registers the word for this block, and answers overflow or not.
  Since only the slow path writes the table and it writes only a word that is *true* for that block (a range of addresses that lie inside the thread's usable stack),
  a fast-path pass is a proof that `sp` is inside a stack with at least the margin left: two threads' stacks do not overlap, so whoever reads a word with `sp` in its range
  is the thread whose range it is. Collisions (two blocks 8 GiB apart share an index) only make a thread take the slow path again; they never pass wrongly.
- A thread that ends clears the words of its blocks that still describe it (`fib.guard-leave`, and the pthread-key destructor for a thread the runtime did not start),
  so a stack address reused by another thread, perhaps with another size, does not inherit a stale range.
- The slow path is a `preservemost` function (a new lIR calling convention word, spec/lir.md 6.6b): the callee saves what it uses, so the caller keeps its argument registers across the
  cold call. With `ccc` the checked function had to keep its argument in a callee-saved register: its frame grew from 16 to 32 bytes and the deepest recursion halved (RAN, `down`:
  8,375,000 frames against 16,772,704); with `preservemost` the frame is the 16 bytes it was (16,758,300 frames: the margin and nothing more).
- A deep recursion takes the slow path once per 2 MiB of stack it descends, the first time through each block; a shallow program stays in one block.

**Threads fibber starts** (the program's main thread of `fib.run-main`, tasks, pool workers): `fib.guard-enter` already knows the stack's size and the top;
it now also keeps `{lo, hi}` for the thread, where `lo = top - size + guard` (the guard is counted inside `size` by glibc's mmap'd stacks; if it is not, the limit is higher
than necessary, never lower).

**Threads it did not start** (a host thread calling an export of a `--shared` library, the thread that attaches to a live session and evaluates): there is no
`guard-enter`; the first slow path on such a thread builds the record itself: `pthread_getattr_np` + `pthread_attr_getstack` on Linux (the main thread of the process is
read by glibc from `/proc/self/maps` and `RLIMIT_STACK`), `pthread_get_stackaddr_np`/`pthread_get_stacksize_np` on Darwin. The result is cached in the pthread key.
If the system cannot say (musl without `pthread_getattr_np`, an error), the thread gets an *unknown* record (`lo = 0`): it is never reported as overflowing, nothing is
registered for it, and its calls take the slow path (a getspecific and a compare) - slow, not wrong, and the guard page is the backstop. This is the one
silent fallback and it is documented here and in shared-libraries.md.

## 4. How much room is left (the margin)

The check passes while `sp - frame >= lim`; everything that runs after it and is not checked must fit between `lim` and the end of the usable stack, and the guard
page lies beyond that. **`lim = lo + 256 KiB`** (and for a stack smaller than 1 MiB, `lo + size/4`). What the room is for, and why 256 KiB:

| runs after the check, unchecked | needs |
|---|---|
| the callee's own frame, if the callee is a leaf the emitter does not check | a frame is bounded by the objects it holds; stack objects are structs, hundreds of bytes; a spill area of a SIMD kernel a few KiB |
| the runtime's lIR functions (`fib.alloc`, `fib.drop`, `fib.raise`, vector and string builders) | each a few hundred bytes of frame, nesting a handful deep (`fib.drop` of a nested value recurses: section 8) |
| libc: `memcpy`, `snprintf` (the index trap formats one on the stack: 96 bytes), `malloc` (a slow path reaches `mmap` and the arena code: a few KiB), `printf`-family calls from `fib.print` | glibc `printf` of a double uses about 1-2 KiB on x86-64; `malloc`'s worst path under 4 KiB; `qsort` and `bsearch` are not used |
| the unwinding itself | a failure return is `ret`; the releases are calls to `fib.release` |
| JIT'd code of a live session: the same, it is the same emitted code | |

256 KiB is two orders above each row, so a rare large frame does not turn a check into a fault, and it costs 0.1 percent of the 256 MiB main stack and 3 percent of a
task's 8 MiB: the deepest legal recursion is reduced by the margin and no more (a test pins it, section 6c). A frame bigger than the margin (a function that takes
a huge stack array) cannot be made by the language today (stack objects are structs of the program's types, `lower/objects.fib`); if one is ever added the check must
add the frame size to the compared address.

## 5. Which functions are checked

A recursion needs a cycle in the call graph. A body is checked when it can be part of one and is lowered to pass failures on (`unwind` is true). The graph is the typed
program's (`emit.lower.recur`, Tarjan's components over the direct calls between `defun`s, kept per program in `Program.rec-memo`): a `defun` is checked when it lies on a
cycle of direct calls (it calls itself, or two or more call each other), or when it makes a call to a target the program does not name (a closure value, a protocol method,
a `def`), because every cycle through such a call has a member that is checked. A method or a closure is checked when it makes such a call. Everything else is a bounded
leaf as far as the stack goes (it is on no cycle, and any recursion that reaches it goes through a checked function), so it is not checked, and the margin covers its frame.
(The first version checked every body that made a non-builtin call; section 9 has what that cost and why it was narrowed.)
Not checked, by construction: `def` initialisers and `async` bodies (they cannot pass a failure on: ADR 0009), functions with `array-take!` (`conj` and the other in-place
primitives of the prelude: a failure there is already fatal, so a check there could only abort; their callers are checked and the margin covers their frames), macro modules,
and the generated vtable/wrapper functions (they call real bodies, which check). A body that tail-calls itself is a jump back to the entry and the check runs on each entry;
a function that loops without a call is a `loop`/`recur`, which has no check.

The check is at the *entry*, before anything is evaluated, so the frame holds only its owned parameters. The ownership plan records what an unwinding at that point
releases, under a negative key like every other unwinding (`own.walk.state` `w-put-unwind`, the key `-1 - e` for a call; the entry's key is the one no expression has),
and the emitter runs those releases through the existing `lcx-unwind-exit`, so the caller sees what it sees for any failing call: a failure in the result slot, the
callee's owned parameters already released by the callee, the caller's own releases as the plan says. No new unwinding code exists.

## 6. The failure and what receives it

The failure is `fib.raise-c "stack overflow"` (a fresh tagged `str`, the same as any trap: `fib.raise-bytes`), so `(ex-message e)` is `stack overflow` and the kind read from
the message is `stack`. When no `catch-run` is active on the thread, `fib.raise-bytes` is `fib.trap-bytes`: the abort with `trap: stack overflow` that an ordinary program
gave before (a task's thread: the task's failure, ADR 0001). So the check never *worsens* anything: with no catcher it ends exactly as the signal path did, with the same
words and the same status.

- **A live session** is already a catching program, with a catch around every evaluation (docs/design/live-repl.md). The overflow comes back as the reply's error; the
  session keeps its state and answers the next input. `(down 100000000 0)` is the test (section 9).
- **A library** is compiled as a catching program (section 7). Each export wrapper enters a catch (`fib.catch-enter`), calls the function and leaves it
  (`fib.catch-leave`). On a failure the wrapper returns the zero of the result type (0, 0.0, `NULL`, length 0) and **leaves the failure for the thread**. The host asks
  with two new symbols of every library, `int32_t fib_status(void)` (0: the last call on this thread returned, 1: it trapped, 2: it threw) and `int64_t fib_error(uint8_t *out,
  int64_t cap)` (the message, copied as a `str` result is: the length needed, at most `cap` bytes copied). The status is per thread (the pthread key of the catch state),
  cleared at the start of each export call, so two host threads never see each other's. A call that returned normally leaves 0. This also turns *every* other trap of
  the library (an `nth` out of range, a division by zero) from `abort()` of the host into the same status: the N2 stage of shared-libraries.md 0 for the traps that can
  unwind (not for `array-take!` regions, `cell-update!` functions and `def` initialisers, which stay fatal as ADR 0009 says).

## 7. Where it applies, and the flag

| program | check | why |
|---|---|---|
| a program that never calls `catch-run`, not live, not a library | **none**; the signal path, `trap: stack overflow`, 134 | nothing could catch the failure; the check would be paid for a message the guard prints anyway; this keeps the compiler and every benchmark byte for byte |
| a program that calls `catch-run` (`try`, `fib.test`'s children, `fib.ex`) | **on** | the one place the old rule said "not even `try` catches it"; the cost is on top of the convention the program already pays |
| `fibc run --live` | **on** | every other trap is a reply; this one was the exception |
| a library (`--shared`, `--emit obj --export`) | **on**, and the library is a catching program | a host cannot be given an abort; it can be given a status |
| `FIB_STACK_CHECK=0` | off everywhere | measuring, and a bisect when the check is suspected |
| `FIB_STACK_CHECK=1` | on in a program that catches, as the default | |
| wasm32-wasi | never | no signals, no threads, the host's stack (stack.md) |

The default is the program's own property (it catches) rather than a flag the user must remember, because the failure the flag would guard against is the one the
owner called miserable. `FIB_CATCH=all` makes any program a catching one, and so, with the default, a checking one; section 9 measures the check on that.

## 8. What is still unsafe (the backstop)

- **Code that is not checked** runs on whatever the margin leaves. The runtime's lIR functions are not checked. A runtime function that recurses without bound on the
  *data* (`fib.drop` of a list a million nodes long, the free of a deeply nested value) can run through the margin and the guard: `trap: stack overflow` on the signal path,
  fatal, in a live session or a library too. The emitter's own functions that drop are checked; the runtime's walkers are not. This is the same hole as before and is now
  the only one for a pure-fibber recursion.
- **Foreign code**: a C library that recurses (a regex engine, `qsort` with a comparator that calls back) is the host's or the library's own and is not checked.
- **A foreign thread with no known stack** (section 3) is unchecked.
- **A function that keeps a frame bigger than the margin** (section 4).
- **A host thread that enters with almost no stack left** gets the failure at once, which is correct; the host's own frames below are the host's.
- The check cannot see another thread's stack, and a thread that moves to another stack (a coroutine library in the host) breaks the table's assumption that a stack address
  belongs to one thread; such a host must not call into a library from a stack it switched to.
- A fault that is not an overflow is as before (`fatal signal N`).

## 9. Cost (measured)

RAN on this machine (28 cores, load 1.5 to 5 from other jobs: noise of several percent either way; `num-f64` is 40 percent faster under the catching convention for a reason unrelated to
the check: LLVM compiles that loop differently once its callees return a failure). Each program built four ways with the F of this tree and timed, median of 5 (seconds;
script: a loop of `F build` and `time` over `scripts/bench/*.fib`): `F0` the main of a548530b (no check code exists); `A` this F, the program as written (no `try`, so no check and no
catching); `B` this F with `FIB_CATCH=all FIB_STACK_CHECK=0` (the catching convention, no check: what a live program paid before); `C` this F with `FIB_CATCH=all` (the convention and the check).

| program | F0 | A plain | B catching | C catching + check | B/A | C/B |
|---|---|---|---|---|---|---|
| recursion (fib 44, tail-recursive sum) | 2.21 | 2.13 | 3.33 | 4.20 | +56% | +26% |
| binary-trees | 1.50 | 1.41 | 1.51 | 1.50 | +7% | -1% |
| dispatch | 3.30 | 3.15 | 3.29 | 3.59 | +5% | +9% |
| num-f64 | 5.39 | 5.46 | 3.23 | 3.31 | -41% | +2% |
| num-nbody | 1.40 | 1.43 | 1.50 | 1.56 | +5% | +4% |
| vec-conj-pop | 1.71 | 1.72 | 1.77 | 1.80 | +3% | +1% |
| vec-index (100e6 `nth`) | 0.46 | 0.50 | 0.51 | 0.77 | +3% | +51% |
| vec-sort | 0.68 | 0.66 | 0.69 | 0.69 | +5% | 0% |
| map-assoc-get | 0.49 | 0.50 | 0.47 | 0.54 | -6% | +14% |
| set-conj | 0.39 | 0.39 | 0.41 | 0.40 | +4% | -2% |
| lazy-fused | 1.84 | 1.85 | 2.15 | 2.57 | +17% | +20% |
| strings | 0.87 | 0.87 | 0.88 | 0.87 | +1% | -1% |

- **A against F0: no change** (the differences are noise, both ways). A program that does not catch has no check, and its lIR differs from F0's by three unused `declare` lines
  (`pthread_self`, `pthread_getattr_np`, `pthread_attr_getstack`); checked on `num-f64` and `vec-sort`. This is the requirement "above 3 percent in programs that do not need it: make it
  apply only where required": it applies only to programs that pass failures on.
- **C against B, the cost of the check where it is needed**: from nothing to +51 percent, median about +3 percent. It is paid by a function that is on a cycle of calls and does little
  per call: `vec-index` spends its time in `vnode-get`, a four-level tree descent of a few instructions, checked on each call; fib 44 is 2.7 billion calls; `lazy-fused` is recursive
  stepping. Narrowing the check from "every body that calls something" to "bodies on a cycle of calls" (section 5) took `strings`, `vec-conj-pop`, `binary-trees` and `vec-sort` from +6 to
  +14 percent down to noise, and left `vec-index` at +43 to +51 percent (its one hot function is the recursive one: it cannot be skipped). Two things made the difference
  between tolerable and not: a `ccc` slow path doubled the frame of every checked function (32 bytes against 16) and halved the deepest recursion; `preservemost` restored both.
  A tight recursion pays about 2 ns a call. Not done, and the next lever: a check on only one member of a cycle of several, and none in a function whose recursion is
  structurally bounded (a descent of a fixed-depth tree).
- **What a live session pays** is B (before) against C (now): the same table. A live program already ran 5 to 56 percent slower than a plain one for the convention (B/A), and the
  check adds the C/B column on top. `FIB_STACK_CHECK=0` removes it (and with it the guarantee); not recommended as a default because the failure it guards against is the one
  the owner called miserable.
- **Depth**: `down` (16 bytes a frame, 256 MiB main stack): the deepest recursion that completes is 16,772,704 frames without the check and 16,758,300 with it (bisected to 20,000
  frames): 14,404 frames, 230 KB: the margin and nothing else. 100,000 and 1,000,000 frames answer. (With a `ccc` slow path: 8,375,000.)
- **Not measured**: aarch64 and Darwin (no Mac run in this package); a library benchmark apart from the test's own calls (a library is a catching program: its calls pay B, and its recursive
  functions C).

## 10. Tests

Before (the main of a548530b, `F0`) and after (this tree), RAN:

- Live: `(up 100000000)` over the attach socket. Before: `connection closed (the program is gone, exit -6)`, `trap: stack overflow` on standard error. After:
  `{"ok":false,"failed":"stack overflow","exit":134,"trapped":true}` and the next input `(+ 1 2)` answers `3`. `compiler/tests/live/stackcheck.sh F` (about 7 s): 14 `ok` lines: 1000 and 100,000
  frames answer; three overflows each followed by an answer; an allocating recursion (`grow`, a Vec and a str a frame) overflows the same way and `(grow 1000)` answers 7893; ten churns of
  100,000 allocations; five more allocating overflows each followed by an answer; the program alive; no `trap:` or `fatal signal` on standard error. Planted fault (`FIB_STACK_CHECK=0`):
  `FAIL overflow 0 ...: connection closed (the program is gone, exit -6)` and four more FAIL lines.
- Library: a host `depth(1000000000)` on its main thread. Before (`before.c` against a library built by F0): `depth(1000)=1000`, then exit 139 and no `host survived`. After,
  `compiler/tests/shared/stackcheck.sh F` (about 57 s): 28 `ok` lines: every call returns 0 with `fib_status() == 1` and `fib_error() == "stack overflow"`, the next call answers and clears
  the status; str parameter, str result and unit result; an index trap is a status too; a second host thread's status is its own; threads with a 256 KiB and a 64 MiB stack; 8 threads at once
  (40 rounds each); 60 threads started and ended in turn; the main thread still answers. Planted fault: the host exits 139. Depth: 16,758,300 frames with the check, 16,772,704 without.
- `cases/stdlib/8352` (was: `try` does not catch an overflow, the process ends 134; now: the catch answers 7), `7892` (a hundred thousand frames each holding a Vec and a str unwind, `audit: clean`,
  and the kind is `"stack"`), `cases/lir/inline/stack-addr.lir` and its reject case (`lairf cases cases/lir/inline`: 10 of 10).
- Unchanged and still passing: `compiler/tests/driver/host-threads.sh` (4 ok), `compiler/tests/live/mt.sh`, `shared.sh refusals kernels`, `sh-live-stubs-sample`, the golden checks, `runtime.sh`
  (31 checks), the executable ADRs (27 accepted), `make quick`.
- fib-solid (not modified): `ONLY=recovery native/test-live.sh` with this F: the request that overflowed returned `{"ok":false,"error":{"message":"stack overflow"}}` in 0.13 s and the helper
  did not die or recover (recovery used to take 10 s, 0.7 s with a standby). Its four "died / recovered" checks fail because they pin the old behaviour; they want rewriting to expect the error.
