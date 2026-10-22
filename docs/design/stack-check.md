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

A body is checked when it can be part of a cycle of the call graph and is lowered to pass failures on (`unwind` is true): in the emitter, `emit-body` (`defun`, method,
the all-owned twin) and `emit-fn` (a closure) when the body **contains a call whose callee is not a builtin** (a call of a function, method, closure value or
constructor-less protocol dispatch). Bounded leaves are skipped: a body whose calls are all builtins (arithmetic, array access, `nth`, `str` ops, the runtime's own
helpers) or that has none, cannot recurse by itself, and any recursion goes through a function that makes a call, which is checked. Not checked, by construction:
`def` initialisers and `async` bodies (they cannot pass a failure on: ADR 0009), functions with `array-take!` (a failure there is already fatal; an overflow there says
`trap: stack overflow` and aborts through `fib.unwound-fatal`, the same words as before), macro modules, and the generated vtable/wrapper functions (they call real bodies,
which check). A body that tail-calls itself is a jump back to the entry, and the check runs on each entry; it is one compare for a loop iteration, and the
function that loops without a call is a `loop`/`recur`, which has no check.

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

(filled in by the measurements: `scripts/bench/quick.sh` with `FIB_CATCH=all` and `FIB_STACK_CHECK=0` against the default, and a recursion-heavy subset)

## 10. Tests

(filled in with the output: compiler/tests/live/stackcheck.sh, compiler/tests/shared/stackcheck.sh, cases)
