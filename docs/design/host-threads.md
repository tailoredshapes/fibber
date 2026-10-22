# Host threads: a fibber runtime entered by threads it did not start

Status: found and fixed on branch `race-hunt` (2026-10-22). Every claim says how it was established: `RAN` = a command run in this session, `READ` = a file read.

Two crashes looked like one defect: a fibber shared library called by four C threads at once (SIGSEGV, 20 of 20 runs in the study that found it), and a
`fibc run --live` session whose first input after an attach traps with `nth: index out of range` or dies with `fatal signal 11`. They have **one cause**.

## 1. The cause

The runtime has two modes for small objects (`rt/core.lir`, "The small-object free lists", READ). While the program has one thread (`fib.mt` is 0) an object
of at most 512 bytes comes from, and goes back to, a per-class free list (`fib.fl`) with **no lock and no atomic**. `fib.mt` is set to 1 by the runtime
itself, in `fib.spawn`, `fib.pool-start` and the task pool (`rt/thread.lir`, `sched.lir`, `task.lir`), before the first thread it starts; from then on every
block comes from `malloc` and goes to `free`. The assumption is: *every thread of the program is one the runtime started, so it knows when the second one appears.*

Two places break the assumption.

1. **A library entered from host threads** (`fibc build --emit obj --export`). A host thread is never started by the runtime, so `fib.mt` stays 0 for the
   life of the library and all host threads pop and push the same `fib.fl` lists. Two threads pop the same block, or one pops while another pushes: a block
   is handed out twice, a list gets a live object's first word (its reference count, 1) as its next link, and the next pop dereferences it.
   Also, `fib.export-ready` started the runtime from a plain, non-atomic flag (`fib.exported`): two threads entering for the first time both ran `fib.init`
   and the def initialisers.
2. **A live session.** `fibc run --live` runs the program's `main` on a thread of its own, and the attach thread (a thread of the *compiler's* runtime)
   calls program code for each `eval`, beside it (`compiler/serve/live.fib`, `docs/design/live-repl.md` 3). The program's copy of the runtime is in the same
   process, with its own `fib.mt`, still 0 unless the program spawns a thread. So the attach thread and the program's thread share the unlocked free lists.
   A program that spawns early (or has no `main` loop that allocates) hides it.

## 2. The evidence

Embedding (`fibber-wt-browser/docs/experiments/native/{kernels.fib,host.c}`, `compiler/tests/driver/hostthreads/`):

- `host LIB.so 4`: exit 139 in 20 of 20 runs (RAN); the gdb backtrace is `vec-collect.i64` called from `sum-squares` called from the host's `worker`, at
  `mov 0x10(%rcx),%rbx`, a load through a block that is not what the allocator thought it was (RAN).
- With gdb setting `fib.mt` to 1 at the first `pthread_create` of the host (`set var *(int*)&'fib.mt' = 1`), nothing else changed: **20 of 20 runs exit
  normally; without it 20 of 20 crash** (RAN). `nm libk.so` shows `fib.mt`, `fib.fl` and `fib.exported` as local symbols of the library.
- The hypotheses: (a) the first-call race is real but is not what crashes (the study's main thread had called first and it still crashed; the new test has a
  "main thread called first" row); (b) the allocator's large-block cache is under a lock (`fib.lc-lock`, READ) and is not involved; the small-object lists are;
  (c) `vb-*`, interning and statics keep no thread-shared scratch (READ: `vec-collect` uses a local `(cell nil)`; the crash is in the allocation under it);
  (d) is the live case.

Live (`fib-solid-wt-live/scripts/live-bench.py`'s `Session`, the bolt model, RAN):

- Unmodified, 6 of 6 sessions die at the first or second `(summary)` after the attach (`fatal signal 11`).
- gdb following the fork into the session: thread 2, the attach thread, faults in JITted code at `mov (%rdi),%rdx` with `rdi` = `0x7fff94666d7000`, in the
  pop of `fib.alloc` (`store (load ptr head) slot`): the head of a free list is garbage. The "fatal signal 11 at 0x1" of the notes is the same pop one step
  later (a link of 1 is a reference count read as a link).
- The same sessions with `main` first doing `(join (spawn (fn () 1)))`, which only sets `fib.mt`: **6 of 6 good**, 4 requests each.
- With `main` blocked in one long sleep instead of its allocating loop (so the program's thread does not allocate during the input): 4 of 4 good. So the
  crash needs two threads using the lists, not the bolt's code.
- fib-solid's `thread`/`screw-hex` uses no threads (grep of `src`, READ); they only allocate a lot, which is why only they hit the window.

## 3. The design for host threads

A library has no main thread of its own: its callers are threads the runtime did not start and cannot count. So **a library is in multi-threaded mode from
its first call**, and a live session's program runtime is **from the start of the session**.

- `fib.lib-enter` / `fib.lib-ready` (`rt/core.lir`): the first call of any export takes a spinlock (`fib.lib-lock`), sets `fib.mt`, runs `fib.init`,
  `fib.set-args` and the def initialisers, publishes `fib.lib-state` = 2 with a release store and unlocks. Other first callers wait on the lock and then
  find state 2. Later calls cost one acquire load (a plain `mov` on x86 and arm64). `emit.compile/export-ready` generates the call.
- `fib_live_ctl` op 10 (`rt/live.lir`) sets `fib.mt`; `serve.live/serve-program` calls it before `main` is started.
- What this costs: a library never uses the free lists, so each small object is a `malloc`/`free`. `sum-squares(100)` in one host thread: 1020-1200 ns
  before, 1110-1140 ns after (RAN, noisy, another job was running): about 5 percent.

Not a lock around everything: the rest of the runtime was already written for threads (atomic reference counts on shared objects, the large-block cache
under its lock, per-thread state through `pthread_getspecific`); only the single-thread fast path was unsafe, and it is gone for these two cases.

## 4. Tests

- `compiler/tests/driver/host-threads.sh STAGE2` (tool `sh-driver-host-threads`): builds `hostthreads/kernels.fib` with `--emit obj --export`, `cc -shared`,
  runs `hostthreads/host.c` with 4 and 16 threads, cold and with the main thread calling first, 12 runs each, all threads released together by a barrier and
  every answer checked (`sum-squares`, `add`, a `str`). Before the fix: every row failed, exit codes 139 (and 134 `trap: out of memory` in some runs), 12 of
  12 runs in each of four rows (RAN). After: four `ok` (RAN, stage 2 built from this tree).
- `compiler/tests/live/mt.sh STAGE2` (tool `sh-live-mt`): `mt-app.fib` allocates in `main` all the time; ten `(churn 300000)` inputs over the attach socket
  must answer the same number, the program must be alive, stderr free of `fatal signal`. Before: `connection closed (the program is gone, exit -11)`,
  `fatal signal 11` (RAN). After: 12 `ok` lines, three runs in a row (RAN).

## 5. What is still not safe

- (Changed 2026-10-22, docs/design/stack-check.md: a library is a catching program, its exports return a status for a trap and for a stack overflow, and a host thread's stack is found by the check. What stays unsafe is stack-check.md 8.) Before that: a trap in fibber code called `abort()` (exit 134) and a deep recursion on a host thread segfaulted on the host's stack.
- `mallopt(M_MMAP_THRESHOLD)` is still called on the host's process at first call.
- A fibber value must not be handed from one host thread to another except through the library's own shared types; the C boundary passes scalars and
  raw pointers, and the host owns what a raw pointer names.
- A callback from fibber code into the host that calls an export again during the *start* (inside a def initialiser) deadlocks on `fib.lib-lock`.
- `dlclose` while a host thread is inside the library, or `fork` in a host, are not handled; thread-exit cleanup of a host thread's exception record relies
  on the pthread key destructor (`free`) and was not tested for 16 short-lived threads beyond the runs above.
- Not examined: other places where compiler-side threads call into a JITted program's runtime (the macro runner's mailbox worker, `fibc repl`); the
  same rule (a second thread running program code needs `fib.mt`) applies to each.
- The "about 1 in 10 after a swap" live deaths of the notes are very likely the same cause (the first batch after the fix: 0 crashed of 15 `live-bench.py`
  sessions), but that rate is too low to prove a cure from one batch.
