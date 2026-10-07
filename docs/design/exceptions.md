# Catching traps: exceptions for fibber

Status: design (EXC0, 2026-10-05). Nothing in `compiler/`, `lib/`, `rt/` or `spec/` is changed; the only code is prototypes under
`docs/design/exceptions/proto/` and every number below was run in this session on this machine (x86-64 Linux, 28 cores, shared with other
agents, so timings carry noise; the command that produced each number is quoted). Nothing here is **Decided** by the owner except what
section 1 quotes. Section 7 lists the decisions with recommendations.

Tree: main at `d7c9622` (RR1: `crates/` is gone; the runtime is `rt/*.lir`). Toolchain: stage 2 `F` copied from
`~/.cache/fibber-scratch/gate-main-gate/F` (`fibc 0.1.5`), `lairf` built from `compiler/lairf.fib`, LLVM 21.1.8 (`/usr/lib/llvm-21/bin`: `opt`,
`llc`, `lli`, `llvm-size`), `g++`. No `clang`. Scripts under `proto/` name the scratch directory `~/.cache/fibber-scratch/EXC` (with a symlink `W` to
the worktree); copy them there to rerun.

## 0. Short answer

- **Recommended mechanism: (b), a transitively inferred "may throw" effect with a hidden result, reusing the scope-exit release code**, reached in
  two stages. Stage 1 needs no compiler change beyond the thread entry: a task that traps is isolated by the runtime and `try-join` returns the trap
  (a prototype of it runs 911 to exit 0 and prints 5; what the dead task owned leaks, and the trace shows how much). Stage 2 is `try`/`catch`/`finally`
  with real unwinding.
- Why not landing pads (a): lIR cannot express `invoke`/`landingpad`/`resume`/a personality (section 3.1), so it is work in three layers (spec,
  lair, runtime personality); on this toolchain unwinding through JIT frames does work (ORC on x86-64 Linux and on aarch64 macOS, not MCJIT), but a throw
  through 10 frames cost 5.5 us against 58 ns for (b), and the hot path got 0 to 13 % slower on the six shootout programs I converted (a lower bound on the
  cost, a crude transformation; section 3.1).
- Why not setjmp/longjmp (c) as the final answer: it is 166 ns per throw and free on the happy path, but every object live in the skipped frames leaks
  (2,000,000 leaked objects after 200,000 throws through 10 frames), which is the one thing the audit forbids and a server cannot afford. It is
  the right first step at a *task* boundary only, where the leak is bounded by one task.
- Two memory-safety facts decide the design and are not "plumbing" as stdlib section 2.10 says: (i) `&` parameters taken instead of acquired (types section 6.6)
  and `cell-update!`/`array-take!` leave **moved-from slots**; after a caught trap those are use-after-free or null reads unless unwinding writes back,
  poisons, or the region is proven trap-free (section 2); (ii) a trap is not one thing: out of memory, stack overflow and a failed thread start must stay
  fatal (section 1.2).
- The language server can be made to answer -32603 **before any of this lands** by forking per query handler (86 us at a small heap, 1.7 ms at a 96 MB
  heap, measured) with the parent reading the child's exit status; section 5.

## 1. Requirements and semantics

### 1.1 What a trap is today (read, and run)

- types section 2.11 (**Decided**, owner, 2026-09-28): a trap aborts the program, on whichever thread; nothing runs after it; objects live at the abort
  are not leaks; "a trap is not an exception: there is no handler". A case may expect it (`expect: trap`, method.md rule 3).
- Compiled: `fib.trap-bytes` (`rt/core.lir`) writes `trap: MSG` to fd 2 and calls `abort` (SIGABRT, exit 134). In a macro module it calls a hook instead
  (`@fibm.trap-hook`, `compiler/macros/runner.fib` `trap-shim`), a second, existing and leaking recovery path that is not general (it fails the expansion
  and never returns into compiled code).
- lIR `(trap)` is `llvm.trap` "without unwinding" (spec/lir.md line 638).
- A trap on a spawned task ends the process. Ran, with the scratch `F`:

```
cd ~/.cache/fibber-scratch/EXC; FIB_LIB=W/lib ./F build t2.fib -o t2 && ./t2; echo "exit $?"
trap: boom
exit 134
```

where `t2.fib` is case stdlib/911's program (`proto/t911.fib`).

Traps in the corpus: 207 case files have the header `;; expect: trap` (`grep -rl "^;; expect: trap" cases | wc -l`; the arithmetic ones 101 to 104, index and window ones, str ones, library
ones such as stdlib/003, 016, 023, 051, 053). Runtime-originated traps are few: `grep -n 'fib.trap-c (string' rt/*.lir` gives `out of memory` (3),
`str-from-bytes: invalid UTF-8`, `show: no shortest digits`, `spawn: cannot start a thread`, `async: cannot start a worker thread`; every other trap
is emitted by the compiler (`lcx-trap-if`, `lcx-trap-c` in `compiler/emit/lower/cx.fib`) or is a call of `trap` in `lib/`.

### 1.2 What must be catchable and what must not

| Failure | Catchable | Why |
|---|---|---|
| `(trap msg)`, and `(throw e)` (a trap carrying an `ExInfo`) | yes | the point |
| library traps: `nth`/`array-get`/`array-take!` out of range, `unwrap: nil`, `case: no matching clause`, parse traps, `lazy-seq: a seq forced itself`, `assert` | yes | Clojure throws these; a server must survive a bad input |
| integer overflow, `/` and `rem` by zero, checked conversions, `str-from-bytes` | yes | the same |
| a trap raised by the **joined task** | yes, as the value of `try-join` (stage 1) | S16, case 911 |
| `out of memory` (`fib.alloc`, 3 sites in `rt/core.lir`, case ownership/195) | **no: fatal** | a handler and the unwinder allocate; the runtime may be half way through building an object; recovery cannot be argued |
| stack overflow | **no: not a trap** | it is SIGSEGV on the guard page; there is no code to run, so it can never unwind. Since LANG-2 a `sigaltstack` handler prints `trap: stack overflow` and aborts (status 134, every thread, `try` or not; docs/design/stack.md, cases 8350-8354) |
| `spawn: cannot start a thread`, `async: cannot start a worker thread` | no: fatal | resource failures of the runtime |
| memory corruption, use after free, a negative count | no | none of them is detected as a trap; the audit stops the *interpreter* run (`HpErr`), compiled code does not detect them. A catch must never be able to hide an audit error |
| hardware faults (SIGSEGV, SIGFPE, SIGILL from `(trap)`) | no | lIR `(trap)` stays `llvm.trap`: the path for faults without a message is fatal by definition |
| `unsafe` code that violates its own obligation | no | outside the model |

So the runtime gets two entries: `fib.trap` (recoverable, takes a kind) and `fib.fatal` (today's behaviour). Reclassifying `rt/*.lir` is 5 call sites.
Trap kinds, as a small closed enum carried with the message: `user` (`trap`, `throw`), `index`, `overflow`, `arith` (zero divisor), `convert`, `assert`,
`absent` (`unwrap`, `case`, refutable `let`), `invalid` (UTF-8, parse). The kind is chosen by the emit site, which already has the message; the message
texts stay as they are (cases pin them).

### 1.3 Semantics wanted

1. `(try body (catch e handler) (finally cleanup))` is an expression of the type of `body` (and `handler`), `finally` of type `unit`. `e` has type `ExInfo`
   (stdlib section 2.10 step 1: message, `(Map keyword Val)`, `(Option ExInfo)` cause). A library trap arrives as an `ExInfo` whose data holds `:fib/kind`.
   `throw` takes an `ExInfo`; `(throw e)` before stage 2 is `(trap (ex-message e))`, as stdlib already says.
2. After a caught trap the program is in a state a later read can trust: no count is wrong, no live object is released twice, no slot a later read
   may reach holds a moved-from pointer (section 2), and the audit of a run that finished is `clean`, because unwinding released what it skipped.
3. A trap inside a `finally` that runs because of a throw is **fatal** in stage 2 (simple, safe; Java's "replace the exception" can come later). A trap inside a
   `catch` handler propagates outwards like any other.
4. A trap that reaches `main` or a task boundary with no catch keeps today's behaviour (message, abort) unless the task boundary is the stage 1 isolation.
5. `Result` and `try-let` stay the typed way for expected failures (stdlib N13 `try-<name>` twins); exceptions are for the unexpected. A sugar form
   `(try-catch body)` returning `(Result a ExInfo)` bridges them.

## 2. The memory-safety argument

stdlib section 2.10 says the hazard is a leak, not corruption. That holds for objects held by ordinary locals. It does not hold for the in-place machinery,
which moves a count out of a slot and leaves the slot pointing at an object that is no longer counted for it. What unwinding must do, case by case:

| What is live at the throw | What unwinding does | What the checker/emit must provide |
|---|---|---|
| Owned `let` bindings and parameters of every unwound frame | release, in reverse order of binding, as a normal scope exit | own already computes the live owned set at each scope exit (types section 6.3, `own.walk.step`). It needs the set **at each call site** (bindings bound so far, not yet moved), a new fact `unwind-live(site)`; the existing exit sets are the special case "at the end of the scope" |
| Temporaries of a call step: arguments already evaluated and Owned, not yet consumed (`(f (g x) (h y))` with `h` throwing) | release | the same fact; the rule that an `Owned` temporary is released after the call (section 6.3 table) gives the list |
| Owned arguments passed at an owned position | the **callee** releases them in its unwind exit (it owns them); the caller must treat them as moved, as on the normal path | callee summaries (section 6.4) already say which parameters are owned |
| Partially built values: struct/variant constructors evaluate their fields first, so no half-built object exists in compiled code | nothing | none. Library loops that build an array element by element (`fib.vecbuild` in `rt/vecbuild.lir`, `array n e`, vector building in `lib/`): the object under construction is held by a local, with `len` as initialised count; its release releases the first `len` elements | check that the runtime's builders keep the count current (`rt/vecbuild.lir`, 100 lines; read, not audited for this) |
| `array-take!` that left a null slot, `array-pop!` mid-way | see below | see below |
| `cell-update!` (content moved out, `f` called with it owned) | see below | see below |
| `&` arguments: the callee's private cell, and the caller's cell if the call **took** (types section 6.6 "move in") | the private cell always holds a valid counted pointer (`set!` consumes the new value before releasing the old). Unwinding performs the **write-back** the normal return would: the caller's cell receives the private cell's content | uniform rule: *after a caught trap an `&` argument holds the callee's last state* (basic guarantee). Today's acquire mode would otherwise keep the old content and the take mode would not; the take is "invisible when its conditions hold" only if both modes write back. No retain in the take mode, one release of the old content in the acquire mode, as on the normal path |
| Exclusive-view lends (`with-view`, types section 6.15): the lent array is moved out of the lent place for the extent, the window points into it | the extent's end must run: the lend is returned to its place | the extent is the initialiser of a `let`; its end is a scope exit and so already a place where emit produces release code. Unwinding must run the *view return* there, exactly as a `finally`. I read the checker (`compiler/own/views.fib` header) and the spec, not the emit of the lend; this is the first thing to check in package U5 |
| Atoms and locks | the spinlock of an atom (`rt/atom.lir`) is held only around loads and stores inside the runtime, never across user code; `swap!`'s `f` runs unlocked (types section 8.6: snapshot `old` retained, `f(old)`, lock, compare) | the snapshot `old` and `f`'s retained counts are temporaries of the `swap!` runtime function: if `f` throws they must be released (the retries loop is lIR in the runtime; its unwind exit is hand-written). Invariant to keep: no lock is held across a call that may throw. A future `locking` (L12, C9) is a macro over `finally` |
| Async frames | a suspended state machine keeps its live locals in the task object; a throw out of `resume` marks the task failed and the task's own release (its trace function) frees what it held | same shape as stage 1 |
| Spawned tasks | stage 1: the thread entry catches; the task is completed as failed, waiters are woken | section 4 |

**In-place primitives: what state does a caught trap see?** Reading `rt` and types section 2.13.1:

- `array-take!` on a unique array of objects sets the slot to **null** and moves the count to the result (`lib/prelude.fib` uses it 9 times: vector
  nodes lines 96, 155, 215 and the shifting loops 426 and 427). If a trap happens between the take and the refill and the array stays reachable, the next
  read of that slot returns null as an object: a crash, not a leak. Unwinding releases the owned result (it is an owned local), so even "putting it back"
  is not possible once released.
- `cell-update! c f` moves the content out of `c`, calls `f` with it owned, stores the result. If `f` traps, `f`'s unwind releases its parameter (maybe
  freeing it) and `c` still points at it: use after free on the next `@c`.
- A taken `&b` (section 6.6): the caller's `b` names a moved-from slot until the write-back.

Three rules, in order of preference:

1. **Trap-free regions** for `array-take!`: the compiler rejects, between an `array-take!` and the store that refills the slot, any call that may throw (a may-throw
   inference is needed by (b) anyway). The 9 library uses are shift loops and node surgery with in-range indices; the check would show whether any of them
   calls through a closure. (A rule "no call, or only calls proved nothrow" is a checker rule of about the size of the existing `own.peek` conditions.)
2. **Poisoning** for `cell-update!` (Rust's `Mutex` precedent): the cleanup of the call stores null and sets a `POISONED` flag in the cell; `@c`/`set!`/the next
   update on a poisoned cell traps `cell: poisoned by a trap in cell-update!` (a null test on an already loaded pointer; a cell that holds a scalar needs
   nothing, the scalar is not moved). A catch that wants to continue calls `(cell-reset! c v)`. This keeps `cell-update!` in place, which is its purpose
   (docs/design/in-place-update.md).
3. **Write-back on unwind** for `&` (above): not a new restriction, it is the normal return's code run from the unwind exit.

An alternative to rule 1 is to make object-array reads null-check ("slot taken" trap). I did not measure the cost of that extra test on `array-get` and do not recommend it
before rule 1 is tried.

**How the audit stays clean.** The audit judges a finished run (every `A n k` freed once, spec/compiler.md section 4 trace; the interpreter's heap also finds
use-after-free). With (b), a caught trap unwinds by running the same release code as a normal exit, so a case that traps, catches and ends has `audit: clean`
and the rule "objects live at the abort are not leaks" (types section 2.11) shrinks to "objects live at an *uncaught* abort". With the stage 1 task isolation
the dead task's stack objects leak (section 3.3); the audit needs a class for that (section 4.1).

## 3. The mechanisms, with measurements

Common method for (a) to (c): `proto/gen.py MODE ALLOC DEPTH` writes LLVM IR for a chain of `DEPTH` functions `f_0 .. f_{D-1}`; with `ALLOC=1` each function
owns one refcounted object (`fib_alloc`/`fib_release`, defined in C, so they are not inlined) across the call to the next; the leaf fails when
its second argument is non-zero. `proto/drv.cpp` calls `f_0` 20,000,000 times (happy path) and then 200,000 times with a failing leaf (caught). Build:
`opt -passes='default<O2>'`, `llc -O2 -relocation-model=pic`, `g++ -O2`. All functions are `noinline` (a worst case: every call is a real call).
Modes: `plain` (today: the leaf aborts), `invoke` (a), `result` (b, `{i64, i8}` return and a branch after every call), `flag` (b', a global flag tested
after every call), `jmp` (c, `longjmp` to a `setjmp` in the caller of `f_0`).

```
bash proto/run.sh 10        # sizes, one run each, and the caught-throw costs
bash proto/rep.sh           # 7 runs of the happy path of each binary, sorted
```

### 3.0 Today's code already carries unwind tables

`lairf emit-llvm` of case 911 (`F emit t2.fib > t2.lir; lairf emit-llvm t2.lir`) gives `define internal tailcc i64 @f.main() #0` with attribute `#0` holding only
`target-cpu` and `target-features`: no `nounwind`, no `uwtable`. LLVM therefore already emits `.eh_frame` for every function (264 to 464 bytes in the
micro programs; 808 to 6640 bytes in the shootout programs, table below), which is why the `plain` rows of the size table already have an `eh_frame`.
Frames are unwindable by DWARF today; what is missing is the personality and the landing pads.

### 3.1 (a) Landing pads: `invoke` / `landingpad`

**Expressible in lIR today? No.** `grep -n -i 'invoke\|landingpad\|personality\|unwind\|resume' spec/lir.md` finds only the sentence on `(trap)`;
`compiler/native` and `compiler/llvm` have no binding of the EH instructions (`grep -rln` over `compiler docs` finds `invoke` only in the call shim comment and in
`docs/design/lair-in-fibber.md`/`relational-search.md`, unrelated). What lIR lacks: an `invoke` terminator with a normal and an unwind destination, a `landingpad`
(cleanup/catch clauses), `resume`, a per-function personality, `unwind_table`/`uwtable`, and the checker rules for them (the landing pad's block must
be reachable only by unwind edges, values live into it need dominance through the invoke). A runtime personality: LLVM's `__gxx_personality_v0` needs libstdc++/libc++abi
(`__cxa_allocate_exception`, `__cxa_throw`); a cleanup-only personality (`__gcc_personality_v0`) needs `_Unwind_ForcedUnwind` with a stop function instead of
the two-phase `_Unwind_RaiseException` (phase 1 finds no handler and returns `_URC_END_OF_STACK`). I prototyped only with the C++ personality.

**Does the JIT unwind?** `proto/jit.ll` throws through a JIT-compiled frame with a cleanup and catches in another JIT frame; run:

```
bash proto/jit.sh
-- lli --jit-kind=orc    -> "mid cleanup", exit=7
-- lli --jit-kind=mcjit  -> "mid cleanup", exit=7
AOT (llc + g++)          -> "mid cleanup", exit 7          (lli: Ubuntu LLVM version 21.1.8)
```

On the Mac Studio (arm64, Homebrew LLVM 21.1.8, `bash proto/mac.sh`, scratch dir `~/fibber-a64-scratch/exc`, removed afterwards):

```
-- lli --jit-kind=orc    -> "mid cleanup", "orc exit=7"
-- lli --jit-kind=mcjit  -> libc++abi: terminating due to uncaught exception of type int  (crash)
AOT (llc + c++)          -> "mid cleanup", exit 7
```

So ORC/JITLink registers the unwind info on both targets and MCJIT does not on aarch64 macOS. lair's JIT is `LLVMOrcCreateLLJIT` with the default builder
(`compiler/llvm/orc.fib`; I found no object-linking-layer override), the same layer `lli --jit-kind=orc` uses; I did not run **lair's own** JIT through an `invoke`,
because lIR cannot express one. Everything that runs generated code through the JIT (the macro runner, `fibc run`) would need exactly this to work;
on the evidence it does, but it is a dependency on a registration that is not under fibber's control.

**Hot path and code size, micro** (7 runs of 20M chain calls, ns per call of `f_0`, sorted; `bash proto/rep.sh`):

```
alloc=1 plain : 55.91 57.15 57.19 57.80 58.72 59.11 60.82
alloc=1 invoke: 52.91 55.29 56.21 56.25 57.41 57.72 58.80
alloc=1 result: 56.96 57.05 57.36 57.64 57.84 58.47 59.14
alloc=1 flag  : 51.42 53.87 54.72 55.06 55.61 56.75 57.47
alloc=0 plain : 7.06 7.70 8.02 8.02 8.10 8.44 9.48
alloc=0 invoke: 6.72 7.27 7.29 7.34 7.54 7.72 8.28     (no landing pad: nothing to clean, plain calls)
alloc=0 result: 6.77 6.92 6.97 7.51 8.14 8.80 8.96
alloc=0 flag  : 4.66 4.73 4.83 4.90 4.97 5.53 5.77
```

No difference is visible beyond layout noise (flag at 4.9 ns against plain at 8 ns is a layout effect I did not chase; I would not read it as a speedup). Sizes
(`llvm-size -A`, bytes, depth 10): with an owned object per frame, `.text` plain 624, invoke 789 (+26 %), result 767 (+23 %), flag 785 (+26 %); `.eh_frame` plain 464,
invoke 512, plus `.gcc_except_table` 160. With nothing to release: plain 163, result 305 (+87 %), flag 314, invoke 163 (no pads at all).
Throw through 10 frames, per throw (200,000 throws): **invoke 5503.6 ns** (with ten cleanups) and 1831.8 ns (without cleanups: the cost of the two-phase
unwind alone); result 58.2 ns; flag 55.6 ns; longjmp 166.1 ns.

**On the shootout programs.** `proto/xform.py IN.ll OUT.ll` rewrites the LLVM IR that `lairf emit-llvm` gives: in every user function (`@f.*`, `@l.*`) each
call to a user function or through a closure pointer that is not in tail position becomes an `invoke` with one cleanup landing pad per function (a call
of an external no-op `fib_cleanup_stub`, then `resume`); `personality` is added to the function. This is a **lower bound** of the real cost: a real pad
releases that site's live locals, which keeps values live into the pad, and library calls that may trap (bounds checks) would be invokes too. `proto/prog.sh NAME` builds
`scripts/bench/NAME.fib` both ways with the same `opt`/`llc`/`g++` line and runs each 5 times (`bash proto/many.sh recursion binary-trees num-nbody vec-sort
map-assoc-get strings`). User CPU seconds, sorted, and bytes:

| program | invokes | plain (s) | invoke (s) | median change | `.text` plain, invoke | `.eh_frame` plain, invoke | `.gcc_except_table` |
|---|---|---|---|---|---|---|---|
| recursion | 6 | 2.38 2.45 2.48 2.59 2.65 | 2.60 2.62 2.63 2.66 2.76 | +7 % | 4477, 4535 | 808, 864 | 92 |
| binary-trees | 13 | 1.47 1.50 1.77 1.94 2.56 | 1.53 1.54 1.69 1.82 2.17 | -4 % (noise: 1.53 to 1.97 in another batch) | 6301, 6409 | 1048, 1112 | 140 |
| num-nbody | 21 | 0.67 0.68 0.70 0.70 0.76 | 0.77 0.77 0.79 0.79 0.79 | +13 % | 18823, 18948 | 1904, 1976 | 392 |
| vec-sort | 38 | 0.45 0.46 0.46 0.47 0.51 | 0.47 0.47 0.48 0.50 0.53 | +4 % | 30727, 31147 | 3576, 3672 | 564 |
| map-assoc-get | 18 | 0.24 0.25 0.26 0.27 0.27 | 0.24 0.26 0.27 0.27 0.28 | +4 % (noise) | 17210, 17476 | 1944, 2016 | 252 |
| strings | 62 | 0.65 0.66 0.66 0.68 0.71 | 0.65 0.65 0.66 0.68 0.70 | 0 | 54301, 54733 | 6640, 6760 | 996 |

(Plain rows differ between batches by up to 20 % on binary-trees: the machine was shared. The `+13 %` of num-nbody was reproducible across two batches; I did not find its
cause, which may be that an `invoke` inside the hot loop blocks a loop transformation or sinks something; I did not look at the optimiser's remarks.) `.text` grows 0.7 to 1.5 %.

**Cost of (a) in work.** lIR spec (`invoke`, `landingpad`, `resume`, personality, rules), the checker in `compiler/lir`, lair lowering in `compiler/native` (EH
instructions through the LLVM-C bindings, and a `-fno-exceptions`-free link line: libgcc_s for `_Unwind_*`), a personality and thrower in `rt/`, then emit. On
aarch64 the same personality ABI applies (Itanium unwinding with `.eh_frame`; Mac JIT: ORC only). Zero cost happy path is real; code size +0.7 to +26 % and 0 to +13 % time here.

### 3.2 (b) "May throw" effect and a hidden result

Prototype: the `result` and `flag` modes above. A may-throw function returns `{T, i1}` (an `i1`/`i8` in a second register for scalars and pointers, a hidden
out-pointer for aggregates larger than two registers); after every call to a may-throw function: `br err, unwind-exit, continue`; the unwind exit of a call site is
**the scope-exit release sequence of that site's live set** followed by `ret {zero, 1}`. A throw is a store of the exception object into the
thread's context and the same return. Measured above: happy path within noise; the throw through 10 frames, with ten releases, is **58 ns**.

Static cost on the shootout programs (the count of branches (b) would add, `python3 proto/xform.py IN.ll OUT.ll` prints it to stderr; "may throw" is computed as the
transitive closure over direct calls from `fib.trap*`, and every closure call counts; an **upper bound**: it also counts `fib.alloc` (out of memory, which is fatal and
would not count) and runtime helpers):

| program | call sites in user functions | calls to may-throw functions or closures (branches) | of which user-to-user or closure calls | may-throw functions of defined |
|---|---|---|---|---|
| recursion | 33 | 29 | 7 | 79 of 174 |
| binary-trees | 59 | 56 | 14 | 82 of 176 |
| num-nbody | 284 | 200 | 25 | 106 of 208 |
| vec-sort | 417 | 309 | 42 | 145 of 253 |

So the effect, as a plain closure of direct calls, is nearly everywhere: any call through a closure or a protocol method of unknown instance must be assumed to throw
(`map`, `reduce`, `swap!` take closures). The remedy is not precision but the nothrow inference being cheap to *prove* for leaves and for the inner loops of numeric code (checked
`+` on `i64` still traps on overflow: those traps are inline sites, each becoming a cleanup-and-return block, which is the main code-size item; they can share one unwind exit per scope with phis).

Things (b) changes that (a) does not: every function's lIR type (`tailcc` `musttail` calls must match return types, so a tail call from a may-throw function to a nothrow one
needs a wrapper or the same return type); closures' code pointers; `rt/*.lir` callbacks that call compiled code (7 `indirect-call`s in `rt/`; the task `resume` is the
one that can throw); the macro-runner ABI (`fibm.*` entries); `extern` callbacks from C (nothrow by rule, a trap in one is fatal). What (b) needs from lIR: nothing new. It needs no
personality, no unwind tables, no libgcc_s, and works the same in the interpreter (`Throw` is an outcome the evaluator propagates, section 4.4), the JIT and every target.

### 3.3 (c) Isolation at task boundaries, and `catch` as a lightweight task

Real costs, measured on fibber programs built with `F build` (`proto/spawn.fib`, `proto/async.fib`, `proto/fork.fib`; build with `proto/b.sh DIR/NAME`, i.e. `F build NAME.fib -o NAME`):

```
/usr/bin/time -f '%e s wall' ./spawn   # 20000 x (spawn (fn () (+ i 1))) + join, vm unlimited
  0.55 s wall, 0.11 user 0.55 sys, maxrss 165660 KB            -> 27.5 us per spawn+join
ulimit -v 16000000; ./spawn           # the same under the 16 GB cap
  trap: spawn: cannot start a thread / Command terminated by signal 6   (1.21 s)
./spawn with 1500 iterations (cap on): 0.04 to 0.05 s, maxrss 12 MB       -> ~30 us
./async  # 2,000,000 x (let [t (async (+ i 1))] (join t))
  0.05 s wall, maxrss 1696 KB                                  -> 25 ns per async+join
```

Finding F1 (independent of exceptions): `fib.spawn` (`rt/thread.lir`) keeps every thread as a *joinable* pthread in `fib.threads` until `main` returns, so a program that
spawns tasks in a loop keeps one thread's resources per task (165 MB for 20,000) and stops at the thread limit (about 2,000 tasks under a 16 GB address-space cap, 8 MB stacks).
A server that spawns per request needs the thread entry to detach (or to reap finished threads) before it can use tasks at all. This is 5 lines and belongs to stage 1.

`spawn` per catch costs 27 us: acceptable for "run the handler of a request" (stage 1), not for a `try` in a loop. An `async` task is a heap state machine run by the joiner,
25 ns: a `catch` form that wraps its body in an `async` and `join`s it would be cheap, but it needs the trap to *end that resume* and return to the joiner without
returning through the frames: that is a `longjmp` to the resume call, with the same leak as below. `setjmp`/`longjmp` itself: 166.1 ns per throw through 10 frames and no happy-path cost, but
**the 10 owned objects of the skipped frames are never released** (`live objects left 2000000` after 200,000 throws; `bash proto/run.sh 10`, `jmp` row).

**Stage 1 prototype (runtime patch in a scratch copy).** `proto/patch.py IN.lir OUT.lir` patches the lIR that `F emit` prints (which includes the runtime, i.e. `rt/*.lir` as
embedded in the compiler): a pthread key; the thread entry stores its task with `pthread_setspecific`; `fib.trap-bytes`, when it finds a task, prints
`trap in task: MSG`, calls `fib.task-complete` (state done, waiters woken), releases the thread's count and calls `pthread_exit`. `bash proto/proto.sh t911` and
`bash proto/proto.sh tt2` (build both ways, run with `FIB_TRACE=1`, count trace lines):

```
--- t911.orig   exit=134  stderr: trap: boom           A lines: 3  F lines: 1
--- t911.proto  exit=0    stdout: 5   stderr: trap in task: boom   A lines: 3  F lines: 3
--- tt2.orig    exit=134                                A lines: 9  F lines: 4
--- tt2.proto   exit=0    stdout: 5                     A lines: 9  F lines: 6     (the task owned a vec and a string when it trapped: 3 objects leaked)
bash proto/tt3.sh (20,000 tasks, each traps while owning a vec, vm unlimited)
  proto: 20000 trapping tasks: 0.85 s wall, 0.22 user, 0.82 sys, maxrss 173760 KB      -> 42 us per trapping task
  A lines 140000  F lines 100000                                                          -> 2 objects leaked per task
```

911's own program passes (it owns nothing at the trap). **What a caught task trap leaks:** everything the task's frames owned. In `tt3` that is the vec and its array. Closure environment and
the result atom are not leaked (the task and the thread release them as on a normal end). Whether the audit can account for it: `A n k` lines carry no thread, so the harness
sees only the difference (`leaks=N`); section 4.1 proposes `audit: abandoned`.

(`join` in the prototype returns the zeroed result atom: `0` for an `i64` task, a **null pointer** for an object-typed task. A program that uses it crashes; this is why stage 1
changes the type of what a joiner can read, section 4.1.)

## 4. Recommendation and staged plan

Order: stage 1 (isolation by join, runtime only) now; stage 2 (b) with `try`/`catch`/`finally`; (a) only if (b)'s measured hot-path cost on the real shootout turns out
worse than the 0 to 13 % above, which I doubt: its happy path is a predictable branch, and the lower-bound invoke numbers are not better.

### 4.1 Stage 1: a task's trap is isolated by `join`

**Types (needs the owner).** Case 911 says `(join t)`'s value may be ignored and the joiner goes on. `join : (Task a) -> a` cannot both give a trap and return an `a`. Options:
- **J1 (recommended)**: `try-join : (Task a) -> (Result a Trap)` (stdlib N13's `try-<name>` twin pattern); `join`, `block-on` and `@t` keep the type `a` and, when the task
  trapped, **trap in the joiner** (Clojure's `@future` rethrows `ExecutionException`; no value is invented). `(defrecord Trap (kind: TrapKind msg: str))`; `TrapKind` the enum of
  section 1.2. Case 911 is then rewritten to use `try-join` (the old text with `join` would abort `main`, as Clojure's `deref` would throw): a **change to an `open` case's program**
  that the owner approves, and 911's header says it "does not fix that type".
- J2: `join : (Task a) -> (Result a Trap)` for everything; all 33 `spawn` cases and the `Deref (Task a)` instance change; 911 stays as written. A bigger break; consistent with Rust's `JoinHandle::join`
  (which stdlib section 2.10 cites as the model) but not with Clojure.
- J3: `join` returns a `TaskResult` the program may ignore, `@t` returns `a`. The same as J1 with a different name for the same type; no benefit.

**Package S1a, runtime (~60 lines of lIR in `rt/core.lir` and `rt/task.lir`, `fib.thread.*` entry in `compiler/emit/lower/threads.fib`, ~10 lines).** The prototype above, made real:
a task records the trap (state `3`-like `failed` with `kind` and message in the task object, a field the layout adds), `fib.task-complete` wakes waiters, awaiters of a failed task
are woken to find the failure; the thread detaches or is reaped (F1); `fib.fatal` for OOM and the thread-start failures; a trap on the **main thread** or in a macro module is as today.
Which thread is "in a task" is a `pthread` key now; lIR has no thread-local storage, so the per-thread context pointer should live in the thread entry's frame and be passed
to trap sites through a runtime global per thread (`pthread_getspecific` cost on a trap path only).
**Package S1b, library:** `Trap`, `try-join`, the trapping `join`/`@t`, `ex-info`/`ex-message`/`ex-data` (stdlib step 1; plain data, ~60 lines of fibber with `defrecord`).
**Package S1c, audit:** a failed task's stack objects are *abandoned*. `fibc cases` reads the trace: add the header verdict `audit: abandoned` (some leak, no error, like the existing `audit: leak-cycle`
which "holds when something leaked and nothing erred"); an otherwise clean case must stay `clean`. The interpreter port runs a task to completion lazily
(fibber-interpreter.md 3.3): a trapping task there is an outcome, and its abandoned objects are the ones its frames owned: the interpreter can *name* them, which the trace cannot.

**Tests that can fail (stage 1):**
1. `911` (rewritten with `try-join`): result 5, exit 0, `audit: clean`; must fail on `F` today (exit 134).
2. A task that traps while owning a vec and a string: `audit: abandoned`, and the leak count equals what the task held (fails if the runtime releases twice, or leaks the task itself or its result atom).
3. `try-join` returns `Err` with the message and kind; `Ok v` for a task that does not trap; a joined-twice failed task returns the same `Err` both times.
4. `join` of a failed task traps in the joiner with the original message; main exits 134 (the old expectation, `expect: trap`).
5. 5,000 sequential tasks that trap: process finishes under `ulimit -v 16000000` (fails today and in the prototype without the detach of F1).
6. A trap in a task nested in a task; a joiner parked on a task that traps (async `await` of a failed task: the awaiter fails the same way); a trap in task A while task B is running on another thread (B finishes).
7. A trap in `main` while a task runs: the old abort.
8. OOM (case ownership/195) stays an abort even inside a task: it must not be isolated.

### 4.2 Stage 2: `try` / `catch` / `finally` by (b)

**Syntax.** `(try body (catch e handler) (finally cleanup))`, `finally` and `catch` optional but not both absent; one `catch`, `e : ExInfo` (kind in `(ex-data e)` under `:fib/kind`; the
Clojure shape `(catch Type e ..)` can be added when `ExInfo` has subtypes, so `(catch Trap e ..)` / `(catch ExInfo e ..)` are reserved now). `try` is a **core form** (the expander cannot write it: no early exit). `throw : ∀a. (fn :send (ExInfo) a)`.
Type: `body`, `handler` unify; `cleanup : unit` and runs on both paths. `(try-catch body)` is a macro for `(try (Ok body) (catch e (Err e)))`. `try-let` is untouched.
**`:send` and closures.** `try` is a form, not a function, so colour is that of its parts; the `ExInfo` payload crossing a task boundary must be `Send` (the data map of `Val`s is `Send` if its values are).
An exception propagates through closures called by library code (a `map` over a closure that throws) because a closure is a frame; the closure's type does not change (the effect is not in the type: it is an
inferred property of the compiled function, like `escapes`). **Async:** `await` inside a `try` extent is rejected (`await inside try`, as `await inside with-view`, section 6.15 L5) in v1; a throw out of an `async` body completes its
task as failed (stage 1's mechanism).
**Checker rules.** (i) the exception value is an owned binding of the handler; (ii) a **take** (`&b` move in) or `array-take!`/`cell-update!` inside a `try` extent is subject to the rules of section 2 (write-back, trap-free region, poison);
(iii) `finally` is a scope exit on both paths; a trap in a `finally` run for a throw is fatal (section 1.3); (iv) `recur` and tail calls are not allowed from inside a `try` extent (the frame must stay to run the cleanup; `recur not in tail position`, section 6.10 style); (v) `with-view`'s extent is a `try`/`finally` in its expansion.

**Packages** (sizes are my estimates from the shape of existing passes, not measured):
| # | package | content | size |
|---|---|---|---|
| U0 | spec | types section 2.11 amendment (the owner signs it: it reverses the 2026-09-28 decision, stdlib section 2.10 / 5 D1 / 9.1 Q35), syntax section 3 `try`, stdlib L28 / S9 / S16 rows, lir.md unchanged | docs only |
| U1 | library | `ExInfo`, `Trap`, `ex-*` (if not done in S1b), `throw` as a trap of `ex-message` until U3 | S |
| U2 | `compiler/own` | may-throw inference (a fixpoint over the call graph in `own.facts`, like `escapes`); `unwind-live(site)` facts; the take/trap-free/poison rules; checker errors | M (about 400 lines; the live-set fact is the subtle part) |
| U3 | `compiler/emit` | hidden-result ABI for may-throw functions, unwind exits sharing the release sequence, thread context for the in-flight exception, closures and `musttail`, `rt/*.lir` runtime entries (`fib.throw`, `fib.fatal`), `swap!` loop, `resume` | L (touches lowering of every call; about the size of the M5 ownership emission) |
| U4 | `try` form | reader/expander core form, typing, ownership walk for `catch` and `finally`, lowering | M |
| U5 | in-place and views | `array-take!` trap-free check on the 9 library uses; `cell-update!` poisoning and `cell-reset!`; `&` write-back on unwind; `with-view` return on unwind (read `emit` of the lend first) | M |
| U6 | interpreter port (`fibref`) | section 4.4 | M |
| U7 | consumers | `with-open`, `locking`, `future-cancel` (S16), `fibc lsp` handlers | S each |

**Tests that can fail (stage 2),** each a `cases/` program with a header verdict, each with the mutant that it kills named in its header (the way `scripts/mutant-amp-param.sh` works):
1. *leak after catching:* a function owning a vec and a string calls a function that traps three frames down; `catch` returns a value; `audit: clean`. Kills: unwind exit that skips a release.
2. *double release:* the same, where the object is also released by the normal path after the call returns (a `finally` that releases and a scope that releases). Kills: an unwind exit that also falls through to the normal exit; must fail with the audit's double free (interpreter) or an `F` twice (trace).
3. *trap inside `finally`:* `finally` traps while unwinding: exit 134, message names the second trap; and a `finally` that traps on the normal path propagates as an ordinary trap and is catchable by an outer `try`.
4. *nested try:* inner `catch` rethrows, outer `catch` catches, both `finally` clauses run in order; objects owned at each level are freed (`audit: clean`).
5. *trap in a task inside a try:* `try` around `(try-join t)` and around `(join t)`; the latter's `catch` sees the task's message and kind.
6. *trap while an `array-take!` slot is taken:* a library-shaped loop that takes a slot, calls a function that traps, with the array reachable after the catch: must be **rejected by the checker** (trap-free region), and, as an `unsafe` variant, must read the array after the catch without a null read (`expect: trap` of the slot-taken kind, not a crash). Kills: removing the rule.
7. *`cell-update!` whose `f` traps:* the next `@c` traps `cell: poisoned..`, not a use after free; `cell-reset!` makes the cell usable again; the audit is clean. In the interpreter the stale pointer is an audit error if the poison is removed (use-after-free).
8. *`&` that took, then trapped:* `(f &v)` with a take, callee mutates `v` then traps; the caller reads `v` after catch: the callee's last state, no double free. Kills: no write-back.
9. *exclusive window live:* `with-view` over an array, the body traps, the catch reads the array: window gone, array whole. Kills: no view return on unwind.
10. *swap! whose f traps:* atom unchanged, snapshot and `f` released (`audit: clean`); a second `swap!` works (no lock left held).
11. *uncatchable:* OOM (case 195 shape) and a failed thread start abort inside a `try`.
12. *happy-path guard:* `scripts/bench/quick.sh` shows no regression over 10 % and an unchanged checksum (CLAUDE.md "Performance cycle"); code size of the shootout programs recorded.
13. *a thousand throws:* a loop that throws and catches 1,000,000 times with live objects: `maxrss` flat (no leak), and the cost per throw recorded (target near the 58 ns of the micro).

### 4.3 What the audit needs

- Compiled: the trace is unchanged for stage 2 (every `A` has an `F`), so `fibc cases` needs nothing; the verdict `audit: abandoned` for stage 1 only. A new trace line `K` at a task's death would let a harness attribute
  the abandoned objects; not needed for v1.
- The spec sentence "the objects live at the abort are not leaks" (types section 2.11, spec/types.md rule 5 of the audit text) becomes "at an *uncaught* abort or a fatal one".

### 4.4 What the interpreter port needs (docs/design/fibref-port.md)

The evaluator walks the ownership plan, so a throw is an outcome `Throw ExInfo` next to `Normal`; every step that has a scope exit already releases its owned set from the plan,
and the unwind path runs the **same** releases for the `unwind-live` set the checker emits (U2). The heap and its 17 audit errors do not change: a catch that frees correctly is clean, a catch that
leaves a moved-from slot is a use-after-free the interpreter reports (this is how tests 6 to 9 would fail: they are tests of the interpreter's audit, and the only place that sees stale pointers; the compiled run
cannot). The task schedule (lazy run-to-completion) needs a `Failed` task state and `try-join`. Port order: after the heap and audit core (F1), with the trap outcome in the evaluator's result type from the start so that adding `try` later does not re-plumb every builtin
(each builtin's trap becomes an `Err` outcome in any case: `trapped:` today).

## 5. The language server today (before any of this lands)

`fibc lsp` is a loop reading JSON-RPC messages and calling handlers (`compiler/lsp/*.fib`; on main `server.fib` is a 17-line stub, the real loop is in a branch in progress). A handler that traps
aborts the process, so no -32603 can be answered and the editor sees a dead server. fibber has no process API in `fib.os` (`lib/fib/os/process.fib` has `process-id`, `parent-process-id`, paths; docs/design/os.md
"leaves room for process spawning"), but `extern` plus `unsafe` reaches libc, as `proto/fork.fib` does with `fork`, `waitpid` and `_exit`; this ran.

**Design W1 (recommended): fork per query.** The parent keeps the framing, the document text table and the request ids. For each *query* request (hover, definition, completion,
document symbols, diagnostics on request):
1. flush stdout, `fork`;
2. the child runs the handler (which analyses the buffer, `lsp.analysis`), writes the framed response to fd 1 itself (the parent is blocked in `waitpid`, so there is no interleaving), `_exit(0)`;
3. stderr of the child goes to a pipe created before the fork (`pipe2`, `dup2`) so the parent can read `trap: MSG`;
4. the parent `waitpid`s: exit 0 means the child answered; a signal (SIGABRT from a trap, SIGSEGV from a stack overflow) means the parent answers
   `{"id":ID,"error":{"code":-32603,"message":"internal error: trap: MSG"}}` and logs the message; the server lives on.
Notifications that change state (`didOpen`, `didChange`, `didClose`) are applied in the parent; they are text edits (`lsp.text`) and must be kept trap-free (guard offsets), or applied to a copy in a child and the
new text sent back through a pipe. A request whose handler needs the *analysis cache* pays the analysis in each child (the child's cache dies with it); if analysis is the cost, the parent can hold the cache and
compute it **in a child** that writes the result back through a pipe in a simple framed form (more code; not designed here).
Costs, measured with `proto/fork.fib` (`fork`, child `_exit`, parent `waitpid`, 5,000 iterations; the heap is an `(array N 7)` that stays live):

```
96 KB heap : 0.43 s / 5000 =  86 us per fork+wait   (/usr/bin/time: 0.25 user 0.19 sys)
9.6 MB heap: 1.79 s / 5000 = 358 us                  (0.78 user 0.90 sys)
96 MB heap : 8.70 s / 5000 = 1.74 ms                 (0.28 user 8.32 sys, page-table copy)
```

A language server's heap of 10 to 100 MB gives 0.4 to 1.7 ms per query, negligible against analysis. Cautions: `fork` in a process with other threads is unsafe (do not start tasks or the executor pool in the server, or fork before they start);
do not hold the LLVM JIT session of the macro runner across a fork unless the child only reads it (on analysis the JIT is used to expand macros: untested); a child that runs long is still the editor's problem (add a timeout:
`waitpid` with `WNOHANG` and `kill`). Not measured: the real handler. The only measurement is the cost of the fork and wait.

**Design W2: supervised worker.** `fibc lsp` re-executes itself (`posix_spawn` of `executable-path` with `--worker`, pipes both ways); the supervisor forwards messages, and when the worker dies, answers -32603 for the in-flight id,
restarts the worker and replays `didOpen` for the open buffers (the supervisor keeps the texts). Process start is below 10 ms (`/usr/bin/time -f %e ./F --version` printed 0.00 s; a hello binary 0.00 s); the real cost is
re-analysing every open buffer after a crash. One more pipe hop per message. More code than W1 (a process API in fibber, framing in both directions) and it keeps analysis caches inside the worker; it is the right shape if
W1's per-query analysis is too slow. Both are discarded when stage 1/2 land: the handler runs in `try`, the server keeps its cache, no process.

**Design W3 (after stage 1, before stage 2):** run each handler in a `spawn`ed task and `try-join` it: 27 us a request, no fork, the cache stays in the process; each trap leaks what the handler owned (bounded: one request's
analysis), the share-marking of the closure's captures walks the reachable document data (cost proportional to what the handler captures: unmeasured) and F1's detach is required. Acceptable for a server that restarts daily, not for a long-lived one: that is stage 2.

## 6. What was not done

- Nothing in `compiler/`, `lib/`, `rt/`, `spec/` changed; the stage 1 patch is a text patch of emitted lIR in scratch (`proto/patch.py`), not a change of the runtime source.
- (a) was not built through lair: lIR cannot express it. The shootout numbers are from a text transformation of LLVM IR that I wrote (crude: one cleanup pad per function that calls a stub, tail-position calls left as calls) built with plain `opt`/`llc`, not lair's pipeline.
  A real (a) has per-site pads that keep values live. The +13 % on num-nbody is not explained.
- (b) was measured only on the chain micro and counted statically on the shootout programs (upper bound). I did not hand-edit a shootout program's lIR to return `{T, i1}`; the real hot-path cost of (b) on `quick.sh` is not known.
- The `array-get` null-check alternative (section 2) and the cost of the poison test on `@c` are not measured.
- The `with-view` unwind and `rt/vecbuild.lir`'s builders were not audited for unwind safety (section 2).
- A cleanup-only personality with `_Unwind_ForcedUnwind` (section 3.1) was not prototyped; only the C++ personality was.
- No ARM measurements of cost (only that ORC unwinds through JIT frames on the Mac); no measurement of the language server's real handler under fork.
- The 20,000-task runs (`spawn.fib`, `tt3`) ran with the address-space cap lifted, because the cap stops the program at about 2,000 live threads (finding F1); every other command ran under `ulimit -v 16000000`. At most two heavy jobs at a time.
- No spec text was amended, no case was added or changed; the proposed tests are not written.

## 7. Decisions for the owner

1. **Join type (4.1): J1** (`try-join` returns a `Result`; `join` and `@t` rethrow as a trap in the joiner; case 911's program changes to `try-join`) against J2.
2. **Mechanism: (b)**, with (a) held in reserve if the measured hot-path cost on `quick.sh` is worse than the micro. Reversal of types section 2.11 (the signature of U0) after the library is viable, as stdlib Q35 already says.
3. **Fatal list** (1.2): OOM, stack overflow, thread-start failures, `(trap)` stay uncatchable.
4. **The `&` write-back on unwind** (basic guarantee) as the language rule, and **poisoning** for `cell-update!`.
5. **Trap in `finally` while unwinding is fatal** in v1.
6. Do F1 (detach finished threads) now: it is a defect with or without exceptions.

## 8. Stage 1 as built (2026-10-05)

Built on the lead's decisions of 2026-10-05 (docs/design/decisions-2026-10-04.md, "Exceptions (§7)"): J1 (`try-join` returns the trap, `join` and `@t`
trap in the joiner), the fatal list of 1.2, and F1 fixed first. What exists, and where its cases are:

| Piece | Where | Cases |
|---|---|---|
| finished threads are detached; main's return waits on a count of live threads (F1) | `rt/thread.lir` (`fib.spawn`, `fib.thread-done`, `fib.join-all`) | 912, 913, 914; `scripts/mutant-spawn-leak.sh` |
| a trap on a spawned task's thread completes the task as failed (message in a new task slot, `slot-task-failure`), releases the thread's count and ends the thread | `rt/core.lir` (`fib.fail-task`, `fib.trap-bytes`, `fib.fatal-bytes`), `rt/thread.lir` (`fib.thread-enter`), `rt/task.lir` (`fib.task-failure`, `fib.raise-failed`) | 916, 917, 921, 922, 925 |
| fatal, never isolated: out of memory (3 sites), a thread or pool worker that cannot start (2 sites) | `fib.fatal-c` | 922 |
| builtin `task-failure`, the trapping `join`/`@t` | `compiler/emit/lower/threads.fib`, `compiler/types/builtins.fib` | 925, 916, 917 |
| `Trap`, `try-join` | `lib/fib/async.fib` | 911, 915, 919, 920, 923, 926 |
| `ExInfo`, `ex-info`, `ex-message`, `ex-data`, `ex-cause` | `lib/fib/ex.fib` | 924 |
| `audit: abandoned` with `leaks: N` | `compiler/driver/header.fib`, `verdict.fib` | 918, 923, 926; `compiler/tests/driver/cases-check.sh` |

Every case names the planted fault that kills it in its header; `scripts/mutant-task-trap.sh MODE` builds each mutant and shows the cases fail.

**Differences from the sketch above, and what is not done.** The task's failure is the message only: `Trap` has a `message` and no `kind` (kinds need the
emit sites of stage 2, 1.2). `ExInfo` is generic in its data (`(ExInfo d)`) since `Val` of fib.data does not exist. What a trapped task's frames owned
is abandoned, and so is the closure environment of a task that trapped (its captures): `try-join` of the same program reports 3 leaked objects for a
handler that captured a request string (case 926). An `async` task that traps on a pool worker is not isolated (the worker has no task of its own):
the process aborts as before. `pthread_exit` unwinds the task's frames with the C++ forced-unwind machinery, so a static binary needs `libgcc_s`.
`try`/`catch`/`finally`, `throw`, unwinding, may-throw inference, `future-cancel`: not started (stage 2).

### Isolating a handler with `try-join`

A server that must survive a bad request runs each handler in a task and asks with `try-join`; the trap becomes the reply, and the loop goes on
(case 926 is this program):

```
(ns main (:use fib.core fib.async))

(defun handle (method: str n: i64) -> i64
  (cond (= method "double") (* n 2)
        (= method "divide") (quot 100 n)
        :else (trap (str "unknown method " method))))

(defun reply (method: str n: i64) -> str
  (match (try-join (spawn (fn () (handle method n))))
    ((Ok v) (str "result " (show v)))
    ((Err trap) (str "error -32603 " (. trap message)))))
```

`(reply "divide" 0)` is `error -32603 integer / by zero`; the next `(reply "double" 4)` is `result 8`. Cost: one thread per request (about 27 us); each trap
abandons what that handler owned, so this is for a server that restarts daily, not for one that traps in a loop (W3 above); stage 2 replaces it.

## 9. Stage 2 as built (2026-10-06)

Built on the lead's decisions of 2026-10-05 (mechanism (b), the fatal list, `&` write-back, `cell-update!` poisoning, trap-free `array-take!`
regions, a trap in a `finally` while unwinding fatal) and recorded as docs/adr/0009. Cases `stdlib/7870` to `7891`; planted faults
`scripts/mutant-catch.sh MODE` (ten modes, each kills the cases its header names).

| Piece | Where |
|---|---|
| the convention: in a program that calls `catch-run`, every body, closure code and vtable function returns `{ T ptr }` (`ptr` for unit); a result of two leaves or a SIMD vector keeps its type and takes a failure slot `exs` as a last parameter (`{ {i1,i64}, ptr }` has three leaves, which LLVM returns in memory, and no tail call can: found by compiling every ownership case with `FIB_CATCH=all`) | `compiler/emit/ir.fib` (`fb-ret-text`, `fb-slot?`, `unwind-ret`), `compiler/emit/lower/call.fib` (`callee-unwinds?`, `call-checked`, `fresh-slot`) |
| the may-throw decision: per **program** (`emit.program` `program-unwinds?`); a program that never calls `catch-run` has no observer of a failure, so its code is byte-identical to before (12 bench programs compared, user code) and a trap aborts as before | `compiler/emit/program.fib` |
| what a frame releases when a call fails: the ownership plan's, recorded by the final walk at every call and `match` (`w-put-unwind`: the scopes' owning sites and the owned parameters the path has not handed over, which is what a tail call's jump releases), kept in `guard-fail` under the key `-1 - e`; plus the arguments evaluated and not yet handed over (`pending`: owned temporaries, retained and stolen arguments, which the plan makes temporaries only after every argument is walked); plus the write-back of the call's `&` arguments | `compiler/own/walk/state.fib`, `call.fib`, `pattern.fib`; `compiler/emit/lower/call.fib` (`hold`, `lcx-unwind-exit`, `unwind-releases`) |
| trap sites: `trap`, index, overflow, zero divisor, `array-pop!` of nothing, `array-uninit`, a `match` with no clause, `join` of a failed task, `throw-object` raise the failure (`fib.raise-*`) and unwind where the plan says what the frame holds; elsewhere the trap as before | `compiler/emit/lower/cx.fib` (`lcx-fail`, `lcx-catchable?`), `builtins.fib`, `ctrl.fib`, `threads.fib` |
| runtime: the thread's catch state (a pthread key: depth, the failure caught last, finally depth); a trap unwinds only while a catch is active on its thread; `catch-enter/leave`, `caught-take`, `raise-bytes/-c/-index/-object`, `unwound-fatal`, `finally-enter/leave`, `fatal-finally` | `rt/core.lir` |
| fatal where a failure cannot be passed on: a `def` initialiser and an async body (they are called by code that does not test), the function of `cell-update!` (`fatal-while`), a function that calls `array-take!` (`takes-slots?`) | `compiler/emit/lower/mod.fib`, `cells.fib` |
| the library: `throw`, `try` (`catch`, `catch .. :when`, `finally`), `try-result`, `ex`, `ex-assoc`, `ex-kind`; `transact` of fib.db rolls back on a failure | `lib/fib/ex.fib`, `lib/fib/db/core.fib` |

**Differences from 4.2, and why.** `try` is a library macro over the builtin `catch-run` and a closure, not a core form: the expander is
another agent's, and a closure gives rules (iv) (no `recur` out of the body: case 7889) and the `await` rule for free. There is one exception
type, `(ExInfo (Map keyword Datum))` (fib.datum, not the unbuilt `Val`); `(catch T e ..)` is `(catch e :when cond ..)`. The may-throw effect
is a per-program switch, not a per-function inference: inside a catching program every function uses the convention (the inference would
let leaves keep the plain one; not built). Poisoning (`cell-reset!`) and the trap-free-region checker are not built: both regions are fatal
instead, which is sound and loses only catchability there. `with-view` needs nothing: a window is a base and a length into an array that never
leaves its place (case 7879). Async bodies do not pass failures on (a trap in an `async` body run under a joiner's `try` is the trap it was).

**Measured** (this machine, shared; `FIB_CATCH=all` compiles any program as one that catches):

- Every ownership case passes compiled as a catching program (354 of 354, verdicts and audits included); stdlib 1294 pass, 20 open (as
  without), 2 fail: 1707 fails on main too (`scripts/ci-stage2.expected`), 6221 failed with a vector result before vectors took the slot
  (fixed).
- Happy-path cost of a catching program, quick bench plus recursion and binary-trees, median of 5 under `/tmp/fibsuite.lock`
  (`BENCH_FIBC=F3 scripts/bench/quick.sh -n 5 ..`, then the same with `FIB_CATCH=all`), seconds plain -> catching: num-f64 1.70 -> 1.62,
  num-nbody 0.71 -> 0.69, vec-conj-pop 1.01 -> 0.98, vec-index 0.44 -> 0.42, vec-sort 0.48 -> 0.53 (+10 %), map-assoc-get 0.29 -> 0.33 (+14 %),
  set-conj 0.24 -> 0.26 (+8 %), lazy-fused 0.92 -> 0.99 (+8 %), lazy-bound 0.04 -> 0.04, strings 0.71 -> 0.81 (+14 %), recursion 2.28 -> 2.41
  (+6 %), binary-trees 1.46 -> 1.66 (+14 %); no checksum changed. Between -5 % and +14 %: the 0 to 13 % range section 3.1 measured for (a), not the
  "within noise" of the micro; the plain run itself was 10 to 35 % off the recorded baseline for identical code, so the noise is of that size.
  A program that does not catch pays nothing (identical user code).
- Compile time: `emit compiler/fibc.fib`, median of 3: 17.00 s (main) and 17.01 s (this tree), once the unwinding releases are recorded by
  the final walk only (recording them in every walk cost 14 %).
- Caught failures (test 13): `scripts/bench/throw-catch.fib`, a million iterations each owning a vector and a string three frames deep,
  750,000 of them failing (500,000 throws, 250,000 index traps) and caught in the loop: 0.25 0.25 0.25 0.27 0.29 s (5 runs, under the suite
  lock; start-up 0.00 s), about 330 ns per caught failure including the loop's own work and the exception's allocation; maximum resident set
  1.5 MB at a million and at ten million iterations (2.58 s): nothing grows. The audit of the 1,000-iteration run: 12,007 allocations, 12,007
  frees. Native throws of 3.1 (58 ns through ten frames) were a micro without the exception object.
- The JS backend (U6) needed no change: it sees lIR only. `compiler/tests/js/check.sh` with this tree's stage 2: 8121 runtime checks, 441 pass,
  the listed differ/unsupported only; catching programs emitted with `fibc emit` run under node with the native answers (t1 53, the README's
  example -1 and its `done`); a program with a spawned task stays unsupported there (no `pthread_create`).
- Async: a trap in an `async` body joined inside a `try` is the trap it was (`trap: nth: index out of range`, exit 134); a `try` inside a
  spawned task's function works as anywhere (its thread has its own catch state).
