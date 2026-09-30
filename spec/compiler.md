# fibc: the compiler

**Proposed** (2026-09-28, M4). What this page fixes is everything the
compiler needs that types.md §8 does not already fix: the command
line, how a program is assembled into lIR modules, the runtime
module's ABI and its trace mode, the canonical free trace and how it
is compared with the reference interpreter's (method.md rule 6), the
case harness, and how macros run. The mapping itself — every lIR shape
the compiler emits — is types.md §8, which is **Decided**; nothing here
changes it. Where this page and §8 disagree, §8 wins and this page is
wrong.

## 1. What `fibc` is

`crates/fibc` is the Rust compiler of ROADMAP M4. It **reuses the
reference interpreter's front end as a library**: `fibref`'s reader,
expander, type checker and ownership checker produce the
`TypedProgram` and the `OwnedProgram` (the plan of types §6), and
`fibc` lowers that plan to lIR text, which `lair` checks (method.md
rule 7) and compiles. The plan is the specification of every count
operation: `fibc` performs exactly the operations `OwnedProgram` lists,
where it lists them, plus those that are part of each primitive's
definition (types §8.6–§8.8), and decides nothing about ownership
itself, exactly as the evaluator does (`fibref` `eval/`). A program the
front end rejects is rejected by `fibc` with the same message.

```
fibc run   FILE.fib            ; compile through the JIT and run main; exit with its status
fibc build FILE.fib -o OUT     ; an executable (lair's AOT)
fibc emit  FILE.fib            ; print the lIR module
fibc explain FILE.fib          ; the plan, as `fibref explain` prints it (types §9)
fibc cases [DIR]               ; the rule-6 harness (§5); default cases/ownership
```

`fibc run` and the executable print `main`'s result as one decimal line
on standard output and exit 0; a trap prints `trap: MESSAGE` on
standard error and aborts (SIGABRT, as types §8.12 says), so the exit
status is 134 under a shell. With `FIB_TRACE=1` in the environment the
runtime also prints the trace of §4 on standard error.

## 2. Modules

One compilation produces one lIR module: the runtime (§3), the type
table, the static objects, and every specialised body. Every symbol is
`internal` except `main`, so that two such modules can live in one
`Jit` (the macro-time module of §6 and the program) without clashing.
`main` is lIR's `(main i32)`: it initialises the runtime (reads
`FIB_TRACE`), calls the program's `main` specialisation, prints its
result, joins the threads still running (types §8.8) and returns 0.

Names. A fibber type prints as types §1 writes it; its **mangled form**
replaces `(` by `$`, `)` by `_` and a space by `.`: `(Vec (Box i64))`
is `$Vec.$Box.i64__`. A function specialisation is `f.NAME` followed by
the mangled key of §7 (`f.count.$Vec.i64_`); an all-owned body adds
`.owned`; a method implementation is `m.PROTO.METHOD.` + the head's
mangling; a closure body `l.` + the body's name + the literal's
expression id; an object struct `%struct.o.` + the mangled type; a
string literal `str.N`; the type table `fib.types`; a vtable
`vt.PROTO.` + head; a runtime function `fib.NAME` as §8 names it.

## 3. The runtime module `fib.rt`

`crates/fibc/rt/*.lir` is lIR source, included in the module verbatim
(with `internal` linkage), in front of the emitted code. It declares
the C functions it uses (`malloc`, `free`, `memcpy`, `write`, `abort`,
`getenv`, `pthread_create`, `pthread_join`, `sched_yield`, `printf`,
`snprintf`, `strlen`) and defines, with the signatures of types §8.2:

| Function | Type | Does |
|---|---|---|
| `fib.alloc` | `(i64 size, i32 tid) -> ptr` | `malloc`; header count 1, tid, flags 0; a trace line in trace mode |
| `fib.retain`, `fib.release`, `fib.drop`, `fib.unique?`, `fib.share`, `fib.immortalise` | as §8.2 | |
| `fib.trap` | `(ptr str) -> void` | writes `trap: ` and the `str` object's bytes and a newline to fd 2, then `abort` |
| `fib.trap-c` | `(ptr cstring) -> void` | the same for a NUL-terminated C string (the arithmetic messages of §8.12) |
| `fib.stack-init` | `(ptr p, i32 tid) -> void` | stores the header of a `STACK` object (count 0, tid, flags `STACK`) and, in trace mode, its trace ordinal (§4) |
| `fib.stack-end` | `(ptr p) -> void` | the trace line of a stack object's scope end; the inline drop follows in the emitted code |
| `fib.str-*`, `fib.array-*` | | the builtins of syntax §4.3 on `str` and `Array` objects, allocating through `fib.alloc` exactly where the interpreter's natives allocate |
| `fib.weak`, `fib.upgrade`, `fib.weak-clear` | §8.7 | |
| `fib.lock`, `fib.unlock` | `(ptr) -> void` | the spinlock of an atom or a weak box |
| `fib.spawn`, `fib.join-all` | §8.8 | a spawned task's thread; `main`'s return joins every thread |
| `fib.enqueue`, `fib.dequeue`, `fib.pool-start`, `fib.worker`, `fib.run-one`, `fib.drive`, `fib.await-or-park`, `fib.task-complete` | §8.8, §8 item 3 | the run queue, the pool, a joiner driving the queue, parking at an `await`, the wake at a completion |

The type table `fib.types` is emitted by the compiler as a `constant`
(§8.2) and reached from the runtime by name, since both are in one
module; type ids are assigned in order of first use, one per object
layout the program allocates (`str`, each `(Array T)` by element
layout, each struct and enum instance, each cell and atom content
layout, weak boxes, tasks, each closure literal). Beside it the
compiler emits `fib.classes`, one byte per type id giving the object's
trace class (§4), read only in trace mode.

## 4. The trace

**Why.** method.md rule 6 says results and memory audits must match.
The interpreter's audit is its trace of heap events (`fibref`
`heap/event.rs`); the compiled program cannot replay every retain and
release one to one, since the compiler elides pairs the interpreter
performs (types §6.12), but §6.12 promises that **the same objects are
freed at the same points**. So the comparable trace is the sequence of
allocations and frees, and the comparison is exact on that sequence.

**The canonical trace.** A sequence of lines, one event each:

```
A n k     ; a counted heap object was allocated: its ordinal n, its class k
F n       ; heap object n was freed (its count reached zero and its drop ran)
S n k     ; a STACK object was allocated: ordinal n, class k
D n       ; the scope of STACK object n ended (its inline drop ran)
T         ; a thread was spawned (a multi-threaded run, below)
```

`n` counts allocations in the order they happen, heap and stack
together, from 1; immortal objects (static data: literals, `def`
values, named-function closures, vtables) are not allocated and have
no ordinal. `k` is the object's class: `o` an immutable object (struct,
variant, string, array, closure, weak box, task), `c` a cell, `a` an
atom. Nothing else is traced: retains, releases, reads, writes, weak
upgrades and share-marking are not comparable across the two
implementations, by §6.12.

**The interpreter's side** is derived from `Heap::trace()`: `Alloc`
gives `A`, `Free` gives `F`, `AllocStack` gives `S`, `Drop` gives `D`,
with `ObjId`s renumbered in order of first allocation (immortal
allocations are skipped and never renumbered); a thread spawn gives
`T`.

**The compiled side** is written by the runtime in trace mode
(`FIB_TRACE=1`): `fib.alloc` takes the next ordinal from a global
counter (an atomic increment) and, to remember it, allocates 16 bytes
in front of the object (the pointer returned is 16 bytes past the
`malloc` result, and `fib.drop` frees from 16 bytes before; the
object's layout is unchanged); `fib.stack-init` takes an ordinal and
stores it in the stack object's `count` word, which in trace mode is
the one place a `STACK` object's count is not 0 — sound because every
test of §8.2 reads the flags first and refuses a `STACK` object before
reading its count, and `weak` never targets a stack object (§6.11).
Outside trace mode nothing of this runs and the count word is 0 as §8.2
says.

**Comparison.** Single-threaded (no `T` on either side): the two
sequences must be identical, line for line. Multi-threaded (a `T` on
either side): the interpreter's schedule (types §8.8) and real threads
interleave differently, and ordinals are assigned in allocation order,
so the comparison is by **tally**: the objects live at exit
(allocated and not freed, stack-allocated and not ended) must agree
in number per class, and the compiled program must have allocated at
least as many objects per class as the interpreter. It may have
allocated more: a `swap!` attempt that loses its compare discards the
value `f` built (types §8.6), and how many attempts lose under real
threads is unspecified, while the interpreter's schedule loses none
that the program's own `f` does not force. Every object the compiled
program allocated beyond the interpreter's must therefore have been
freed, which the live tally checks. A program whose
result or frees depend on the interleaving has no single correct
trace; none of the cases does (§8.8, "What depends on this choice").

**A trap** aborts the program: both traces stop at the abort; a
single-threaded compiled trace is whatever was written before it,
compared line for line with the interpreter's. A threaded run's trace
at an abort is not compared at all: the other threads are wherever
the OS's schedule left them, which no tally can predict (§8 item 12).
The harness compares the trap message (the header's text must occur
in both) and requires a non-zero exit.

## 5. The harness: `fibc cases`

For every `.fib` file of the directory, in name order:

1. the header is parsed as `fibref`'s harness parses it (method.md
   rule 3);
2. the case is run in the reference interpreter, in process, keeping
   its outcome (result, audit, or rejection or trap message) and its
   canonical trace;
3. the case is compiled and run in a **child process** (`fibc run
   --trace FILE`), because a trap aborts the process and a runaway
   thread must not take the harness with it; standard output gives the
   result, standard error the trace and any trap message, the exit
   status says which;
4. the verdict is `fibref`'s `judge` on the compiled outcome, whose
   audit is: clean iff the traces agree under §4 and the interpreter's
   audit was clean; `leak-cycle` iff the traces agree and the
   interpreter reported a leak cycle; an error naming the first
   differing line otherwise. A rejection must carry the same message on
   both sides (they share the front end; the harness checks that it
   still does), and a trap the same message.

`cargo test -p fibc` runs the same over `cases/ownership` and fails on
any case that is not a pass or an explicitly pending one; a pending
case is one the compiler reports `unsupported: REASON` for, and it is
listed, never counted as a pass (method.md).

## 6. Macros at compile time (syntax §3.16, ROADMAP)

The expander calls a *runner* for each `defmacro` application. `fibc`'s
runner (`crates/fibc/src/macros`) compiles the **macro-time module** —
the `defmacro` with the prelude, the same forms the interpreter's
runner checks (eval/macros.rs), checked and planned by the same front
end — through `lair`'s `Jit`, one module per macro, on the macro's
first application. The module exports, beside the entry that runs the
macro's body, constructors and readers of `Form` objects (`abi.rs`);
the runner converts the argument forms into objects, calls the entry
through the `ccc` trampoline the `Jit` provides, and reads the result
graph back into the expander's `Form`. `gensym` and the reflection
builtins need the expander's context, which lives in Rust: the module
holds two hook pointers and a context pointer set through
`fibm.set-hooks` before each call, and the lowered builtins call the
hooks with the context (`bridge.rs`); a reflection error ends the
expansion with the expander's message. Objects made at expansion time
are never freed (the process is the compiler's). The expansion must
equal the interpreter's: `tests/macros.rs` compares the two expansions
on every case that contains a `defmacro` and requires equality, form
for form, errors included. A build without the `llvm` feature runs
macros with `fibref`'s evaluator instead.

## 7. Monomorphisation

As types §4.3: a specialisation's key has one component per quantified
variable of the scheme, the full type when the variable carries a
protocol bound, else its layout class (`i1 i8 i16 i32 i64 float double
ptr opt dyn`). Inside a class-keyed specialisation the compiler
substitutes a **representative** type of the class for the variable
(`str` for `ptr`, `(Option str)` for `opt`, a `dyn` for `dyn`, the
scalar itself), so every expression has a concrete layout; nothing in
such a body can dispatch on the variable, since it has no bound. Method
calls whose resolution is a bound of the scheme are resolved per
specialisation to the instance of the key's full type. Bodies are
emitted on demand from `main` (and from every `(dyn P e)` vtable
reached), each key once.

## 8. Open questions for the owner

Recorded as they arise; none changes §8.

1. Trace mode stores a stack object's ordinal in its `count` word
   (§4). The alternative, a per-thread side stack in the runtime, needs
   thread-local storage that lIR does not have. Is the deviation
   acceptable as a trace-mode-only rule?
2. The multi-threaded comparison is by multiset (§4). A finer rule
   (per-thread sequences) would need the compiled trace to name the
   thread and the interpreter's to do the same, and its threads are
   not the OS's; proposed as out of scope for M4.
3. **`async` bodies are state machines** (**Decided**, owner,
   2026-09-30: as types §8.8 says, and as Clojure compiles a `go`
   block). The body is lowered like any function, each `await` ending
   its block, and `crates/fibc/src/resume.rs` then makes the function
   resumable: the entry block keeps the prologue (the captures) and
   ends in a `switch` on the task's resume point; every value live
   into a continuation lives in a slot of the task's frame (stored
   after its definition, loaded before each use, so no SSA value
   crosses a park and no `phi` is rebuilt); every `alloca` is a byte
   slot of the frame; every `ret` is the task's completion. The frame
   follows the captures (§8.8's `..locals`): the resume point, then
   the slots, so a task is allocated at the type table's size. The
   executor (`rt/task.lir`): a run queue under a spinlock and a pool of
   one worker per processor started at the first enqueue (which writes
   the `T` of §4: from then on the trace is compared by tally); a
   `join` drives the queue itself until its task is done, resuming the
   task when it can claim it, running other queued tasks meanwhile
   (§8.8); `await` finds the awaited task done, or queues it if it is
   pending (syntax §3.12: the executor runs a task when it is
   awaited), registers the awaiting task in its waiter list, marks it
   parked and returns from the resume; completion wakes every waiter
   onto the queue. A task's state word: 0 pending, 1 running or
   queued, 2 done, 3 parked; only a claimed task in state 0 or 1 is
   resumed, and a spawned task's driver is held by its thread for
   good. The interpreter keeps driving a task to completion on the
   joiner's stack, which §8.8 allows; results and frees agree (cases
   11, 24, 32, 38, 43, 44, 148, 174 to 176).
4. **`def` initialisers run through the JIT** (done, 2026-09-30, as
   §8.10 says). Each initialiser is lowered as the body `d.NAME`; the
   module so far (runtime, tables, the constants of the earlier
   `def`s, every body) is JIT-compiled, the initialiser called, and
   its value read out of the JIT's memory by the type table's layouts
   into `IMMORTAL` constants (`defs/jit.rs`), a `def` at a time in
   source order so that each sees the ones before it; the statics a
   value may point to (earlier constants, the closures of named
   functions) are found by address and shared. The interpreter backend
   (`defs/interp.rs`) remains the executable spec of the same values
   and the fallback of a build without `llvm`; `tests/defs.rs` requires
   the two to emit the same constants, text for text. Still
   unsupported in a `def`: a closure other than a named function, a
   `dyn`, a boxed `Option`, a cell, an atom.
5. **A boxed `Option`'s `nil` may be null.** §8.3 gives every
   non-null `Option` a heap enum with tag 0 for `nil`. The
   interpreter allocates that object only where the plan decides the
   site (`Alloc::Heap`, a concrete scalar payload); inside a generic
   body it makes `nil` an unboxed value (eval/option.rs), so a
   generic `(nil)` allocates nothing there and the two traces would
   differ. `fibc` follows the interpreter: such a `nil` is the null
   pointer, and a pattern on a boxed `Option` tests for null before
   reading the tag. One rule should be chosen for both; the
   interpreter's cannot know the payload type of a `nil` in a generic
   body without monomorphisation.
6. **A weak box carries a fourth word**, the link of the global table
   (a list under one spinlock); §8.7's `fib.weakbox` has three. The
   table's structure is the runtime's; is the extra word acceptable?
7. **A `dyn` field or content is one `{ ptr ptr }` field** in the
   emitted `defstruct`s rather than two `ptr` fields (§8.6): the same
   layout, one lIR value.
8. **Joining waits by yielding** (`sched_yield` until the state is
   done), not on a futex or condition variable (§8.8); an
   implementation detail with the same behaviour.
9. **A `spawn`'s result lives in an atom object of the result type**,
   as the interpreter allocates one (eval/task.rs `result_atom`), so
   that the threaded trace tallies agree; §8.8's task holds `result`
   inline.
10. **`@w` on a `(Weak (dyn P))` allocates** (**Decided**, owner,
    2026-09-28). §8.7 makes the result a heap enum, `(some { t, vt })`
    or the tag `nil`; the interpreter returned an unboxed `some` and
    allocated nothing, so under §4 no compiled program could agree
    with its trace (case 87). The interpreter now follows §8.7: an
    upgrade whose static type is `(Option (dyn P))` goes through
    `option_value` with heap placement (eval/cells.rs), and `fibc`
    emits the same two allocations (`lower/cells.rs`). A weak
    reference to any other object type still allocates nothing.
11. **The text of `show` on a float and on a `str` is undecided.** The
    interpreter prints a float with Rust's `{:?}` (the shortest text
    that reads back to the same value, `1.0`, `1e21`, `NaN`, `inf`)
    and a `str` with Rust's `{:?}` (quoted, with Rust's escapes and
    `\u{..}` for what Rust deems unprintable). Neither is a rule of
    types §2.12, and neither can be reproduced in the runtime without
    a decision on the exact text (a float printer of the shortest
    round-trip kind; the set of characters a `str` escapes). `fibc`
    reports both as unsupported; every other native `Show` and `Hash`
    is lowered (`lower/show.rs`, `rt/str.lir`; case 169). Proposed:
    §2.12 fixes `show` of a float as the shortest round-trip decimal
    with a `.0` for an integral value and `NaN`, `inf`, `-inf`, and
    `show` of a `str` as the string itself, unquoted.
12. **A threaded run that traps has no comparable trace** (§4). Found
    by `fibc gen` (fibgen seed 162, size 6): the interpreter's threads
    had allocated 38 objects when one trapped, the OS's 34, and the
    tally rule requires the compiled run to have allocated at least as
    many. At an abort the other threads are mid-flight on both sides
    under different schedules, so neither the live tally nor the
    allocation counts are determined. The harness now compares only
    the message and the exit status for a trap when either side spawned
    a thread; is that acceptable, or should a threaded trap be judged
    on the trace of the trapping thread alone, which would need the
    thread named in both traces (item 2)?
