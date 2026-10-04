# fibc: the compiler

**Decided** (owner, 2026-09-30; proposed 2026-09-28, M4). What this page fixes is everything the
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
fibc run   [--trace] [-O N] FILE.fib [-- ARG..]
                               ; compile through the JIT and run main; print its result; ARG.. are (args)
fibc build FILE.fib -o OUT [-O N] [-L DIR].. [-l LIB]..
                               ; an executable (lair's AOT), linked against the libraries
fibc emit  FILE.fib            ; print the lIR module
fibc explain FILE.fib          ; the plan, as `fibref explain` prints it (types §9)
fibc cases [DIR]               ; the rule-6 harness (§5); default cases/ownership
```

`fibc run` prints `main`'s result as one decimal line on standard
output, after whatever the program wrote there, and exits 0; the
harness reads the last line. The executable `fibc build` makes prints
nothing of its own: `main`'s result is its exit status, as C gives it
(the low 8 bits, lir.md §7.2), so a compiler written in fibber returns
0 or 1 like any other (M5). On both, a trap prints `trap: MESSAGE` on
standard error and aborts (SIGABRT, as types §8.12 says), so the exit
status is 134 under a shell; the command line after the program (or
after `--` for `fibc run`) is `(args)`. With `FIB_TRACE=1` in the
environment the runtime also prints the trace of §4 on standard error;
`FIB_TRACE=1 fibc run FILE` and `fibc run --trace FILE` print the same trace,
because the evaluation of the program's `def`s before `main` (§8, item 4) starts the
runtime with `fib.rq-init` and not `fib.init`, which is the one that reads
`FIB_TRACE`, so those objects are not traced (they were: case 203 counted 1050
objects with the variable set and 4 without). `fibc itrace FILE` runs the
interpreter and prints its trace alone on standard output; the result and the
audit follow on standard error in `fibref`'s words.

**Optimisation level** (**Proposed**, stdlib design §7 C6; the owner's decision of
2026-10-01, §9 Q15). `-O N` (or `-ON`; `N` is one digit, 0 to 3, given at most once; `run`
takes it before the file, in either order with `--trace`, and `build` anywhere after the
file) is the LLVM optimisation level: the IR pipeline `default<ON>` and the code
generator's level (`lair::aot::Options::opt_level` for `build`,
`lair::JitOptions::opt_level` for `run`; `lair`'s own defaults stay 0). **`fibc build`
compiles at level 2 unless told otherwise and `fibc run` at level 0**: an executable is
compiled once and run many times, and a run compiles the program afresh every time, at
a cost that level 2 multiplies by about two and a half (a main of 17,256 lIR lines takes
0.40 s at level 0 and 1.00 s at level 2 under `lair run`, stdlib design §6.4). A level
that is not one digit from 0 to 3 is a usage error (exit 2) before anything is read.
The macro-time JIT of §6 stays at level 0. Tested by `crates/fibc/src/command.rs` (the
parse and the two defaults) and `crates/fibc/tests/cli/opt.rs` (the executable that
`build` writes without `-O` is the same bytes as `-O 2`'s and not `-O 0`'s, less than half
its size for a small loop; `run` takes every level and gives one result).

**Libraries** (**Proposed**: the owner asked on 2026-10-01 for `-L`, `-l` and an
rpath to close the gap of §9; the details below are not yet signed). A program reaches a
foreign function by `extern` (syntax §3.15); `fibc build` links it
against libc and libm and libpthread always, and against more with
`-l LIB` (linked as `-lLIB`, after the program) and `-L DIR` (a
directory the linker searches), each repeatable, in any order after the
file, and `-LDIR` and `-lLIB` as `cc` takes them. Each `-L DIR` is also
an **rpath**, of the directory's absolute canonical path, so that the
executable finds its libraries when it is run from any directory and
without `LD_LIBRARY_PATH`; a relative `-L` is relative to where `fibc`
ran. A directory that does not exist, is not a directory, or whose
canonical name holds `:` or `$` (an rpath reads the first as a
separator and the second as a variable such as `$ORIGIN`) is refused
before anything is compiled, with `fibc: -L DIR: REASON` and exit 2; a
symbol or library the linker cannot find is `compile failed:` with the
linker's own words and exit 5. `-l` and `-L` belong to `build`: `fibc
run` resolves an `extern` in its own process (libc, and what the process
has loaded) and reports `undefined symbol` for any other. The link step is
`lair`'s (`lair::aot::build_executable` with `Options { libs, lib_dirs }`,
lir.md §11): `fibc` hands it the `-l` and `-L` lists and checks each
`-L` first (`lair::aot::library_dir`) only so that a bad directory is
exit 2 before the front end runs. Tested by
`crates/fibc/tests/cli/link.rs` (the flags, through the `fibc` binary: a
shared library built with `cc`, the executable run without
`LD_LIBRARY_PATH` and from another directory, the same executable failing
once the library is moved, and the link failing without the flags) and by
`crates/lair/tests/link.rs` (the same on `Options`).

**Library roots** (**Proposed**, stdlib design §7 E7; the rule is syntax §5).
Every command that reads a program (`run`, `build`, `emit`, `explain`,
`itrace`; `fibref run` and `fibref explain` likewise) takes `-I DIR` (or
`-IDIR`), repeatable and anywhere before a `--`, and reads the environment
variable `FIB_LIB` (directories separated as `PATH`'s are; an empty entry
is skipped). A module `a.b` is looked for at `a/b.fib` in the main file's
directory, then in each `-I DIR` in the order given, then in each directory
of `FIB_LIB` in order, then in the library the executable carries: the
`.fib` files of the repository's `lib/` other than `lib/prelude.fib`,
embedded by `crates/fibref/build.rs` as the prelude is by `include_str!`
(`a/b.fib` there is the module `a.b`). The first that has the file wins and
the rest are not read; a directory that is not there is skipped; a file that
is there and cannot be read (a directory, not UTF-8) is an error, not a
reason to try the next root. A module found nowhere is `module a.b is not at
FILE: REASON; nor at FILE2, ..`, the first file being the main directory's.
`-I` on a command that reads no program (`cases`, `gen`) is a usage error
(exit 2), not ignored. An executable that `fibc build` makes carries the
modules it was built from and needs no root when it runs. The roots are
values passed down from the command line (`fibref::roots::Roots`);
nothing below it reads the environment. The case harness (§5) gives a case
the roots of its header's `roots` key (`cases/modules/README.md`), to the
interpreter as roots and to the child `fibc run --trace` as `-I` flags, and
removes `FIB_LIB` from the child's environment, so a case does not depend
on the machine. Tested by `crates/fibref/src/roots.rs`,
`crates/fibref/tests/roots.rs`, `crates/fibc/tests/cli/roots.rs` and
`cases/modules` 013 to 016.

## 2. Modules

One compilation produces one lIR module: the runtime (§3), the type
table, the static objects, and every specialised body. Every symbol is
`internal` except `main`, so that two such modules can live in one
`Jit` (the macro-time module of §6 and the program) without clashing.
`main` is lIR's `(main i32) ((i32 argc) (ptr argv))`: it initialises
the runtime (reads `FIB_TRACE`, keeps the command line for `(args)`),
calls the program's `main` specialisation, joins the threads still
running (types §8.8), and then, under `fibc run`, prints the result
and returns 0, or, in an executable, returns the result.

Names. A fibber type prints as types §1 writes it; its **mangled form**
replaces `(` by `$`, `)` by `_` and a space by `.`, and writes each
nominal type behind the module that defines it and a `/` (none for the
main module): `(Vec (Box i64))` of the prelude is
`$fib.prelude/Vec.$fib.prelude/Box.i64__`, a main-module `Pt` is `Pt`
and module `a`'s is `a/Pt`, so that two modules' types of one name,
which are different object layouts and different instance heads, have
different symbols (**Proposed**, stage-1 fix s1a, case
`cases/modules/025`). A function specialisation is `f.NAME` followed by
the mangled key of §7 (`f.count.$Vec.i64_`); an all-owned body adds
`.owned`; a method implementation is `m.PROTO.METHOD.` + the head's
mangling, where `PROTO` is the protocol's name behind its module's prefix
(`NS.`; none for the main module, `fib.builtin.` for the checker's own
protocols `Eq`, `Ord`, `Hash`, `Show`), so that two modules' protocols of
one name have different symbols (**Proposed**, stdlib design §7 B3); a
closure body `l.` + the body's name + the literal's
expression id; an object struct `%struct.o.` + the mangled type; a
string literal `str.N`; the type table `fib.types`; a vtable
`vt.PROTO.` + head (the same `PROTO`); a runtime function `fib.NAME` as §8 names it.

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
| `fib.drop-one`, `fib.drain`, `fib.drop-fields`, `fib.share-queue` | `(ptr wl, ptr p) -> void`, `(ptr wl) -> void`, `(ptr p) -> void`, `(ptr wl, ptr p) -> void` | the **iterative** drop and share-marking of types §8.2: `fib.drop` makes a worklist (32 entries in a 256-byte `alloca` on its own frame, spilling to a heap buffer that doubles; one per top-level drop, private to the thread, no global) and calls `fib.drop-one`, which clears weak references, calls the type's `drop` function (slot 0 of `fib.types`; it **queues** the children last first and releases none) and frees the object, then `fib.drain` pops the worklist and releases each entry as `fib.release` does; the decrement is made when the entry is popped and not when it is queued, which keeps the `F` lines in the order a recursion would produce when a child is shared (case 228). `fib.drop-fields` is the same for a `STACK` object at its scope end: the children, not the object. `fib.share` marks a task's captures SHARED the same way, through the type table's slot 4 (`share.N` per type); the one-level `trace.N` callback walker of slot 1 (the retains of a copy, `fib.immortalise`) is unchanged (`cases/ownership` 225 to 231) |
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
   still does), and a trap the same message;
5. a header with `allocs: <= N` (method.md rule 3) is checked against
   the number of `A` lines of the **compiled** run's trace
   (`Trace::allocs`), which is the interpreter's count
   (`fibref::heap::trace_allocs`) whenever the traces agree under §4;
   in a threaded run the compiled program may allocate more (§4), and
   the count that is checked is the compiled one. A case whose child
   did not run (rejected, unsupported, trapped) has no count, and
   `allocs` is for `accept` cases only.

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
macro's body, constructors and readers of `Form` objects and the table
of the keywords it interned (`abi.rs`): the width of an `Int` or `Flt`
form is a keyword id, `fibm.kw-count.K` is the number of keywords and
`fibm.kw.K` maps an id to the keyword's name, a `str` object, or to null
for an id that is none; the ids of `:i8` to `:f64` are found in the table
by name, never assumed, and a module whose table lacks one is refused.
The table is in the module's text, so a runner written in another
language reads it the way `macros/module.rs` does and so does
`compiler/lair/fibm.fib`; the runner converts the argument forms into
objects, calls the entry through the `ccc` trampoline the `Jit`
provides, and reads the result graph back into the expander's `Form`.
`gensym` and the reflection
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
ptr opt box dyn`). Inside a class-keyed specialisation the compiler
substitutes a **representative** type of the class for the variable
(`str` for `ptr`, `(Option str)` for `opt`, `(Option (Option i64))` for
`box` (stage 1 used `(Option i64)`, which stage 2 holds as a pair), a
`dyn` for `dyn`, the scalar itself, and for the pair `{ i1 t }` of an
`(Option scalar)` the `(Option` of the scalar's representative), so every
expression has a concrete
layout; `opt` is an `(Option T)` held as a nullable pointer and `box`
one held as a heap enum (types §8.1: a unit, `dyn` or `Option`
payload; in stage 1 also a scalar), kept apart because an `(Option a)` is a
nullable pointer when
`a` is a `ptr` and a heap enum when `a` is `opt` or `box`; nothing in
such a body can dispatch on the variable, since it has no bound. Method
calls whose resolution is a bound of the scheme are resolved per
specialisation to the instance of the key's full type. Bodies are
emitted on demand from `main` (and from every `(dyn P e)` vtable
reached), each key once.

## 8. Questions for the owner, and their answers

Recorded as they arose; none changes §8. Every item was decided by
the owner on 2026-09-30, the undated ones by accepting the design
as described.

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
   the two to emit the same constants, text for text. A boxed `Option`
   (`nil` and `some` of a scalar or of an `Option`) is a constant
   enum object of tag 0 or 1 in both backends (a null `nil` the JIT
   reads is emitted as the nil object, as the interpreter's is), and
   the value of each `def` is recorded as it is made, so a later
   initialiser may name an earlier `def` (cases/ownership/222). Still
   unsupported in a `def`: a closure other than a named function, a
   `dyn`, a cell, an atom.
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
11. **The text of `show` on a float and on a `str`** (**Decided**,
    owner, 2026-09-30, amended 2026-10-01; types §2.12; cases 178 and
    187): a `str` shows as itself, unquoted; a float as Clojure's text,
    Java's `Double.toString` (stdlib design §7 C12): the shortest
    decimal that reads back at its width (the nearest of that length, a
    tie going up; two digits where one would do, `4.9E-324`), positional
    with `.0` when integral for `1e-3 <= |x| < 1e7`, otherwise
    `d.dddE<exp>` with no plus sign, and `NaN`, `Infinity`, `-Infinity`.
    The interpreter printed both with Rust's `{:?}`, which the runtime
    could not reproduce; it now follows §2.12 (eval/floattext.rs
    `float_text`: Rust's `{:e}` digits, and `{:.1e}` where one digit
    would do), and the runtime finds the same digits with libc: the
    exact expansion from `%.800e`, then for each length n from 2 the two
    n-digit decimals that bracket the value, kept if `strtod` (or
    `strtof`) reads them back, the nearer winning and a tie going up,
    the trailing zero of a length-2 result dropped, and the layout laid
    out by `fib.fp-text` (`rt/str.lir` `fib.show-fp`;
    `fibc/tests/floats.rs` compares 64,000 values with `float_text`
    and with a model of the rule that shares no code with either, and
    91 hand-written values with the texts Java gave; case 187 pins
    the ties). The first version took the first `%.*e` from 0 up that
    read back, which rounds a tie to even and misses a shorter reading
    above a power of two.
12. **A threaded run that traps has no comparable trace** (**Decided**,
    owner, 2026-09-30; §4). Found by `fibc gen` (fibgen seed 162, size
    6): the interpreter's threads had allocated 38 objects when one
    trapped, the OS's 34, and the tally rule requires the compiled run
    to have allocated at least as many. At an abort the other threads
    are mid-flight on both sides under different schedules, so neither
    the live tally nor the allocation counts are determined. The
    harness compares only the message and the exit status for a trap
    when either side spawned a thread.
13. **Stage 2 holds an `Option` of a scalar as a value, the interpreter
    as a heap enum** (performance batch 4, lever A; types §8.1,
    docs/design/unboxed-option.md). The compiled count of heap objects
    for a program that makes such an `Option` is lower than the
    interpreter's, where §4 and method.md rule 3 say the traces are the
    same lines. The `allocs: <= N` bound still holds (a compiled run
    allocates at most what the interpreter does), and a case whose
    bound only the compiled count meets (`cases/ownership` 267) carries
    `;; stage: 2`, as compiler/mirror-pending/X9c-stage2-only.md says;
    the memory audit is unchanged (there is nothing to leak). The
    frozen Rust `fibc` still boxes.

## 9. The C interface to lair (M6)

**Retired by `native.*`** (lair in fibber, stage 7, 2026-10-04). What follows is what the interface was: the 23 `lair_*` functions of
`liblair.so` that stage 2 called through `extern`. The compiler no longer calls any of them. Its users (`emit.defs.jit`, `macros.runner`,
`driver.native`) require `native.api` (the names of `lair.jit`: `jit-new`, `jit-add-source`, `jit-c-entry`, `check-source`,
`build-executable`, ...) and `native.call` (the names of `lair.call`: `call-i64`, the mailbox and its hooks), which are lair written in
fibber (`compiler/lir/`, `compiler/native/`) over LLVM's C interface (`compiler/llvm/`); `native.fibm` and `native.expand` are `lair.fibm` and
`lair.expand` moved onto them. A stage 2 links LLVM 21 itself (`fibc build compiler/fibc.fib -L DIR -l LLVM-21` for development, the static
archives through the ld script of `scripts/llvm-static.sh` for a release) and has no `liblair.so`: `ldd` of a release `bin/fibc` shows libc,
libm, libstdc++, libgcc_s, libz and libzstd. Fibber cannot export a C function, so `lair.h` cannot be offered again: nothing of it is kept
for third parties (owner decision D2 of docs/design/lair-in-fibber.md). `crates/lair` with its `liblair.so`, `compiler/lair/` (`lair.jit`,
`lair.call`, `lair.ffi`, `lair.err` and the originals of `lair.fibm` and `lair.expand`, for `compiler/jit-demo.fib` and the tests of
`crates/fibc/tests/capi/`) stay as a legacy oracle until the Rust `fibc` goes (stage 10), then everything under this heading goes with them.

**Decided** (owner, 2026-10-01): the self-hosted compiler reaches `lair`
through a C interface, as LLVM stays in C++ and `lair` in Rust
(ROADMAP M6). Stage 2 is a fibber program: it calls the functions below
through `extern` (syntax §3.15), and `fibc build FILE -o OUT -L DIR -l lair`
links it against `liblair.so`, a `cdylib` of the `lair` crate whose
header is `crates/lair/include/lair.h`. The function list is
**Proposed**; the operations are the ones `fibc`'s own runner already
performs through `Jit` (§6), so the Rust runner and the C interface
cannot drift apart: both are `lair`'s `Jit` and `aot`. The bindings are
`compiler/lair/*.fib` (bootstrap.md §1); `compiler/jit-demo.fib` uses
them, and `crates/fibc/tests/capi.rs` builds it with `-L` and `-l` (§1) and
runs it: a JIT session, the checker, executables, a hook round trip, and
real macro modules (the cases of `cases/ownership`) run from fibber and
compared with `JitRunner`.

Conventions. A string is a pointer and a byte length (UTF-8, not NUL
terminated; a null pointer only with length 0). A function that can
fail returns a `lair_error *` (null on success) and gives its value
through an out-parameter; the error holds the message `lair` would
print (the text `lair::Error` displays: one diagnostic per line, no
file name) and is freed by the caller. A null argument the callee needs
is an error, not undefined behaviour, and no panic crosses the
boundary: it becomes an error (or `-1`, or a null) whose text starts
`internal error:`. `lair` keeps no state of its own outside the handles:
the one process-wide thing is LLVM's native target registry, initialised
once, on first use, behind a `Once` (`crates/lair/src/llvm/target.rs`);
handles may be used from any one thread at a time, different handles
from different threads at once (`crates/lair/tests/capi.rs`,
`capi_mailbox.rs` run sessions and mailboxes in several threads).
`lair.h` has the exact C types (every length is a `size_t`; an address is
a `size_t`). The interface is 23 functions, the two tables below: `nm -D
--defined-only target/debug/liblair.so | grep -c ' T lair_'` prints 23,
and a unit test (`crates/lair/src/capi/header.rs`) keeps `lair.h` equal
to the `#[no_mangle]` functions of the source, parameter counts included.

| Function | Does |
|---|---|
| `lair_error *lair_jit_new(int opt_level, lair_jit **out)` | a JIT session (`Jit::new`) |
| `void lair_jit_free(lair_jit *)` | ends it; the addresses it gave die with it |
| `lair_error *lair_jit_add_source(lair_jit *, name, len, src, len)` | parse, check, lower, verify and add a module (`Jit::add_source`) |
| `lair_error *lair_jit_address(lair_jit *, name, len, size_t *out)` | the address of a defined function (`Jit::address`) |
| `lair_error *lair_jit_c_entry(lair_jit *, name, len, size_t *out)` | its `ccc` entry, a trampoline when it is not `ccc` (`Jit::c_entry`) |
| `lair_error *lair_check_source(src, len)` | parse and check, no code |
| `lair_error *lair_build_executable(src, len, path, len, int opt_level, const char *const *libs, const size_t *lib_lens, size_t n)` | compile a module that satisfies the `main` rule and link it; each of the `n` names is linked as `-lNAME`, after libm and libpthread, which are always linked, and is found where the process's `cc` finds it: **the library search is the system's, and no `-L` directory or rpath can be given through this function** (`aot::build_executable` with `Options::lib_dirs` empty) |
| `lair_error *lair_build_executable_with(src, len, path, len, int opt_level, libs, lib_lens, size_t n_libs, const char *const *dirs, const size_t *dir_lens, size_t n_dirs)` | as `lair_build_executable`, and each of the `n_dirs` directories is a library directory of the link: `-L` and an absolute canonical **rpath** (`aot::build_executable` with `Options::lib_dirs`, `aot::library_dir`'s rules: a directory that is missing, is no directory, or has `:` or `$` in its canonical name is an error before anything is compiled), so the executable finds its libraries with no `LD_LIBRARY_PATH`, as `fibc build -L` does (§1). `lair_build_executable` stays, and is this function with no directories. Tested by `crates/lair/tests/link.rs` (an executable built through it runs from `/` without `LD_LIBRARY_PATH`) and `crates/lair/tests/c/consumer.c`; the fibber binding is `build-executable` of `compiler/lair/jit.fib`, which `fibc2 build -L` calls |
| `const char *lair_error_text(const lair_error *, size_t *len)`, `void lair_error_free(lair_error *)` | the message |
| `int64_t lair_call_i64(size_t addr, const int64_t *args, size_t n)`, `double lair_call_f64(...)` | call a C-ABI function at `addr` with `n <= 8` integer or pointer arguments (a float or double *parameter* cannot be passed; a double *result* is `lair_call_f64`); fibber cannot call a function pointer itself. These two have no error channel, so a call that cannot be made (`addr` 0, `n > 8`, null `args` with `n > 0`) is not made and returns 0. A result narrower than 64 bits has unspecified high bits |

**Hooks, without a function pointer from fibber.** A macro-time module
calls back into the compiler for `gensym` and the reflection builtins
(§6): two hook pointers and a context pointer installed with
`fibm.set-hooks`. The compiler here is fibber code, which has no way to
give C a pointer to one of its functions, so `lair` supplies the hooks
and turns each call into a request the compiler answers. The macro runs
on a worker thread of `lair` (64 MiB of stack); a hook call parks that
thread and wakes the compiler's thread, which reads the request, builds
the answer with the module's own constructors, and replies:

| Function | Does |
|---|---|
| `lair_call *lair_call_new(void)`, `void lair_call_free(lair_call *)` | a mailbox |
| `size_t lair_hook1_address(void)`, `size_t lair_hook2_address(void)` | the hook pointers, `(cx, a) -> word` and `(cx, a, b) -> word`; the compiler installs them with the mailbox as `cx` |
| `void lair_call_start(lair_call *, size_t addr, const int64_t *args, size_t n)` | run `addr(args..)` on the worker |
| `int lair_call_wait(lair_call *)` | block until the call returned (0) or a hook is waiting (its arity, 1 or 2); -1, once, after a misuse of the mailbox (below) |
| `int64_t lair_call_hook_arg(lair_call *, size_t i)`, `void lair_call_hook_reply(lair_call *, int64_t)` | the waiting request's arguments; the answer, which resumes the worker |
| `int64_t lair_call_result(lair_call *)` | the value of the finished call |
| `const char *lair_call_fault(lair_call *, size_t *len)` | the text of the first misuse since the last accepted start, or null (**added**: the `void` functions above cannot report one) |

A mailbox used out of order is defined, not undefined. The call that
was out of order does nothing and returns 0 (if it returns a value);
the mailbox keeps the first such "fault" text for `lair_call_fault`
until the next accepted `lair_call_start`, and the next
`lair_call_wait` returns -1 once; the call that was running is left as
it was and is collected by waiting again. Refused: a start while a
call is running or parked (the call goes on), with address 0, with
`n > 8` or with a null `args` and `n > 0`; a reply with no hook
waiting; `lair_call_hook_arg` with no hook waiting or an index not
below its arity; `lair_call_result` before the call is done. A hook
called by any thread but the call's worker, or with no call running,
returns 0 at once (waiting would deadlock the compiler's own thread).
`lair_call_wait` on a mailbox that never started returns -1; on a
finished call, 0 again; on a parked one, its arity again. A mailbox
may be reused for the next call once the last has finished. Freeing a
mailbox whose call is still running or parked **detaches** it: the
worker keeps its own reference to the shared state, so nothing it
touches is freed, but it is not stopped (it cannot be, safely): it
runs on and its result is dropped, or, parked, stays parked forever;
that thread and a few words leak until the process ends, and the
session must not be freed while a detached worker might still run its
code (freeing it under a running call is undefined). Free after the
call is done joins the worker and leaves nothing. Null handles
do nothing (wait gives -1, the value readers 0).

The objects a macro builds and reads are plain heap data of its module
(the names `fibm.kw.K` returns are immortal `str` objects of the module,
and the compiler reads the whole keyword table once, when it looks the
module up, so an id in a form the macro returned is named as Rust names
it, and a width the table lacks is the lookup's error, not a form's);
the worker runs from `lair_call_start`, and again from each
`lair_call_hook_reply`, until its next hook call or its return; the
compiler's thread touches the module's objects only after
`lair_call_wait` has returned and before the next `lair_call_hook_reply`
or `lair_call_start`. So the two never run on the module's data at
once, and each hands the other the mailbox's lock, so the module's
non-atomic counts are safe. The rule is the compiler's to keep: nothing
here can see it broken.
A trap in the module aborts the process, as every trap does (types
§2.11). Memory is the process's and is not freed at expansion time, as
in §6.
