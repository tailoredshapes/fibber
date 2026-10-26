# 0027. A stack overflow in a catching program is a caught trap

Status: accepted
Date: 2026-10-22
Source: the owner, 2026-10-22 (the status quo is "miserable for meat and agent alike"; the owner authorised revisiting the rule they had ruled, docs/design/stack.md 3 and ADR 0009);
docs/design/stack-check.md; docs/design/live-incremental.md 11; docs/design/host-threads.md 5. Reverses, for emitted code, the "stack overflow is fatal and never caught" of ADR 0009.

## Context

ADR 0009 put the stack overflow on the fatal list: it is a fault on a guard page, the handler cannot return into the program, the frames are abandoned and the fault may be inside
`malloc`, a lock, a reference count or a free-list push (docs/design/stack.md 3; live-incremental.md 11). That is still true of a fault. What it cost: a live session died on a
deep recursion although every other trap came back as an error; a `--shared` library ran off the host's stack and killed the host (status 139); agents that write recursive code met
it constantly and got no useful failure.

## Decision

A function of a catching program tests its stack before it uses it. On entry it compares the stack pointer (`(STACK-ADDR)`, spec/lir.md 6.6b) with the word of a global table for
its 2 MiB block; outside the range the word vouches for, `fib.stack-slow` finds the thread's limit (kept per thread, built from `pthread_getattr_np` for a thread the runtime did
not start) and either registers the block or answers overflow. The overflow is the trap `stack overflow`, taken through the existing catching return with the releases the
ownership plan records for the entry (`unwind-entry-key`): frames unwind normally, the audit stays exact. The limit is the lowest usable address plus a margin of 256 KiB. The
check is emitted in a program that passes failures on (it calls `catch-run`, it is live, it is a library), in a function that makes a call that is not a builtin's and can pass a
failure on; `FIB_STACK_CHECK=0` removes it. A program that never catches has none and keeps the signal path (`trap: stack overflow`, 134). A library catches in each export and
leaves the failure for `fib_status` and `fib_error`. The guard page and its handler stay as the backstop for what is not checked (the runtime's own walkers, foreign code).

Nothing recovers from the signal handler; the reasons of ADR 0009 stand for it.

## Consequences

- `try` catches a stack overflow; its kind is `"stack"` (`ex-kind`), its message `stack overflow`. Case 8352 changed its verdict with this ADR.
- A live session answers a deep recursion with an error reply and goes on. A library returns a status and the host goes on.
- A function with `array-take!` (like `conj`) is not checked, its callers are; the margin covers its frame.
- The cost is paid by the programs that need it (docs/design/stack-check.md 9), not by a program that does not catch.
- Still unsafe: docs/design/stack-check.md 8.

## Governance

```fibber fitness
(rule "a function of a catching program tests its stack on entry"
  (must-contain repo "compiler/emit/lower/mod.fib" "(when (stack-check? cx recurses) (emit-stack-check cx (unwind-entry-key key)))")
  (plant-file "compiler/emit/lower/mod.fib" "(ns emit.lower.mod)"))

(rule "the plan records what an entry that fails releases"
  (must-contain repo "compiler/own/walk/lit.fib" "(w-put-entry-unwind w (. (. spec body) id))")
  (plant-file "compiler/own/walk/lit.fib" ""))

(rule "the slow path answers overflow below the thread's limit"
  (must-contain repo "rt/thread.lir" "(DEFINE INTERNAL NOINLINE PRESERVE_MOSTCC (fib.stack-slow I1) ((I64 sp))")
  (plant-file "rt/thread.lir" ""))

(rule "a library's exports catch and leave the failure for the host"
  (must-contain repo "compiler/emit/export.fib" "(n/call \"fib.catch-leave\" [(n/local \"cs\") (n/local \"ex\")])")
  (plant-file "compiler/emit/export.fib" ""))

(rule "the cases and tests that pin it are in the tree"
  (missing repo ["cases/stdlib/8352-a-stack-overflow-under-try-is-caught.fib"
                 "cases/stdlib/7892-a-stack-overflow-unwinds-every-frame-and-releases-what-they-held.fib"
                 "compiler/tests/live/stackcheck.sh"
                 "compiler/tests/shared/stackcheck.sh"])
  (plant-remove "compiler/tests/shared/stackcheck.sh")
  (plant-remove "compiler/tests/live/stackcheck.sh"))
```
