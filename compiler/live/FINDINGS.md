# The live session prototype: findings

A measured prototype of the hardest mechanism behind a Clojure-style live environment (docs/design/live-repl.md, written by LIVE-1):
a resident JIT session that keeps compiled modules and values alive across inputs and replaces one function while other threads
call it. Code: `compiler/live/` (the mechanism `orc.fib jit.fib stub.fib swap.fib`, the load scenario `load.fib`, the fibber-level
session `extract.fib session.fib`) and the tool `compiler/live.fib`; the checks `compiler/tests/live/run.sh`. Two small hooks in the
emitter (`emit.program/program-new-with`, `emit.compile/compile-executable-ast-with`), both marked PROTOTYPE, both leaving the existing
entry points unchanged. Everything below was run on 2026-10-14 on lilnas (28 cores, shared) with stage 2 built from this tree by the
seed v0.1.16; the commands are quoted so that the numbers can be made again. The logs named below are under
`/tank/data/fibber-scratch/live2/`.

```
scripts/fetch-seed.sh /tank/data/fibber-scratch/live2/seed
$SEED build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F        # 25 s (42 s with the emitter hooks)
F build compiler/live.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o live          # 32 s
FIBC=F compiler/tests/live/run.sh                                                                  # every check below; 3 min
```

## The mechanism chosen: an indirection through one atomic pointer, with resource trackers for the old code

A live function `NAME` is compiled under a versioned symbol `NAME.vN`, one ORC module under its own resource tracker per version,
and every caller calls the stub `NAME` (live.stub), a function of the same type and convention that loads the current version's address
from `NAME.slot` (one aligned pointer, seq_cst) and calls it. A swap (live.swap `publish`) adds the new module, stores its address, and
retires the old module once no call can be inside it: the stub counts calls in flight per epoch parity (two counters per thread shard,
each on a cache line of its own); the swap bumps the epoch and spins until every counter of the old parity is zero (the grace period of
an RCU), then `LLVMOrcResourceTrackerRemove` frees the old code.

Why this and not replacement by resource tracker alone. ORC can remove a module and add another that defines the same symbol, but the
callers already compiled hold the old address: there is no way to re-bind a direct call in code ORC has already emitted without an
indirection, and ORC's own lazy re-exports/stubs (`LLVMOrcLazyReexports`) are not bound in compiler/llvm and would be the same
mechanism with a less visible quiescence story. So: the indirection is for the callers, the trackers are for the memory. Both are needed.

What it costs, measured (`live load`, 8 threads, 1000 swaps, body of 16 dependent multiplies; `matrix-sharded.log`):

| | calls/s (8 threads) | ns per call per thread | swap |
|---|---|---|---|
| direct call of the body, no stub (`--direct`) | 797 M | 10 | |
| through the stub, ONE shared in-flight counter (first version) | 25.7 M | 311 | 312 us |
| through the stub, counters sharded by thread (16 shards), -O 0 | 269 M | 30 | 306 us |
| the same at -O 2 | 372 M | 21 | 1295 us |

The first stub cost 300 ns a call: eight threads bouncing one cache line between the increment and the decrement. Sharding the
counters by `pthread_self` brought it to 20 ns over a direct call (two uncontended atomic read-modify-writes plus `pthread_self` and the
indirect call). A swap costs 0.3 ms at -O 0 and 1.3 ms at -O 2: lowering, LLVM optimisation and code generation of a one-function module,
the ORC add, one lookup, the grace period (84 spins over 1000 swaps: the callees are short), the remove. The grace period itself is not
measurable at this body length; with `--work 100000` (100 us calls) the swap waits for the calls in flight and no longer.

## (1) Resident session: values persist

`compiler/tests/live/session.in` (its transcript, exact, is `session.transcript`; the check diffs them):

```
fib> (def counter: (Atom i64) (atom 0))
; defined counter
fib> (defun handler (x: i64) -> i64 (+ x (swap! counter inc)))
; defined handler
fib> (handler 10)
; session started
11
fib> (handler 10)
12
fib> @counter
2
```

PASS. The same inputs in `fibc repl` print `11`, `11` and `0`: every run is a fresh program and the `def` is evaluated again
(`probe/repl-cost.3.out`). How: the first expression boots the session: the program of the session's definitions with `main` = `0` is
emitted once, prepared (live.extract `prepare-base`: every definition exported, the type tables widened to mutable arrays of 2048 rows)
and added to one LLJIT for good; `fib.init`, `fib.set-args` and `fib.defs-init` are called once, so the `def` slots (`@def.slot.N`,
filled by `fib.init.mNN.K`) hold their objects for the life of the process. A later input cuts out of its own whole program only the
expression's `main` (renamed `live.expr.N.body`, entered through a `ccc` wrapper), with a `declare` for every name the base exports and
a copy of every internal it reaches that the base does not have.

## (2) Function-body swap: callers compiled earlier see the new body

```
fib> (defun handler (x: i64) -> i64 (- x (swap! counter inc)))
; redefined handler (swapped f.handler)
fib> (handler 10)
7
fib> @counter
3
```

PASS: the atom kept its value across the swap and the new body ran on the next call. `(defun twice ..)` defined later, calling
`handler` twice, prints `-11` then `-15` across inputs (the transcript). The closure form of a function used as a value (`f.handler.owned`,
made when a program makes a closure of it) is a second live function with its own stub: `(prn (mapv handler [1 2 3]))` prints `[2 4 6]`
and, after `(defun handler (x: i64) -> i64 (* x 3))`, `[3 6 9]` (`values.transcript`).

The swap happens at the definition input, not at the next expression: every program the session emits is asked for the direct bodies of
all live functions (`compile-module-with` with `requests`, which also keeps the emitter's prune from dropping what `main` does not reach).
The first version of this prototype swapped lazily, and the under-load check caught it: 40 "redefinitions" that changed nothing
(`run-sh2.log`: `; defined handler` 40 times, `; bad 0` trivially). One thing stays lazy: the all-owned closure body cannot be asked for
(`unsupported: no plan for the body`: its ownership plan exists only in a program that makes a closure of the function), so a resident
closure of a redefined function keeps the old body until the next program that makes such a closure. A limitation to record in the design.

## (3) Safety: refused with a diagnostic; a failed check leaves the session unchanged; a trap does not kill the session

A signature change with a textual dependent is refused by the whole-program checker (serve.replcore, unchanged), with its diagnostic:

```
fib> (defun handler (x: i64) -> str "no")
<session>:4:31: no implementation of Num for str [type/NoInstance] (in the definition of twice)
; refused; the session is unchanged
fib> (twice 1)
-15
```

Without a textual dependent the whole-program check passes (nothing in the text calls `handler`), and the resident callers are what
would break: the session compares the checker's type of every live function before and after the definition (`rc/type-of-expr`, the
same table `:type` reads) and refuses before the text changes; the lIR type of the emitted body against the stub's is the last line
(`live.swap add-version`), the one the `skip-check` fault removes:

```
fib> (defun handler (x: i64) -> str "no")
live: handler is now ∀ς0. (fn :send () (fn ς0 (i64) str)) but the resident callers were compiled against ∀ς0. (fn :send () (fn ς0 (i64) i64)); refused
; refused; the session is unchanged
fib> (handler 10)
6
```

A trap in the evaluated code:

```
fib> (nth [1 2] 5)
trap: nth: index out of range
; status 134
fib> (handler 10)
5
```

How: every expression runs as `(live-guard (fn () ..))`, the session's first definition, `(try (f) (catch e ..))`; a program that uses
`try` is compiled under the catching convention (stage 2 of docs/design/exceptions.md: every function returns a failure next to its value),
the trap unwinds to the guard and the session goes on in the same process. The price: the catching ABI is the session's ABI (`f.handler`
returns `{ i64, ptr }`), which is why the type texts above read so. What this does NOT cover: a trap on another thread (a worker spawned in
the session) or inside a `finally` is fatal, as in any program; the fibber-level checks do not test it (the lIR-level load has no traps).
A trap in the middle of a swap leaves nothing half-done: the swap runs on the session's thread and the expression's code does not run
during it.

A `def` after the session started is refused (`live: a def added after the session started would not be resident (its slot is made
at boot); refused`): its slot would be made in the input's module and its initialiser never run. A redefinition of a `def` is not
refused and is not applied (the slot keeps its object; the new text is only text): the design must say what `def` again means.

## (4) Under load

The lIR level (`live load --threads 8 --swaps 1000`, in `run.sh`): 8 pthreads call `handler` through its stub without pause while this
thread swaps it 1000 times between two bodies that answer `x*1000+1` and `x*1000+2`; each thread counts calls, answers of neither
body (a torn call, a wrong ABI, a stale pointer that still runs) and tag changes seen.

```
load: threads 8, swaps 1000, work 16, opt 0
swaps: 1000 in 306 ms, mean 306 us each; grace periods waited 1001, spins 84
calls: 82250577 by 8 threads, bad 0, tag changes seen 8008, 268792735 calls/s
load PASS
```

PASS at -O 0 and -O 2 (`run-sh4.log`: 87.5 M and 601 M calls, bad 0, 8008 changes = 8 threads x 1001 publications). "No torn call"
holds by construction: a call loads one aligned pointer once; the check would count a torn answer as `bad`. "No use of freed code"
holds because the old module is removed after the grace period; the planted faults below show what happens without it.

The fibber level (`gen-load.py 8 40`, in `run.sh`): 8 workers spawned IN the session (`(spawn (fn () (worker)))`, resident tasks in an
atom) call `handler` in a loop and count answers of neither body, while the main thread redefines `handler` 40 times (each a whole-program
check, an emit and a swap); then the workers are stopped and joined:

```
; calls 3719115741
; bad 0
```

PASS (`run-sh4.log`). Pool tasks (`fork-task`, `with-tasks`) were not tried: `spawn` gave the threads the check needs; nothing in the
mechanism is specific to the thread's kind.

ThreadSanitizer: not practical, tried (`tsan-try.log`). `scripts/tsan.sh compiler/live.fib ..` fails at the instrumented link
(`undefined reference to LLVMBuildExtractValue` and the rest of LLVM-C: the script links no LLVM), and what matters is not reachable
that way: the accesses that could race (the slot, the epoch, the counters, the bodies) are in code the session JITs at run time, which
no instrumentation of the tool's own text touches. A TSAN of this mechanism needs the stub and the swap helpers compiled as
instrumented text (they are lIR: `live.stub` could be emitted into a program and run under `scripts/tsan.sh`), which was not done.
The argument for the memory order is written at the head of live.stub; it is an argument, not a proof (CLAUDE.md: TSAN does not see
ordering bugs among atomics either).

## (5) Cost

Per input, the session (`live run compiler/tests/live/session.in`, quiet, -O 0; the `; N ms` lines of the transcript):

| input | ms | what it is |
|---|---|---|
| `(def counter ..)`, `(defun handler ..)` before the boot | 24, 22 | the whole-program check only |
| `(handler 10)`, the first expression | 175 | the boot (emit the base, prepare, JIT it: about 120) and the expression |
| `(handler 10)`, `@counter`, later expressions | 47-85 | check (12-20) + emit (about 25) + cut and JIT the expression's module + run |
| `(defun handler ..)` that swaps | 132-151 | check + emit with the live bodies + cut, JIT and swap the version |
| a refused definition | 25-73 | the check, or the type comparison (one more front end) |

`fibc repl` on the same inputs (`repl-cost.sh`, 3 runs): 2368-2463 ms for 9 inputs; `:time` reports front end 12-19 ms and run
39-88 ms per expression (the child's JIT of the whole program and the fork), so the comparable figure is about 60-110 ms per expression
against the brief's 87 ms, and the live session's 47-85. The session's per-input floor is the whole-program front end (12-20 ms check
plus the emit of the whole program, about 25 ms), not the JIT: cutting out a module and adding it under a tracker is 1-3 ms; the swap
itself 0.3 ms (above). The REPL's `; the program trapped` input cost 1104 ms (the child's abort); the session's trap cost 59.

The fixed cost of a session: the library's expansion once (about 900 ms before the first input, the same as the REPL's engine), and
the base: the first expression's 175 ms.

## (6) Planted faults, each turning a check red

`run.sh` runs every fault; the output of the last run (`run-sh4.log`):

| fault (`--fault`) | what is left out or done wrong | the check that goes red |
|---|---|---|
| `free-first` | the old module is removed before the new address is published | `live load`: SIGSEGV at the freed page (`fatal signal 11 at 0x..f2000`), rc 139 |
| `no-wait` | published, then removed with no grace period | `live load --work 100000`: SIGSEGV inside the freed body (`fatal signal 11 at 0x..7032`, an address in the old code), rc 139 |
| `two-step` | the slot is written null, then the address: not one atomic store | `live load --threads 16 --swaps 5000 --work 1`: `fatal signal 11 at 0x0` (a call of address 0), rc 139 |
| `no-count` | the stub keeps no in-flight counters, the swap waits on zeros | `live load --work 100000`: SIGSEGV in the freed body, rc 139 |
| `skip-check` | a version whose lIR type differs from the stub's is accepted | `live load --mismatch`: `bad 46226928` of the answers (the two-parameter body reads a register), FAIL, rc 1 |
| `skip-check` (session) | the checker-type comparison is skipped | `live run session.in`: 8 lines of the transcript differ (the `-> str` body is accepted; `(handler 10)` prints the address of "no") |
| `skip-program-check` (session) | a `def` after boot is accepted | `live run session.in`: the transcript differs (`(+ late 1)` prints 1: the slot in the input's module is never initialised) |

Two of the faults are timing-dependent and the first run of the matrix missed them: `no-wait` and `two-step` PASSED at 8 threads x 1000
swaps with 16-multiply bodies (`matrix-work16.log`): the remove comes about 100 us after the slot store and no call of 15 ns was still
inside, and a 10 ns window between two stores was not hit in 1000 tries. With 100 us calls (`--work 100000`) and with 16 threads x 5000
swaps they are hit every time (the parameters `run.sh` uses). A check of a race with a planted fault proves the fault is reachable
under those parameters, no more; this is why the run of the real mechanism under the same parameters is the check that matters.

## What broke, and what had to change for it not to

1. **Per-program numbering of everything static** was the blocker, not the JIT. The emitter numbers type ids (`fib.types`,
   `fib.classes`, `fib.kinds`: a row per object type, read by `fib.release` and every walker for every object), keyword ids, string
   and closure statics (`str.N`, `clo.N`) and `def` slots in order of first use, per program, and writes them into the code as plain
   literals (`(call @fib.alloc (i64 32) (i32 4))`). Two programs that differ in one body disagree on the ids of every type requested after
   it; the row for `main`'s own lambda (`closure l.f.main.18881` against `18883`, probe 3) differs between any two inputs. An object
   allocated by one input's code with id 15 and released by the base through its row 15 would run the wrong drop. Nothing at the JIT
   level can fix this; renumbering the literals in the cut-out module is not possible (a tid is indistinguishable from any other `i32`).
   What fixed it: one `Objects` and one `Statics` for the whole session, given to every program (`program-new-with`): a type or static
   already numbered keeps its number, a new one is appended; the base's tables are mutable arrays with room, and an input that adds rows
   also adds a function that stores them (`live.types.append.N`) and is pinned. With that, new types, keywords, a `defrecord` defined after
   boot and strings stored in a resident atom all work (`values.transcript`). The rows of every input's lambdas accumulate in the session's
   table (one per expression), a leak the design must bound.
2. **The emitter is demand-driven from `main` and prunes to it.** A definition input's program reaches nothing, so nothing was swapped
   until an expression happened to call the function (found by the under-load check). `compile-module-with` takes the live functions'
   names, requests their bodies and does not prune. The all-owned closure body cannot be requested (above).
3. **Ownership of resident values.** The emitted `def` slots are run-time globals initialised once (`fib.init.mNN.K` at `fib.defs-init`),
   not compile-time constants, so a resident value is simply the slot's object, counted as in any program. What crosses an input's
   boundary: a value stored in a resident atom (strings, vectors, a closure, `values.transcript`) must not point into a module that is
   freed, so an expression module is freed only when it holds no static, no lambda and no table row (`live.extract/pins?`); otherwise it
   stays for the life of the session. A version of a function is freed on the same rule, so a body with a string literal (`str.N`) or a
   closure of a named function is never freed. The rule is conservative (a static that no resident value took is kept too) and the design
   wants an escape analysis or counting for statics; the prototype has neither. Nothing was observed to break under the rule.
4. **In-flight frames.** A thread inside the old body keeps running the old body to its return, by the grace period; a thread that
   loaded the slot before the store and calls after it runs the old body once more. Deeper frames (a function that called a live
   function and is still waiting for it) are unaffected: they return into their own code. What is not handled: a live function that is
   itself long-running (a server loop) never leaves the old version, so its module is never freed and the swap waits in `wait-quiescent`
   for as long as it runs. The wait has no timeout (the design needs one, and a way to say "the swap is published but the old code is
   still in use").
5. **The catching ABI.** Using the language's `try` to isolate traps makes the whole session a catching program (every function returns
   `{ T, ptr }`); a session that starts without a `try` would compile the base under the plain ABI and a later input's `try` would change
   every body's type. `live-guard` is defined first for this reason. The design must choose the ABI once.
6. **Thread-local and global runtime state** is the base's and shared by every later module (every `fib.*` global is declared, never
   copied: `fib.sched-*`, `fib.exc-key`, the trace state). It worked without attention in the prototype because the runtime is in the
   base once; an input that would need a runtime item the base does not have (the kernel table, a GPU window) would copy it as its own:
   not tested.
7. **Two emitter hooks** (`program-new-with`, `compile-module-with` with `tables` and `requests`) are the only changes outside
   `compiler/live`; the entry points everything else uses are unchanged and the REPL transcripts and the ownership cases pass with the
   rebuilt stage 2 (`compiler/tests/repl/run.sh F`: 5 transcripts and the tty tests ok; `F cases cases/ownership -j 8`: 375 pass).

## What was not measured or done

- The fixed point and the full gate (the lead batches them). The golden suites were not run.
- Pool tasks under load (`fork-task`), traps on worker threads, a swap of a function that is on another thread's stack for seconds.
- A session on macOS/aarch64 (`pthread_self` hashing and the ORC memory manager differ).
- Memory growth over thousands of inputs (pinned expression modules, table rows per lambda).
- Keyword tables: with one `Statics` the ids agree; the base's `kw.hash`/`kw.rank` functions and the keyword-name table are the base's
  and do not see keywords interned later. `(prn {:a 1 :b 2})` and `(get {..} :b)` work in `values.transcript`; a keyword first interned
  after the boot and then printed or ranked by the base's code was not tested.
