# 0009. A catching program returns failures next to its values

Status: accepted
Date: 2026-10-06
Source: docs/design/exceptions.md §4.2 and §9 (stage 2); decisions-2026-10-04.md "Exceptions (§7)"; cases `stdlib/7870` to `7891`;
`scripts/mutant-catch.sh`.

## Context

Stage 1 (ADR 0001) lets a caller survive a trap only across a task boundary, and abandons what the task owned. A program that must survive a
bad input in the same thread (a parser, a server's handler, a transaction) needs `try`/`catch`/`finally` with real unwinding, and the audit
must stay exact: after a caught trap every object the unwound frames owned is freed once, and no slot a later read reaches holds a
moved-from pointer. lIR has no landing pads (design §3.1), and setjmp/longjmp leaks every skipped frame (§3.3).

## Decision

Mechanism (b) of the design. In a program that calls `catch-run` (what `fib.ex`'s `try` expands to), every function body, closure code
and vtable function returns its failure next to its value: `{ T ptr }`, `ptr` for unit, or, for a result of two leaves (a `dyn`, an unboxed
option, a SIMD vector), a failure slot whose address is a last parameter, so that tail calls still jump. After each call the caller tests
the failure and unwinds: it writes back the call's `&` arguments, releases the arguments it had evaluated and not handed over, releases
what the ownership plan says the frame holds at that call (`own.walk.state` `w-put-unwind`: the scopes' owning sites and owned parameters
the path has not handed over, as a tail call's jump would), and returns the failure. A program that never calls `catch-run` is compiled
exactly as before: no function pays for the convention, and every trap aborts as it did.

A trap unwinds only while a catch is active on its thread (a per-thread count, `fib.catching`); otherwise it is the trap it always was (the
abort, or the task's failure of ADR 0001). Fatal and never caught: out of memory, stack overflow (which says `trap: stack overflow` and ends with the status of a trap: docs/design/stack.md), a thread that cannot start, a trap
inside a `finally` clause run while unwinding (`trap: in a finally clause run while unwinding: MSG`), a trap inside the runtime's own
functions, and a failure that reaches code that cannot pass it on: a `def`'s initialiser, an async body, the function of a
`cell-update!` (the cell is moved from while it runs; poisoning is not built), and any function that calls `array-take!` (a slot is moved
from between the take and the store).

## Consequences

- `try` is a library macro over a closure: `recur` cannot leave a try body and `await` cannot be in one, by construction.
- The cost is paid only by programs that catch: a predictable branch after each call, and code size for the unwinding blocks (measured in
  the design, §9).
- A `cell-update!` or `array-take!` region turns a catchable trap into a fatal one. Poisoning (`cell-reset!`) and a checker for
  trap-free take regions would make them catchable; neither is built.
- A caught exception is an `(ExInfo (Map keyword Datum))`; a trap's data is `{:fib/kind KIND}` with KIND read from its message.
- The compiler itself does not use `try` until a seed that has it is released.

## Governance

The behaviour a program sees, as scenarios (these run through `fib.test`, in a forked child each):

```fibber spec
;; :use fib.ex
(defun pick-tenth (v: (Vec i64)) -> i64 (nth v 10))
(defun kind-caught (v: (Vec i64)) -> str (try (do (pick-tenth v) "none") (catch e (unwrap-or (ex-kind e) "?"))))
(defun message-caught (m: str) -> str (try (throw (ex m)) (catch e (ex-message e))))

(feature "Catching traps"
  (scenario "a trap below the try is caught as an exception of its kind"
    (given [v [1 2 3]])
    (upon [k (kind-caught v)])
    (then (expect = "index" k)))
  (scenario "a thrown exception is the one caught"
    (given [m "bad input"])
    (upon [c (message-caught m)])
    (then (expect = "bad input" c)))
  (scenario "finally runs when the body returned"
    (given [log (cell 0)])
    (upon [r (try 5 (finally (set! log 1)))])
    (then (expect = 5 r) (expect = 1 @log))))
```

The shape of the compiler and the runtime that the decision rests on:

```fibber fitness
(rule "a trap unwinds only while a catch is active on its thread"
  (must-contain repo "rt/core.lir" "(block check (br (call @fib.catching) unwind plain))")
  (plant-file "rt/core.lir" "(define internal (fib.raise-bytes ptr) ((ptr b) (i64 n)) (block entry (ret b)))"))

(rule "a trap in a finally clause run while unwinding is fatal"
  (must-contain repo "rt/core.lir" "(br (call @fib.in-finally) fin task)")
  (plant-file "rt/core.lir" ""))

(rule "a function with an array-take! does not pass failures on"
  (must-contain repo "compiler/emit/lower/mod.fib" "(takes-slots? p (body-expr g (. inst key)))")
  (plant-file "compiler/emit/lower/mod.fib" ""))

(rule "cell-update!'s function runs with failures fatal"
  (must-contain repo "compiler/emit/lower/cells.fib" "(fatal-while cx (fn () ((. (. cx hub) emit-call) cx (TgValue f) [old] content false)))")
  (plant-file "compiler/emit/lower/cells.fib" ""))

(rule "the unwinding releases what the plan says the frame holds"
  (must-contain repo "compiler/own/walk/call.fib" "(w-put-unwind w (. e id) moved)")
  (plant-file "compiler/own/walk/call.fib" ""))

(rule "only a program that calls catch-run is compiled to unwind"
  (must-contain repo "compiler/emit/program.fib" "(= (. (nth (. typed builtins) b) name) \"catch-run\")")
  (plant-file "compiler/emit/program.fib" ""))

(rule "the cases that pin it (stdlib 7870 to 7891) are in the tree"
  (at-least repo ["cases/stdlib/787?-*" "cases/stdlib/788?-*" "cases/stdlib/789?-*"] 22)
  (plant-remove "cases/stdlib/7872-trap-in-a-finally-while-unwinding-is-fatal.fib"))
```
