# 0001. Task traps are isolated at the thread entry

Status: accepted
Date: 2026-10-05
Source: docs/design/exceptions.md §0 and §1.2 (stage 1); cases `stdlib/911` to `926`; owner's list of 2026-10-05.

## Context

A trap used to end the process, on whichever thread it happened (types §2.11). A server, a test runner or a language server that runs
work on tasks cannot afford that: one bad request kills every other. Full exceptions (stage 2, `try`/`catch` with unwinding) are a large
change in the compiler, the ownership checker and the runtime; the part that is cheap and bounded is the task boundary, where a
failure can only lose what one task owned.

## Decision

A trap on the thread of a **spawned task** ends that task and not the process. The runtime decides at the trap: a thread that has a
task (`fib.task-key`, set once at the thread entry) records the message as the task's failure and completes the task; any other thread
aborts as before. `try-join` returns `(Ok v)` or `(Err (Trap message))`; `join` and `@t` of a failed task trap in the joiner with the
task's message. Out of memory, a thread that cannot start and a stack overflow are fatal everywhere and never isolated. What the dead
task's frames owned is abandoned, not freed (the audit says `abandoned`).

## Consequences

- A caller can survive a task's trap and report it; the process still dies on a trap in `main`'s thread (case `921`).
- Memory the task owned leaks until the process ends; the bound is one task's frames. Stage 2 (real unwinding) is not made harder.
- A trap is only its message today: there is no kind to match on.
- Isolation is a property of the thread entry, so a new way to start threads must install the task key or it is not isolated.

## Governance

The behaviour a program sees, as scenarios (these run through `fib.test`, in a forked child each):

```fibber spec
;; :use fib.async
(defun trap-text (r: (Result i64 Trap)) -> str (match r ((Err e) (. e message)) ((Ok _) "")))
(defun ok-value (r: (Result i64 Trap)) -> i64 (match r ((Ok v) v) ((Err _) -1)))

(feature "Task traps"
  (scenario "try-join of a task that trapped is the trap's message"
    (given [t (spawn (fn () (do (trap "boom") 0)))])
    (upon [r (try-join t)])
    (then (expect = "boom" (trap-text r))))
  (scenario "a trap the runtime raises is isolated the same way"
    (given [t (spawn (fn () (quot 10 (- (count [1 2 3]) 3))))])
    (upon [r (try-join t)])
    (then (expect = "integer / by zero" (trap-text r))))
  (scenario "a task that did not trap is unaffected by one that did"
    (given [bad (spawn (fn () (do (trap "boom") 0))) good (spawn (fn () 42))])
    (upon [rb (try-join bad) rg (try-join good)])
    (then (expect = 42 (ok-value rg)) (expect = "boom" (trap-text rb))))
  (scenario "try-join says the same when asked twice"
    (given [t (spawn (fn () (do (trap "twice") 0)))])
    (upon [a (try-join t) b (try-join t)])
    (then (expect = "twice" (trap-text a)) (expect = "twice" (trap-text b)))))
```

The shape of the runtime: only a thread that has a task isolates, and only the thread entry gives it one.

```fibber fitness
(rule "fib.fail-task is called by the trap path in rt/core.lir and by nothing else"
  (grep-live repo ["rt/*.lir" "!rt/core.lir"] "@fib.fail-task")
  (plant "rt/str.lir" "\n    (call @fib.fail-task t b n)\n"))

(rule "the trap path asks whether the thread has a task before isolating"
  (must-contain repo "rt/core.lir" "(call @pthread_getspecific (load i32 @fib.task-key))")
  (plant-file "rt/core.lir" "(define internal (fib.trap-bytes void) () (call @abort))"))

(rule "only the thread entry stores the task key"
  (grep-live repo ["rt/*.lir" "!rt/thread.lir"] "pthread_setspecific (load i32 @fib.task-key)")
  (plant "rt/task.lir" "\n    (call @pthread_setspecific (load i32 @fib.task-key) task)\n"))

(rule "the thread entry does store it"
  (must-contain repo "rt/thread.lir" "pthread_setspecific (load i32 @fib.task-key)")
  (plant-file "rt/thread.lir" ""))

(rule "out of memory is fatal, never a trap a task survives"
  (grep-live repo ["rt/*.lir"] "fib.trap-c (string \"out of memory\")")
  (plant "rt/core.lir" "\n    (call @fib.trap-c (string \"out of memory\"))\n"))

(rule "the cases that pin it (stdlib 911 to 926) are in the tree"
  (at-least repo ["cases/stdlib/911-*" "cases/stdlib/912-*" "cases/stdlib/913-*" "cases/stdlib/914-*" "cases/stdlib/915-*" "cases/stdlib/916-*"
                  "cases/stdlib/917-*" "cases/stdlib/918-*" "cases/stdlib/919-*" "cases/stdlib/920-*" "cases/stdlib/921-*" "cases/stdlib/922-*"
                  "cases/stdlib/923-*" "cases/stdlib/924-*" "cases/stdlib/925-*" "cases/stdlib/926-*"] 16)
  (plant-remove "cases/stdlib/921-trap-in-main-with-a-task-running-is-the-old-abort.fib"))

(rule "a trap in main with a task running still aborts (case 921 expects a trap)"
  (must-contain repo "cases/stdlib/921-trap-in-main-with-a-task-running-is-the-old-abort.fib" ";; expect: trap")
  (plant-file "cases/stdlib/921-trap-in-main-with-a-task-running-is-the-old-abort.fib" ";; expect: accept\n"))

(rule "the spec still names the section the decision amends (types 2.11: weak, spawn, join, trap)"
  (must-contain repo "spec/types.md" "### 2.11 `weak`, `spawn`, `join`, `trap`")
  (plant-file "spec/types.md" "# types\n"))
```

### What this does not check

That the cases pass (the gate runs them); that every kind of trap, on every platform, reaches the isolating path; that a trap
in a task does not corrupt shared state it touched (the design says a task owns what it traps with; nothing here proves it); the
leak bound (case `918`, `audit: abandoned`). The fitness rules read the runtime's source text: they notice a moved call, not a
changed meaning.
