# A live REPL: change a running program without restarting it

Status: **design, 2026-10-14; nothing here is built.** The owner's rule: "Clojure for ergonomics, Rust for safety". The priority this
answers: an agent or a person runs an app (an HTTP server such as meshql-fib's `meshql serve`), changes a function while it runs, sees the
effect on the next request, and keeps values (an atom, a `def`) from one input to the next, like a Clojure REPL. Today (0.1.16) `fibc repl`
and `fibc serve eval` exist but nothing is resident: every input is a fresh program in a fresh process (dev-loop.md §9, §11.3).

**Measured** means a command was run in this session and its output is quoted (section 1). **Estimate** is arithmetic on measured numbers and
says so. Code read for this design: `compiler/native/{jit,api,call}.fib`, `jit/{names,prune,trampoline}.fib`, `compiler/llvm/orc.fib`,
`emit/defs/jit.fib`, `macros/runner.fib`, `serve/{incr,replcore,evalop,exec,server,sock,repl}.fib`, `driver/native.fib`, `emit/{names,program,
compile,statics}.fib`, `rt/{sched,task,thread,atom}.lir`, `spec/types.md` §2.10, §2.16, §8.4, §8.9, `spec/ownership.md`, dev-loop.md,
persistent-checker.md, parallelism.md §8, exceptions.md and ADR 0009.

Contents: 0 the short answer; 1 ground truth; 2 the model; 3 redefinition and safety; 4 what the checker needs; 5 transcripts; 6 package
plan; 7 open risks and the owner's decisions; 8 what is built, not built, measured.

## 0. The short answer

1. **A live session is `fibc run --live APP.fib`: the app runs inside the compiler's process, in its ORC session, and the same process
   holds the front end's engine (`serve.incr`, the persistent checker) and a unix socket an agent attaches to.** Nothing new has to carry
   LLVM; `fibc run` already JITs the program and calls `main` on the driver's thread (`driver.native/run-module`). A built executable
   (`fibc build`) is not live: it has no compiler in it. This is the one-process design; the alternatives are in 2.1.
2. **Redefinition goes through a patchable slot, not through re-linking callers.** In live mode every function of the program's own
   modules (never the library's) is emitted behind a stub: `f.main.handle` loads a pointer from `live.f.main.handle` and tail-jumps to it.
   A swap compiles the new body under a new name, adds it to the session under its own ORC resource tracker, and stores its address in the
   slot. Callers, the immortal closure a named function becomes when used as a value (the handler `run-server` was given at start-up),
   vtables: all go through the stub, so they follow the swap without being touched. Cost: one acquire-load and one indirect jump per call
   of a program function, live mode only; `fibc build` and plain `fibc run` emit nothing of this.
3. **Milestone 1 is function-body-only swaps, and the rule that makes them safe is "same signature, same ownership summary".** The new body
   must have the scheme of the old one and the same parameter modes (borrowed or owned), escape summary and taken-as-value facts the
   ownership pass printed for the old one: callers were compiled against those as a calling convention (ownership.md §4). Anything else,
   including any change to a type, a record layout, a protocol, an instance, a `def`'s type or a macro, is refused with the diagnostics
   `fibc check` prints plus a `live/*` code that names the rule and tells the user to `:restart`.
4. **Old code is never freed while the program might be running it.** A generation's resource tracker is removed only at a quiescent
   point: no spawned thread alive, every pool worker parked, the main thread parked in the attach server's pause. Until then every
   generation stays mapped (`:status` prints the bytes). Nothing a thread is executing or will return into can disappear.
5. **Values live in the app's heap, not in the session's text.** A `def` at the REPL is a run-time `def` of the session module (the
   `Init` mechanism of `emit.inits`): evaluated once, immortal, borrowed count-free by every later input, never dropped. An atom in it is
   the same atom every input sees. A redefinition of a `def` makes a new immortal value and leaks the old one by design (reported). An
   expression input runs on the attach thread, prints with `prn`, and its temporaries die at the end of the input. A trap in a live
   session is a caught failure, not an abort, because `--live` compiles the program as a catching one (ADR 0009).

## 1. Ground truth (measured, 2026-10-14)

Machine: lilnas, 28 threads, load average 2. Stage 2 `F` = `/tank/repos/tailoredshapes/fibber/build/F` (`fibc 0.1.15`, built 2026-10-13
from main), `FIB_LIB` the worktree's `lib/`, every command under `flock /tmp/fibsuite.lock`.

```
$ F repl --echo < repl.in
fib> (defun sq (x: i64) -> i64 (* x x))            ; defined sq
fib> (defun cube (x: i64) -> i64 (* x (sq x)))     ; defined cube
fib> :time (cube 3)                                27    ; front end 8 ms (52 modules, 1 expanded, 51 reused), run 37 ms
fib> :time (cube 4)                                64    ; front end 13 ms, run 35 ms
fib> (defun sq (x: i64) -> i64 (+ x 1))            ; redefined sq
fib> :time (cube 4)                                20    ; front end 9 ms, run 38 ms
fib> (def counter: (Atom i64) (atom 0))            ; defined counter
fib> :time (do (swap! counter inc) @counter)       1     ; front end 11 ms, run 53 ms
fib> :time (do (swap! counter inc) @counter)       1     ; front end 15 ms, run 41 ms
```

Two readings. The front end of an input is 8 to 15 ms (the persistent checker of 0.1.16 at work: the library is checked once), and the
"run" of 35 to 53 ms is a fork, the JIT of the whole program (runtime, tables, the library it reaches: 2,300 to 2,500 lines of lIR for a
program this size, dev-loop.md 2.1) and the run. And **the counter is 1 both times**: the value does not persist, the `def` is evaluated
again by every input (dev-loop.md §9). That is the gap this design closes.

```
$ FIB_TIME=1 F run -O 0 --time hello.fib        (three runs)
time front.expand 34 ms  front.lower 31  front.infer 10  front.elaborate 3  front.own 22  front 132 ms  lir.emit 11 ms
time front.expand 33 ms  front.lower 30  front.infer 10  front.elaborate 2  front.own 21  front 120 ms  lir.emit 10 ms
```

The JIT phases are not in the `--time` report (dev-loop.md 2.2 measured them with a scratch build: `add-source` 3 to 30 ms, machine code
5 to 172 ms from `hello` to a 176-line program at `-O 0`, before FastISel). **Not measured here**: the cost of adding one small module to
a live session and patching a slot. Estimate from 2.2 and 2.3: FastISel code generation is about 0.15 ms per source line, so a 15-line
function is 2 to 3 ms of code generation plus ORC's materialisation; the front end of the swap (section 4) dominates.

What exists that the design builds on (read, not run): `native.jit` keeps one LLJIT per session with `names` (every exported name so far)
and refuses a second definition of a name (`names-check`); `emit.defs.jit` and `macros.runner` already hold sessions across calls and read
JIT memory through raw pointers (`mem-i64`, `ffi/text-at`); `native.call` calls an address on the calling thread or on a thread with a chosen
stack; the compiler's own runtime and the JITted program's runtime coexist in one process (the macro runner does this on every build);
`compiler/llvm/orc.fib` binds `LLVMOrcLLJITAddLLVMIRModule` and `Lookup` but not the resource trackers, which LLVM 21's `llvm-c/Orc.h`
has (`LLVMOrcJITDylibCreateResourceTracker`, `LLVMOrcLLJITAddLLVMIRModuleWithRT`, `LLVMOrcResourceTrackerRemove`, `LLVMOrcAbsoluteSymbols`,
`LLVMOrcJITDylibDefine`: checked in `/usr/lib/llvm-21/include/llvm-c/`). The emitter names a body `f.MODULE.NAME` and its all-owned twin
`f.MODULE.NAME.owned`, a specialisation `f.MODULE.NAME.$TYPES_`, a closure literal `l.OWNER.N` (`emit.names`); a named function used as a
value is an `IMMORTAL` closure constant whose code is `f.owned` (types §8.4); `run-server` takes the handler as such a value
(`lib/fib/http/server/api.fib`: `(fn :send (Request) Response)`), which is why decision 2 matters. The pool has a quiescence routine
(`fib.sched-quiesce`, `fib.pool-quiesce` in `rt/task.lir`) and a count of live spawned threads (`fib.threads-live`, `rt/thread.lir`).

## 2. The model

### 2.1 A live session is a process

```
fibc run --live APP.fib [-- args]          the app's process = engine + ORC session + attach server + the app
   driver thread:   main(args) of the app, as fibc run calls it today (call-main)
   attach thread:   accept on the socket; one request at a time; runs the front end, the emitter, the JIT; patches slots; evaluates inputs
   app threads:     spawn threads and pool workers, as in any run
```

The session owns: the engine (`inc/Engine`, with the library's state kept by `own.staged`), the session text (`rc/Session`: clauses and
definitions, as the REPL has now), the JIT (`api/Jit`), a **live table** (per program function: its slot symbol, the current generation's
body names, every specialisation and closure literal emitted for it, its scheme and ownership summary as recorded at the last accepted
generation), the generations (each a resource tracker and the names it defined), and the session values (the run-time `def`s of the session
module, section 3.5).

Why one process and not the app attached to a `fibc serve`: the code has to run in the process that holds the app's heap and threads, and
only the compiler process has LLVM. Two alternatives were considered. (a) A built executable that `dlopen`s objects a `fibc serve` makes
on request: no LLVM in the app, but every program function still needs a stub, the app needs a loader and the engine's view of the
program must match the binary's build exactly (ADR 0024's liro cache would key it); it is the right shape for a production process and
is listed as a later package (LV8), not for the first. (b) An interpreter: rejected in dev-loop.md §3 and ADR 0005. **Decision: (one
process), with (a) as the follow-on once the swap mechanics exist.**

### 2.2 Opting in

- `fibc run --live APP.fib`, or `FIB_LIVE=1 fibc run APP.fib`: the program is compiled in live mode (stubs, catching, `-O 0`), the socket
  is made before `main` is called, its path is printed on standard error (`fibc: live at /run/user/1000/fib-live/48213.sock`) and in
  `$FIB_LIVE_SOCKET` for the app.
- `fibc serve`'s `run` request with `"live": true` does the same in the forked program process (serve.exec) and answers the socket path.
- A library call `(live/serve-repl)` **is not offered in v1**: a built executable has no compiler to serve one, and in `--live` the socket
  is already there. What the library gets is `fib.os.live`: `(live/socket)` (the path or nil), `(live/pause-point)` (section 3.3), so an
  app can say when it is quiescent. `fib.os.net` is TCP only (`lib/fib/os/net.fib`): the attach socket is the compiler's (serve.sock,
  outside ADR 0011 as the server's is).

### 2.3 The attach protocol

One JSON object per line, in and out, over the unix socket, one request at a time; the shapes of `fibc serve` (dev-loop.md 8.2: `id`,
`ok`, `error.code`, `diagnostics` in the 8.3 schema, `stats`). A connection may stay open for many requests (unlike `serve`), since the
session is one process and a REPL is a conversation.

| op | request | answer |
|---|---|---|
| `hello` | | `pid`, `program`, `generation`, `version`, `catching` |
| `eval` | `text` (what `fibc repl` takes) | definitions: `defined`, `swapped` (names whose bodies changed), `generation`, `stats` (`check_ms`, `emit_ms`, `jit_ms`, `patch_ms`); an expression: `value` (the `prn` text), `type`, `stdout`, `stderr`, `failed` (a caught trap: its message), `stats`; a refusal: `ok:false`, `diagnostics`, `shown`, `code` (`live/Signature`, `live/Summary`, `live/Layout`, `live/Protocol`, `live/Macro`, `live/Def`) |
| `type` | `expr` | `type` (as `:type`) |
| `inspect` | `name` | `value` (the `prn` of a session value or a program `def`), `type`, `generation` of a function, its bodies |
| `trace` | `name`, `on` (bool), `limit` | the function is swapped for a tracing twin (section 3.2); `trace` lines arrive as `{"event":"call","name","args","result","ms"}` on the connection that asked |
| `load` | `file` | the file's definitions as one `eval` (body swaps for the ones that changed, refusals for the rest) |
| `status` | | `generation`, `generations` (count, bytes kept), `threads` (spawned alive, pool workers, in-flight tasks), `values` (session defs, leaked bytes of redefined ones), `last_refusal` |
| `pause` / `resume` | | the app is held at its pause points (3.3) and the quiescent state reported |
| `retire` | | generations no thread can be in are removed (3.3); `removed`, `kept` |
| `restart` | | `main` is ended through `exit` and the client restarts the process with the same socket path (section 3.6) |

The errors that are not refusals: `not-live` (the program was not started with `--live`), `busy` (an input is being evaluated on the attach
thread; a second client waits), `bad-request`, `timeout` (an expression that ran longer than `timeout` seconds is **not** killed, since it
runs on a thread of the app's heap; the answer is `running:true` and the value arrives later as an event).

### 2.4 How an agent drives it

- `fibc repl --connect SOCK|PID` is the terminal client: the REPL of today (line editing, history, `:type`, `:doc`, `:load`, `:reload`,
  `:defs`) over the socket, plus `:status`, `:trace NAME`, `:untrace NAME`, `:inspect NAME`, `:pause`, `:resume`, `:retire`, `:restart`.
  Without `--connect` it is the REPL of today. `fibc repl --connect SOCK --json` is the stdio form: one request per line in, one answer per
  line out, for an agent or an editor (the controller of dev-loop.md 4.4).
- `fibc serve eval` with `"live": SOCK` forwards the request to that session, so an agent already talking to a server has one socket.
- `fibc watch --live SOCK DIR`: the poll of dev-loop.md 4.2, sending `load` for each changed file of the program (the modules of the
  program are known to the engine), so an editor save is a swap.

### 2.5 Security

The socket is `$XDG_RUNTIME_DIR/fib-live/PID.sock`, else `/tmp/fib-live-$USER/PID.sock`, the directory 0700 and the socket 0600
(`serve.sock/chmod-private`, as `fibc serve`); a client whose peer uid (`SO_PEERCRED` on Linux, `getpeereid` on the BSDs and macOS) is not
the process's is closed before a line is read; **no TCP, no network, by default or by flag in v1.** A session executes arbitrary code in the
app's process with the app's credentials: attaching is the same power as running the app, which is the reason for the uid check and for
refusing anything a container's network could reach. `FIB_LIVE=0` or no `--live` is the production default: nothing listens. For a pod with
no shared runtime directory the owner must decide (section 7) between a token on TCP loopback and `kubectl exec` of the client in the pod;
the design recommends the second.

## 3. Redefinition: semantics and safety

### 3.1 What a swap is

Live-mode emission, for every function `f` of a program module (library bodies stay direct calls: they are not swappable, and they are
most of the code):

```
(global live.f.main.handle ptr @f.main.handle.g0)                       ; the slot, generation 0
(define (f.main.handle R) ((A1 p1) .. (An pn))                           ; the stub: what every caller, vtable and closure constant names
  (block entry (tailcall (atomic-load acquire ptr @live.f.main.handle) p1 .. pn)))      ; an indirect-tailcall through the loaded address
(define internal (f.main.handle.g0 R) ((A1 p1) .. (An pn)) ..)           ; the body as the emitter makes it today, renamed
```

The same for `f.owned` (the all-owned body a closure value jumps into), every specialisation `f.main.handle.$T_` the program instantiated,
and every closure literal `l.f.main.handle.N` of the body (a literal's code belongs to the generation of the body that made it: the new
body makes new literals with new names; old closure objects on the heap keep pointing at old code, which stays mapped, 3.3). A swap of `f`:

1. The session text gets the new definition (`rc/merge-defs`); the program (every module, the session module last) is checked by the
   engine: the persistent checker's library state, the program's modules checked as now (section 4 says what must change for this to be
   fast).
2. The rule of 3.4 is applied: `f`'s scheme and ownership summary before and after must be equal; nothing else of the program may have
   changed (the diff of the session's definitions is exactly `{f}`). Otherwise refuse, session unchanged, nothing emitted.
3. The emitter is run for `f` only: the body, its all-owned twin, the specialisations the live table lists for `f` (the ones the running
   program has; a new specialisation can appear only from a new caller, which is itself a swap), its closure literals, under names
   `...gK`. The module declares (`declare`) everything else it calls, by the stubs' names, so the names rules of spec/lir.md §11 hold
   (`names-check`: a declaration matches the earlier definition).
4. The module is added to the session under a fresh resource tracker (`LLVMOrcLLJITAddLLVMIRModuleWithRT`); the addresses of the new
   bodies are looked up; the slots are stored with release order (one `atomic-store release` per slot, from a tiny lIR function
   `live.patch` the session defines once: fibber's `store-i64` is a plain store and the stub's load is an acquire load, so the pair is
   correct on aarch64 as well as x86). The old generation's tracker is kept (3.3). `swapped: ["handle"]`, `generation: K`.

Data: a slot is a pointer, the patch is one store, the ordering is acquire/release: a caller that loads the new address sees the code ORC
finished writing before the lookup returned (ORC's materialisation completes, with its memory made executable, before `Lookup` answers).
Two swaps are serialised by the attach thread. A caller in the middle of the stub (loaded the old address, not yet jumped) jumps into old
code, which is mapped: it runs the old body to completion. That is the whole in-flight story for callers; the next call sees the new body.

Why not "replace the module and relink the callers" (the resource-tracker replacement dev-loop.md 4.4 sketched): the callers that hold the
address are not only call sites. The handler the server loop holds is a closure constant whose code field was copied when `run-server` was
called; a vtable holds method addresses; a `Task`'s resume function is an address in the task. Rewriting all of these in the heap is a
walk the runtime cannot do safely while threads run. The stub makes every holder indirect at the one place a holder is made, so a swap is
one store. The price, one load and one jump per call of a program function, is paid only in live mode and only by the program's own
functions; LLVM at `-O 0` would not inline them anyway.

### 3.2 What a body swap covers, and what it does not

Covered by milestone 1 (LV2): the body of a `defun`/`defn` of a program module, including its closure literals, loops, `let`s, local `fn`s;
a method body of an `impl` whose instance already exists (the method's stub is `m.PROTO.NAME.$T_`, the same mechanism); a `defmacro` body
is **not** (a macro ran at expansion time; its users would need re-expansion: refused, `live/Macro`, until LV5 reloads a module).
Tracing is a swap: `trace handle` emits a generation whose body is `(let ((r (f.g{K-1} args..))) (live/note "handle" (show args..) (show r)) r)`,
calling the previous generation's body directly (not the stub), and `untrace` patches the slot back to generation K-1's address; a trace
survives a later swap of the same function by re-wrapping. A function the program never instantiated (dead, or generic and unused) swaps
into the session text only; nothing is emitted.

### 3.3 In-flight calls, threads, pool tasks, and when code is freed

- **In flight on any thread**: the thread is in the old body, with the old body's return addresses on its stack, possibly inside old closure
  literals, possibly parked in a pool join inside the old body. It finishes there. Old code must stay mapped and so must every old closure
  literal, since heap closures made by the old body point at them; old generations are never removed while a thread may be in them.
- **Pool tasks**: a task forked before the swap holds a closure object whose code is a literal of the old body: it runs old code when a
  worker takes it, which is right (the task is a value made by the old body). A task forked after the swap, by any caller, goes through the
  stub to new code. `fib.sched-active`, the deques and `fib.sched-quiesce` are untouched.
- **Spawned threads** (`spawn`, `future`, `run-server`'s accept loop): the same; the thread's frames are wherever they are.
- **Quiescence, defined**: every pool worker parked with no work anywhere (`fib.sched-quiesce` is exactly this, `rt/sched.lir`), no spawned
  thread alive (`fib.threads-live` = 0), the driver thread (the app's `main`) parked at a pause point, the attach thread the only runner. At
  quiescence no frame of any thread is in program code except the pause point's, so every generation but the current and the one the pause
  point's frame is in can be removed. A **pause point** is `(live/pause-point)`: the app calls it where it is idle (the accept loop between
  connections; the library's `run-server` calls it once per accepted connection when `FIB_LIVE_SOCKET` is set); `pause` makes the next call
  block until `resume`. A server with a connection in flight is not quiescent until the handler returns; `pause` waits for that with a
  timeout and reports what is still running (`status.threads`).
- **Retirement**: `retire` (also `:retire`) at quiescence removes every tracker of generations no slot points to and no frame can be in:
  `LLVMOrcResourceTrackerRemove`. Not at quiescence it refuses (`not-quiescent`, with the threads). Nothing automatic in v1; `status` prints
  what is kept. **Estimate**: a swapped 50-line function is 5 to 20 KB of machine code and tables; a thousand swaps are under 20 MB.
- **Stack scanning** (frame pointers at `-O 0` and a walk of every thread's stack, mapping return addresses to generations through ORC's
  symbols) would make retirement possible without a pause. It needs dev-loop.md's DV9 (trap stacks) and a way to stop every thread (a
  signal and a handshake). Not in this plan; listed as the way to lift the pause requirement later (section 7).

### 3.4 What is refused, and how

A swap is accepted only if the whole program still checks **and** the change is confined to the bodies the live table can patch. The
check runs the ordinary front end and prints the ordinary diagnostics (`type/*`, `own/*`, `expand/*`, the 8.3 schema, the `<session>` and
`<input>` forms of `serve.replcore/show-diag`); then the live rules, each one diagnostic with a `live/*` code, the position of the input's
definition, and a `hint` field that says what to do:

| rule | refused when | code, message |
|---|---|---|
| signature | the scheme of `f` after differs from before (the catalogue's scheme, compared as types with colours erased, not as text) | `live/Signature`: `handle now has type (fn (Request i64) Response), was (fn (Request) Response); callers were compiled against the old one. :restart to change a signature` |
| summary | a parameter's mode (borrowed/owned), the escape summary, the `Derived` result facts or the taken-as-value set of `f` or of any function of its SCC differs after (own.top's summaries: what `fibc explain` prints) | `live/Summary`: `handle's parameter req was borrowed and is now owned (it is stored at <input>:3:14); callers pass it borrowed. Keep the body from storing it, or :restart` |
| layout | the diff of the session touches a `defrecord`, `defstruct`, `defenum`, `derive`, a field type, a variant | `live/Layout`: `Pt changed; values of Pt exist in the running program. :restart` |
| protocol | a `defprotocol` or `impl` added, removed or changed, including a new instance (its vtable and dispatch rows are in every caller) | `live/Protocol` |
| def | a `def`'s type changed, or a program `def` (not a session one) is redefined | `live/Def`: a program `def` is immortal and read by every function count-free; the session may add defs, not change the program's (3.5) |
| macro | a `defmacro` changed, or a swapped body uses a macro that changed | `live/Macro` (until LV5: reload the module) |
| extern | an `extern` changed | `live/Extern` |
| SCC | the swap changes which functions `f` calls such that its SCC's all-owned bodies change membership | handled by swapping every member (the SCC is in the live table); refused as `live/Summary` only if a member's summary changes |
| main | `main` redefined | `live/Main`: `:restart` |
| type/ownership | the program does not check (an error in the new body or in a dependent) | the checker's own diagnostics, as the REPL today; nothing swapped |

`fibc check` parity: the refusal's `diagnostics` array is the 8.3 schema, `fibc repl --connect --json` prints them as `fibc --server check`
does, and the `live/*` rows carry `hint`. A refusal changes nothing: not the session text, not the JIT, not a slot (the order in 3.1 makes
that true: nothing is emitted before every rule has passed). A **partial** `load` of a file (some functions swappable, one refused) is
refused as a whole, with every refusal listed: a file is one unit.

The reason the rules are this strict, in one sentence each. Signature: callers' calls are typed lIR. Summary: an owned position is a
count the caller handed over (ownership.md §4): a borrowed body would leak it, an owned body under a borrowed caller would free the caller's
value. Layout: every object of the type in the heap, and every body that reads a field by offset, was compiled to the old layout; migrating
values is the Erlang `code_change` problem and is not attempted. Protocol: dispatch rows and vtables are constants in every caller.
`def`: a program `def` is `IMMORTAL` and read as a count-free borrow by every function; replacing the object under them is a use-after-free
risk the checker cannot see. All of these lift with a restart, which keeps the session text and the transcript, so an agent loses no
work, only the heap.

### 3.5 Values: ownership in a live session

- **A session `def` is a run-time `def` of the session module** (`(def counter: (Atom i64) (atom 0))`): its initialiser runs once, on the
  attach thread, when the definition is accepted; the value is made immortal as a program `def`'s is (types §2.16, `emit.inits`: `fib.immortalise`,
  atoms inside registered in `fib.defatoms`). Later inputs name it as a global: a count-free `Borrowed(g)` read, exactly as a program `def`.
  A `def` may hold an `Atom` and not a `Cell` or a `Weak` (the rule stands: a session value is reachable from every thread).
- **Who owns it**: the session; it is never dropped. **Borrowed by later inputs**: free, by the checker's existing rule. **Consumed**: an
  input that needs to own it clones (`(clone @counter)`, `(into [] xs)`): a `def` cannot be moved from, as today. **Dropped**: never; a
  redefinition `(def counter ...)` makes a new immortal value and the old one stays allocated (an immortal object is never freed: types
  §2.10.1 says the same of `freeze`). `status.values.leaked_bytes` counts it. This is Decided-by-design in this document, pending the owner
  (section 7): the alternative, session values as counted roots dropped on redefinition, needs every borrower to be gone, which only the
  quiescence of 3.3 can establish, and would make `def` a different thing at the REPL than in a program.
- **Atoms**: `(swap! counter inc)` in two inputs gives 1 then 2: the atom is one object in the app's heap. The app's own `def` atoms (its
  config, its connection pool) are readable and `swap!`able from the session: `(swap! app/pool-size (fn (n) (+ n 4)))` takes effect on the
  next read by the app, with the same atomicity the app has.
- **Expressions**: an input that is an expression becomes `live.entry.K` (as the REPL's `main` is today, `prn` of the value, or run for
  effect when its type is unit), compiled into a generation of its own, called on the attach thread with `call-i64`, its standard output
  captured through a pipe for the answer (the app's own output keeps going where it went). Its temporaries are released by the entry at its
  end, as any function's. The entry's module is retired with the generations.
- **Traps**: `--live` compiles the program as catching (`FIB_CATCH=all`'s mechanism, ADR 0009: every function returns its failure next to
  its value) and the entry runs under `catch-run`: a trap in an input is `failed: "nth: index out of range"` and the session goes on; a trap
  in a swapped handler is caught where the app catches, or, if the app does not catch, reaches `main` as a failure and ends the app as a
  trap does today. Cost: exceptions.md §3 measured 0 to 13% on the hot path of a catching program; live mode is a development mode. A trap
  inside a pool task is isolated as the catching rule says (parallelism.md §8).
- **Values of a type that is redefined**: refused (3.4, `live/Layout`). After `:restart` the heap is new and the session's `def`s are
  evaluated again by their initialisers (the transcript shows `; re-evaluated 3 defs`); a `def` whose initialiser reads the old world
  (`(def snapshot @app/state)`) gets the new world's value. No migration.

### 3.6 `:restart`

The client sends `restart`; the session ends `main` through `exit` (the program's exit hooks run, the pool drains, buffered output is
written), the process ends, and the client starts `fibc run --live` again with the same arguments and socket path, then replays the
session's definitions (its text) as one `load`. A restart is the only way a type, a signature, a protocol, a macro or `main` changes.
It is how Clojure users already work (a `(refresh)` that fails falls back to restarting the JVM), with the session text kept.

## 4. What the checker needs

What 0.1.16 gives: the library lowered, typed and decided once per process (`own.staged/Memo`, one prefix: the implicit modules);
a program's modules checked whole at every request; a REPL input at 8 to 15 ms with a session of a few definitions (section 1). What a
live session of a real app needs:

1. **A memo of the program's unchanged prefix, not only the library's.** The session program is `app modules .. , session module`. A swap of
   a function in the session module (or in the last app module) must not re-check the app's other modules: `own.staged` keeps one prefix
   under one key; it must keep the state after the longest unchanged prefix of the *program's* modules, keyed as `serve.incr` keys modules
   (the chain), as `serve.snap` does for the expander. The audit of persistent-checker.md §2 (every part of `Globals` and `Checker` is a
   Cell of persistent values) holds for the program's modules as for the library's; what changes is where the copy is taken (after module
   `k`, any `k`) and the demand (`types.infer.demand` must be computed over the whole program before the prefix is checked, as it is for the
   library now). **Estimate**: without it, a swap in a 3,000-line app costs 0.46 ms per line of type and ownership checking (dev-loop.md 2.2)
   = about 1.4 s; with it, the edited module only. **Built 2026-10-15 (LV4, `own.chain`; persistent-checker.md 8)**, with two differences
   from this text: the state is kept after *every* module up to the last but one (a chain under the modules' keys), not after one prefix, so
   an edit of module `i` re-checks `i` and the modules after it; and a kept module is checked whole, every unit and every function decided
   with an all-owned body as well, as the library is, instead of with the demand of the program (a `fib.*` module the program pulls in was
   checked on demand before; an app's own modules were checked whole already). Measured on meshql-fib (section 8).
2. **Per-definition incrementality inside a module** (DV6 proper) is not needed for the first milestones: a module that is a screenful
   costs 30 to 70 ms. It is needed when the app is one big file. Not in this plan.
3. **Summaries as values, comparable.** The signature rule needs the scheme of a function from the catalogue (`lsp.catalog/catalog-value`
   gives its text; the live table needs the `Ty` and a structural equality with colours erased, `ty-map-colours` exists). The summary rule
   needs `own.top`'s per-function summary (modes, escapes, derived result, taken-as-value) exposed as a value with `Eq`, and the SCC of a
   function (the checker works in SCCs, types §3.5). `fibc explain` prints these today; the live table stores them.
4. **The emitter per function.** `emit.program` already works from a queue of `Inst` requests (`program-request`, `WorkBody`, `WorkClosure`):
   a swap asks for `f`'s instantiations from the live table and runs the queue until empty with every other body answered by a `declare`
   of its stub instead of a definition. That is a mode of the queue, not a new emitter. The closure literals' numbering (`l.OWNER.N`) must
   be per generation (`l.OWNER.gK.N`), and the statics (strings, keyword ids, immortal closures) of a generation must either be re-emitted
   under generation names or resolved to the program's by `declare`: strings and closures of the new body are new (small); keyword ids are
   the program's table (shared through `declare`).
5. **The diff of the session.** `rc/merge-defs` keys definitions by `val:NAME`, `type:NAME`, `text:SRC`; the live rules need the diff
   classified (which keys changed, what kind), which is a small extension of `replcore`.
6. **The `--live` flag through the front end**: a `Program` field (as `unwinding` is) that the lowering reads to emit stubs for program
   bodies and the catching convention; the staged check is unchanged by it.

Missing and not in this plan: a check of the program with the library demand pruned *and* kept (persistent-checker.md §6: the kept state
checks the whole library, 0.46 to 0.55 s at the first program: a live session pays it once at start-up, acceptable).

## 5. Transcripts (the end state; nothing below runs today)

Each is `fibc repl --connect` against `fibc run --live app.fib`, an HTTP app whose `main` calls `(run-server handle opts)`. `fib> ` lines
are typed; `;` lines are the client's; `>` lines are the app's own standard error.

**1. Edit a handler in a running server.**
```
$ fibc run --live app.fib -- --port 8080 &
> fibc: live at /run/user/1000/fib-live/48213.sock
$ fibc repl --connect 48213
; attached to app.fib (pid 48213, generation 0, catching); 3 threads, pool 28 workers idle
fib> (defun handle (req: Request) -> Response (text-response 200 (str "hello, " (query req "name"))))
; swapped handle (generation 1): check 41 ms, emit 6 ms, jit 9 ms
$ curl 'localhost:8080/?name=tom'
hello, tom
```

**2. Inspect an atom the app owns.**
```
fib> @app/requests-served
1842
fib> (swap! app/log-level (fn (_) :debug))
:debug
fib> :inspect app/pool
app/pool : (Atom (Vec Conn)) = [#Conn{:id 1 :busy false} #Conn{:id 2 :busy true}]   ; prn, 2 of 2
```

**3. Trace a call.**
```
fib> :trace handle 3
; tracing handle (generation 2 wraps generation 1), 3 calls
call handle (#Request{:method :get :path "/" :query "name=tom"}) -> #Response{:status 200 ..}  0.4 ms
call handle (#Request{:method :post :path "/items" ..}) -> #Response{:status 404 ..}  0.2 ms
call handle (#Request{:method :get :path "/items/7" ..}) -> trap: nth: index out of range (caught in serve-task)  0.3 ms
; trace off (generation 1 restored)
```

**4. Fix a bug found by a failing request.**
```
fib> (defun item-of (path: str) -> (Option Item) (get @app/items (parse-id path)))
; swapped item-of (generation 3): check 39 ms, emit 4 ms, jit 7 ms
fib> (item-of "/items/7")
nil
$ curl -i localhost:8080/items/7 | head -1
HTTP/1.1 404 Not Found
```

**5. A refused change, with a good error.**
```
fib> (defun handle (req: Request db: Db) -> Response (route db req))
<input>:1:1: handle now has type (fn (Request Db) Response), was (fn (Request) Response); callers were compiled against the old one [live/Signature]
  hint: :restart changes signatures; the session's 4 definitions are kept and replayed
; refused; the session and the program are unchanged (generation 3)
fib> (defun handle (req: Request) -> Response (do (swap! app/last (fn (_) req)) (route req)))
<input>:1:1: handle's parameter req was borrowed and is now owned (stored at <input>:1:52); callers pass it borrowed [live/Summary]
  hint: store (clone req), or :restart
; refused; the session and the program are unchanged (generation 3)
```

**6. Reload a file.**
```
fib> :load src/routes.fib
; src/routes.fib: 12 definitions, 2 changed: swapped route, not-found (generation 4): check 58 ms, emit 9 ms, jit 12 ms
fib> :load src/types.fib
src/types.fib:4:1: Item changed; values of Item exist in the running program [live/Layout]
  hint: :restart
; refused; nothing of src/types.fib was applied (generation 4)
```

**7. A long-running pool task during a swap.**
```
fib> (def job (future (pmap score @app/items)))          ; 10 s of work on the pool, calls score per chunk
; defined job
fib> (defun score (it: Item) -> f64 (* 2.0 (. it weight)))
; swapped score (generation 5): check 37 ms, emit 4 ms, jit 6 ms
; note: 1 spawned thread and 28 pool workers are running; generation 4 is kept until :retire at a quiescent point
fib> :status
; generation 5; generations kept 6 (214 KB); threads: 1 spawned, 28 pool workers (27 busy); values: 2 defs, 0 B leaked
fib> @job
[..]                                                      ; chunks started before the swap used the old score, later ones the new
fib> :retire
; not quiescent: 1 spawned thread (the server's accept loop); :pause first
fib> :pause
; paused at 1 pause point (run-server, between connections); pool idle; spawned threads: 0 besides the server
fib> :retire
; removed generations 0..4 except the bodies slots still name (4 kept, 61 KB)
fib> :resume
```

**8. A trap in an input, and the session goes on.**
```
fib> (nth @app/items 99)
; failed: nth: index out of range (caught; the app is unaffected)
fib> (count @app/items)
12
```

## 6. Package plan

Sizes are fibber lines, files under 500 and functions under 50; "day" is the project's rough unit. Each test can fail and names its mutant;
`scripts/mutant-live.sh MODE` plants one fault in a copy (the pattern of `scripts/mutant-serve.sh`) and the named test must fail.

| Pkg | Scope | Files | Acceptance (executable) | Mutants that must be killed | Depends | Size |
|---|---|---|---|---|---|---|
| **LV0** stubs and slots | `--live`/`FIB_LIVE`: program bodies behind a stub and a slot; `atomic-load acquire` in the stub; generation names; a `Program` flag; the catching convention on | `emit/live.fib` (new), `emit/function.fib`, `emit/names.fib`, `emit/program.fib`, `driver/{args,commands}.fib` | `compiler/tests/live/stubs.sh`: `F run --live` over the quick stdlib sample and `cases/ownership` gives the same table as `F run` (305 and 100 rows, the trace mode included: no extra `A`/`F` lines); the emitted lIR of `hello` has one stub per program function and none for the library (a golden); `scripts/bench/quick.sh` row `live-hello` records the cost of the stub (bound: under 1.3x of `run` on the n-body shootout at 1e5 steps; a stub that is a `call` and not a `tailcall` blows the 1e6-deep recursion case) | `stub-no-tailcall` (recursion case overflows); `stub-plain-load` (TSAN on the swap stress of LV2 reports) | none | 300, 1 day |
| **LV1** attach and values | the socket in the app's process, the attach thread, `hello`, `eval` of expressions and `def`s (no swaps yet), `type`, `inspect`, `status`, `hello`; the uid check; `fibc repl --connect [--json]` | `serve/live.fib`, `serve/livesock.fib`, `serve/liveclient.fib` (new), `serve/repl.fib`, `serve/sock.fib`, `serve/replcore.fib` | `compiler/tests/live/run.sh`: the atom transcript of section 1 answers 1 then 2; a `def` value read by three later inputs; an expression that traps answers `failed` and the next input works; a client of another uid is refused (a test user is not available: the check is unit-tested by a planted uid); 1,000 malformed lines, the app survives; the socket is 0600 in a 0700 dir; no `--live`: `not-live` | `value-reevaluated` (the `def` runs again per input: the counter stays 1); `trap-not-caught` (the entry runs without `catch-run`: the app dies) | LV0 | 600, 3 days |
| **LV2** body swaps (milestone 1) | the live table; the diff of the session; the signature and summary rules; emit per function with `declare`s; a tracker per generation; `live.patch`; `swapped`/refusals with `live/*` codes and hints | `serve/livetable.fib`, `serve/liverules.fib`, `emit/live.fib`, `native/jit.fib` (`session-add-module-tracked`, `session-remove-tracker`), `llvm/orc.fib` (the four tracker externs), `own/summary.fib` (the summary as a value with `Eq`) | `compiler/tests/live/swap.sh`: transcripts 1, 4, 5 exact; a swapped function called through a closure value made before the swap (the `run-server` shape, in a case) runs the new body; a method swap; a generic function with two instantiations swaps both (a planted "first instantiation only" fails it); after 200 swaps of one function the answers are right and `status` shows 201 generations kept; **planted faults in the swap** (`scripts/mutant-live.sh`): `patch-before-jit` (the slot is stored before lookup: a call during the window jumps to 0: the stress of LV3 crashes), `summary-ignored` (an owned/borrowed change accepted: the ownership case in the test leaks or double-frees under `--trace`, the audit fails), `signature-ignored` (a changed arity accepted: the test's call reads garbage: the case's expected output differs), `old-tracker-removed` (the old generation removed at the swap: the in-flight test of LV3 crashes) | the four named | LV1; checker item 3 and 4 of §4 | 900, 5 days |
| **LV3** swaps under load | the stress: 8 spawned threads and the pool calling `f` in a loop while the attach thread swaps `f` 1,000 times between two bodies with different constants; every result is one of the two; `fib.threads-live`, `fib.sched-active` read for `status`; `pause`, `pause-point`, `resume`, `retire` at quiescence; `fib.os.live` | `rt/live.lir` (new: `fib.live-pause`), `lib/fib/os/live.fib`, `lib/fib/http/server/api.fib` (the pause point), `serve/live.fib` | `compiler/tests/live/stress.sh`: the stress above exits 0 and `scripts/tsan.sh` on it is clean; transcript 7 exact; `retire` while not quiescent refuses; after `pause`+`retire`, `status` kept = the slots' generations only, and the stress resumed runs right (nothing freed was live); a `retire` that frees a generation a thread is in is prevented by the pause: mutant `retire-without-pause` must make the stress crash or TSAN report | `retire-without-pause`, `patch-plain-store` (TSAN) | LV2 | 500, 3 days |
| **LV4** the program-prefix memo | `own.staged` keeps the state after the longest unchanged prefix of the program's modules (§4.1); `FIB_CHECK_CACHE=verify` covers it | `own/staged.fib`, `types/persist.fib`, `serve/incr.fib` | `compiler/tests/persist/run.sh` extended: a 2,000-line program (the compiler's own `syntax/` modules as the app) with a one-function swap in its last module: check under 80 ms warm, against the whole check; `verify` mode over the swap transcripts byte-identical lIR; the planted faults of `scripts/mutant-persist.sh` re-run with a program prefix (the key ignores a program module's text: stale) | `prefix-key-ignores-text` | LV2 | 400, 3 days |
| **LV5** load, reload, watch, macros | `load` of a file as one unit (all-or-nothing); `fibc watch --live`; a changed `defmacro` reloads the module's dependents' bodies (every body whose expansion changed is swapped, the rest refused by rule) | `serve/liveload.fib`, `driver/watch.fib` | transcript 6 exact; a file with one refused definition applies nothing (mutant `partial-load` applies the rest: the test reads a half-applied state); `watch` swaps on a save within 1 s | `partial-load` | LV2, LV4 | 400, 2 days |
| **LV6** trace and restart | `trace`/`untrace` as generations; `restart` with the client's replay; `serve eval` forwarding | `serve/livetrace.fib`, `serve/liveclient.fib`, `serve/evalop.fib` | transcripts 3 and 5 (the `:restart` hint followed: the replayed session is `:defs` equal) exact; a trace of a function swapped while traced keeps tracing; `untrace` restores the exact previous address (`inspect` shows it) | `untrace-wrong-generation` | LV2 | 300, 2 days |
| LV7 editor and notebook | the VS Code client speaks `--json` to a live socket; a notebook cell is an `eval` | `editors/vscode` | the extension's `npm test` runs a three-cell notebook against a live `hello` | | LV1 | 200 JS, 1 day |
| LV8 a built binary that is live | `fibc build --live`: stubs in, a loader that `dlopen`s objects a `fibc serve` compiles against the binary's liro key (ADR 0024); no LLVM in the binary | later; needs the ADR 0024 cache in the key | the swap suite over the built binary | | LV2 to LV4, ADR 0024 | 800, 5 days; **not in the first plan** |

Order: LV0, LV1 (an agent gets resident values and a REPL in a running app before any swap exists), LV2 (the milestone), LV3 (the stress is
what makes LV2 trustworthy, so they ship together), LV4 (the cost of a swap in a real app), then LV5, LV6 as convenience. LV0 through LV3 is
about 2,300 lines and two to three weeks of one agent; a gate stage `live` (`compiler/tests/live/*.sh`) joins `tools` in `scripts/gate.sh
--full`, and `scripts/mutant-live.sh` runs in the background like the other mutant suites (not gating).

## 7. Open risks, and the decisions the owner must make

Risks:

1. **Two runtimes, one process, more threads.** The compiler's runtime and the JITted program's share the process today only on the
   compiler's one thread plus the macro mailbox worker. The attach thread is a compiler-runtime thread that calls into program code while
   program threads run; the allocator (`fib.mt`, arenas) of each runtime is its own, but signals, `exit`, `SIGPIPE` and the stack guard are
   per process. LV1's tests must include the app spawning and pooling while inputs run; an issue here changes the design to a second
   process (the client) that drives a thin in-app thread, with the engine in the client (more plumbing, same mechanics).
2. **Retirement needs a pause.** Until stack scanning exists, old code is kept and `retire` needs quiescence. A server with a connection
   never idle never retires. Memory grows by the size of swapped bodies (estimate 20 MB per thousand swaps). Acceptable for a development
   mode; the risk is an agent that swaps in a loop for hours.
3. **The summary rule may refuse too much.** A body change that makes a parameter owned (it is now stored) is a legitimate edit refused
   until restart. Measured on the corpus (LV2's test over the cases) the plan will say how often; a second convention (compile both modes
   of the body and let the stub pick) is possible but doubles the emitted code.
4. **Generic functions and closures across generations.** A heap closure made by generation 1 and called by generation 2 code: the
   convention is the same (the closure's code releases `env`), so it works; the risk is a literal's statics (strings, keyword ids) that
   generation 2 re-emits under new names and that must compare equal to the program's (keyword ids by `declare`, strings are values). A
   case per kind in LV2.
5. **`catching` changes the program's performance and its traps.** A live session is a catching program; a bug that only shows with the
   non-catching convention is not seen live. Documented; `cases` stay the verdict.
6. **Darwin.** `getpeereid`, `SO_PEERCRED` and the runtime directory differ; the Mac check (`scripts/mac-check.sh`) gets the LV1 test.

Decisions for the owner:

1. **One process (`fibc run --live`) first, a live built binary (LV8) later.** Yes or no.
2. **The strict swap rule** (same scheme, same summary; types, protocols, `def` types, macros, `main` only through `:restart`). Yes, or ask
   for the dual-mode body of risk 3.
3. **Session `def`s are immortal and a redefinition leaks the old value** (counted in `status`). Yes, or counted roots dropped at quiescence.
4. **A live program is a catching program** (traps in inputs and swapped handlers are failures, 0 to 13% hot-path cost). Yes or no.
5. **No automatic retirement; `pause`/`retire` at quiescence.** Yes, or fund stack scanning (DV9 plus a thread handshake) in the plan.
6. **Security in a container**: unix socket with uid check only, the client run inside the pod; or a token on TCP loopback (`FIB_LIVE=tcp:
   127.0.0.1:0` with `FIB_LIVE_TOKEN`). The design recommends the first and offers the second as a later flag off by default.
7. **The names**: `fibc run --live`, `fibc repl --connect`, `live/*` codes, `fib.os.live`. `--live` must not collide with any compiler
   function name (ADR 0003: it is a flag, not a form; `live` as a namespace prefix is clear in `compiler/`: no `live` module exists).

## 8. Built, not built, measured

- **Built (0.1.16, read and quoted above)**: `fibc repl`, `fibc serve eval`, the engine with the persistent checker, the JIT session over
  LLJIT with names rules and pruning, `native.call`, the pool with quiescence, the catching convention, `FIB_CATCH=all`.
- **Not built**: everything in sections 2 to 6: stubs and slots, the attach socket and thread, the live table and rules, resource trackers
  (the externs are not bound), `fib.os.live`, pause points, retirement, the program-prefix memo, `--connect`, `watch --live`, trace, restart.
- **Measured** (section 1): a REPL input today is 8 to 15 ms of front end and 35 to 53 ms of fork, JIT and run; values do not persist (the
  atom reads 1 twice). **Estimated, not measured**: the cost of a one-function swap (2 to 3 ms of code generation plus the check of the
  edited module, 30 to 70 ms for a screenful), the memory of kept generations, the stub's call overhead (bounded by LV0's bench row).

## 9. Decisions of 2026-10-15 (owner)

1. **A `def` must be redefinable in a running app** ("we absolutely need the ability to redefine a def"). This replaces the "immortal,
   leaked by design" rule of 3.4 and 3.5 for redefinitions. Rules:
   - Same type: the slot's pointer is swapped with a release store (readers load with acquire); the old value goes on a retire list and
     is **dropped, destructor included, after a grace period** (the epoch mechanism that frees old code), not leaked. A borrow of a def
     cannot outlive the function that took it (ownership checker), so quiescence of all threads past the swap makes the drop safe. A
     value that cannot be dropped yet stays on the list (never freed early, the swap never blocks).
   - Different type: treated as a signature change: dependents are re-checked and the swap is accepted only if they still check,
     otherwise refused with the checker's message.
   - A plain `def` re-evaluates its expression on redefinition (Clojure's `def`); a `defonce`-style form keeps the existing value, for
     atoms, connections and servers that a redefinition must not reset.
   - A new `def` in a loaded file gets a slot in the new generation (LT4 of live-meshql.md), its initialiser run once.
   This is LT4 of docs/design/live-meshql.md with the old value retired and dropped instead of leaked.
2. **The project's live entry mimics Clojure's `user.clj`**: a `dev/live.fib` in the project that starts the app and exposes the helpers
   (`rebuild!`, the test entry); `fibc run --live APP --with SPEC` is the general form later (live-meshql.md §8, decision 1).
3. **Redefinition is a live-environment feature only.** Slots, stubs, retire lists and `def` redefinition exist only in code compiled
   with `--live` (`FIB_LIVE`). A normal `run`, an AOT `build` and a release binary compile a `def` as the immutable, immortal constant
   it is today (types §2.16), with no slot, no indirection and no cost, and a second definition of the same name stays the compile
   error it is. `fibc build --live` (LV8) is the only way to get a live binary, and only by asking. The test that guards this is the
   LV0 one already merged (a plain emit mentions neither `live.` nor `.g0`, byte-identical to before); LT4 adds the same check for
   defs: a plain emit has no def slot and a duplicate `def` still errors.

- **Status 2026-10-15**: LV0 (emit.live) and LV1 (serve.live, serve.livesess, serve.livesock, serve.liveclient; compiler/tests/live/attach.sh)
  are built. Where LV1 as built differs from the text above: (a) the attach server runs on the driver's thread and the program's `main` on a
  thread of its own (2.1 has them the other way round; the session's engine is `Cell` state and cannot be given to `spawn`); (b) a session
  `def` may not be redefined (`live/Def`, where 3.5 says a redefinition leaks the old value), nor a function (`live/Swap`: a body swap is LV2),
  nor a type (`live/Layout`), nor `main` (`live/Main`); (c) an expression's `stdout` is not captured (its `pr-str` value is written to a file
  of its own; `println` in it goes where the program's output goes); (d) a `def` whose initialiser traps aborts the program (the init runs
  outside the guard); (e) every input module that holds a static or a lambda is kept for good (`live.extract/pins?`), `status.generations.kept`
  counts them, and nothing is retired (LV3); (f) each program of the session has its own closure and vtable statics (shared objects, strings
  and keywords), since a later program does not emit the code of a closure it does not make; (g) the base is the program as `fibc run` makes
  it (the driver's front end on the file, pruned, the same trace: the case tables agree under FIB_LIVE=1), while an input is checked against
  the REPL's re-assembled text of the program (`<session>:L:C` in a diagnostic about a program definition, as `fibc repl` shows one), and an
  input whose program numbers a `def` of the base differently is refused rather than read a wrong slot; (h) a `def` of the library the program
  never reached is made when an input first reaches it, on the attach thread; (i) `--trace` is set after the compiler's own JIT sessions
  are up, so a later input that runs a macro traces that macro's session on standard error.
- **Status 2026-10-15, LV4 (the program-prefix memo) built**: `compiler/own/chain.fib` (`own.chain`), `compiler/own/staged.fib` (the `Memo`
  holds the chain), `compiler/serve/incr.fib` (the engine checks through it). The function the session calls, through the engine it already
  uses (`inc/engine-front`; nothing in `serve/live*.fib` changes): `(own.chain/staged-check-chain memo lib-key k keys modules prelude target
  main library)`. Its contract: `keys` has one key per module (the engine's `module-key` chain: a module's key covers its text and name and
  those of every module before it; `lib-key` is the library's, the first `k` modules'); the answer is `staged-check`'s (the whole check's
  program, the ids apart, persistent-checker.md 2); the checker's state after each module from the first not kept up to the last but one is
  kept in `memo.chain` under its key, so a later request with the same keys up to module `i` checks only the modules after `i`; the last
  module is always checked (and `main` required of it when `main`); a module that does not check whole is remembered in `memo.chain-bad`
  and never kept; under `FIB_CHECK_CACHE=verify` the engine compares the result with the whole check's lIR or diagnostics and a changed kept
  state is an internal error; `FIB_CHECK_CACHE=off` bypasses it. The memo is the engine's (`Engine.memo`), one per session, no global state
  (ADR 0013). Measured, `--server check` through `F serve` on meshql-fib's server program (304 modules, 51 implicit, 253 "program" modules
  of which about 150 are `fib.*` modules the program pulls in; a function appended to a module as the edit; `check_ms` is the engine's typing
  and ownership time, `total` its front end with the load of 304 files, about 100 ms, and the expansion of the edited modules):

  | edit | before (main 5301e051) check / total | after (LV4) check / total | modules re-checked after |
  |---|---|---|---|
  | first request of the process | 2,735 / 4,002 ms | 6,181 / 8,010 ms | 253 (252 kept) |
  | the last module (`src/main.fib`, 253 of 253), three times | 2,453 to 2,669 / 2,560 to 2,783 ms | 8 to 9 / 112 to 129 ms | 1 |
  | a handler module near the end (`meshql.restlette`, 247), twice | 2,639 to 2,648 / 2,770 to 2,782 ms | 128 to 129 / 257 to 263 ms | 7 |
  | the store, the middle of meshql's own modules (`meshql.store`, 202) | 2,692 / 2,967 ms | 907 / 1,245 ms | 52 |
  | the first of meshql's own modules (`meshql.json`, 137) | 2,682 / 3,441 ms | 2,520 / 3,397 ms | 117 |

  The swap of the design's scenario (a handler in `meshql.restlette` or `meshql.server`, or the session's own module last) is 0.1 to 0.3 s
  of front end, 8 to 130 ms of it the check; the load of the files is now the floor. Honest readings: (1) module order does make an early
  edit re-check most of the program: `meshql.json` is module 137 of 253 and an edit of it costs the whole check again (2.5 s), because the
  chain is prefix-only (everything after the edited module is re-checked, used or not); the fix is dependency-aware invalidation: re-check
  only the modules that (transitively) require the edited one, which needs the states kept per module to be composable out of order, or a
  per-module summary (its exported schemes and summaries) compared before deciding that a dependent must be re-checked, which is DV6's
  shape; not built. (2) The first request costs 6.2 s of check against 2.7: the chain checks every module whole and decides every function
  with an all-owned body as well, where the whole check prunes the `fib.*` modules' unreached units by demand; a live session pays it once
  at start-up (which is 10 s already), and again after an edit early in the order. The fix, if it matters: keep a `fib.*` module's state with
  the demand it was checked under and reuse it while a later request's demand over it is a subset, or decide the all-owned body lazily when
  a later module takes the function as a value. (3) The server's RSS after the transcript was 56 MB with the chain and 57 MB without it
  (`ps -o rss` of the `serve` process): the kept states share their structure.
  The acceptance of the row: `compiler/tests/persist/run.sh F prefix` (the 2,087-line app: the swap's check 1 ms against 236 whole, every
  step byte-identical under on, off and verify and `fibc emit`), `run.sh F server` (an edit of a kept module is seen), the unit program's
  `chain` and `chain-bad` steps, and `scripts/mutant-persist.sh` with `chain-key-ignored` and `prefix-key-ignores-text` (persistent-checker.md 8).
- **Status 2026-10-15, LV2** (serve.liveswap, serve.livetable, serve.liverules; emit.live's generations and body registry; the test
  compiler/tests/live/swap.sh over swap-app.fib; the mutants patch-before-jit, old-tracker-removed, summary-ignored, signature-ignored
  of scripts/mutant-live.sh): a function or an `impl` defined again at the REPL is a body swap of its family (the function, its all-owned
  twin and every specialisation the programs made; a method), emitted under the session's next generation, cut out with declarations
  for everything resident, added under a tracker of its own and kept, its slots stored with `live.patch` (a release store) after the
  lookup; the rules 3.4 `live/Signature` and `live/Summary` over every family of the live table, with a `hint`; `swapped`, `generation`
  and `swap` (`check_ms`, `emit_ms`, `jit_ms`, `patch_ms`) in the answer; `status.generations.count`; `inspect` of a function gives its
  generation and bodies. Where it differs from the text above: (a) the families swapped are exactly those the input's text redefines
  (the diff of the session's definitions), not a comparison of emitted lIR: the base is emitted from the file and an input from the
  REPL's text, whose position strings differ in every body, so lIR cannot be compared; an input whose `ns` clauses change what a name
  of an unchanged body resolves to does not swap that body; (b) a summary refusal says what the parameter is now (`stored`,
  `returned`: own.program's reason), not the position of the store; the scheme in a signature refusal is printed with its colour
  (`(fn :send (Request) Response)`); (c) `fn-type-str` of the stub is not compared: the rules are the checker's, as 3.4 says; (d) the
  `SCC` row of 3.4 is subsumed: a swap of `f` cannot change another body's code while every summary is equal, and a caller's lowering
  reads nothing else of its callee; (e) the emit is of the whole program in the session's tables (the cut is per family), not a mode of
  the emitter's queue (§4.4); the whole-program check is the engine's (LV4 memoises the program's prefix); (f) no generation is
  retired, and `status` prints the count, not the bytes (ORC does not report them); (g) `hello`'s thread and pool counts are LV3's;
  (h) an `impl` is keyed `impl:PROTO HEAD` by the REPL (serve.replcore), so a later `impl` of the same head replaces the earlier in
  `fibc repl` as well; (i) transcript 5's first line cannot come out as written: `main` gives `handle` to `run-server`, which fixes its
  type, so a signature change of `handle` is the checker's refusal (`cannot unify .. in the definition of main`), which also leaves the
  session unchanged; the signature rule's own refusal needs a change every caller still accepts (the test's `tag`, only ever
  `(str (tag))`, redefined to return a `str`), and `(route db req)` itself needs `route` at two arities (`defn`; `defun` has no arity
  overloading); (j) the live/orc and live/jit bindings of the prototype are used as they are, not moved to llvm/orc.fib and
  native/jit.fib; (k) a specialisation of a function nothing in the text reaches (one an earlier input instantiated) is not emitted
  again by a swap and keeps its slot: a type's id is one program's, so the live table cannot ask for it by type; the methods of a
  redefined `impl` are asked for by the `impl`'s key, reached or not.
- **Status 2026-10-15, LV3** (rt/live.lir; lib/fib/os/live.fib; the pause points of fib.http.server; serve.liveretire, serve.livesplit;
  the ops `pause`, `resume`, `retire`, `status.threads`, `hello.threads`/`pool`; `:pause`, `:resume`, `:retire` in the client; the test
  compiler/tests/live/stress.sh over stress-app.fib and swap-app.fib with its TSAN part over stress-tsan.fib; the mutants
  retire-without-pause and patch-plain-store, stub-plain-load's TSAN kill, and the planted fault live-patch of scripts/tsan-planted.sh):
  the stress of the row (8 spawned threads and the pool's tasks calling `f` while the attach thread swaps it 1,000 times; a thread inside
  a 1.5 s call during a swap of it; a trap in a worker task and in a pool task), the pause at the program's pause points, the retirement
  at quiescence, the resumed stress. Where it differs from the text above: (a) **quiescence as built** (rt/live.lir, where the memory
  orders are argued): the threads parked at a pause point equal the spawned threads alive plus one (`main`'s thread, which `fib.run-main`
  makes outside `fib.threads-live`), the async pool's queue is empty with no worker busy, and the work-stealing pool is not started or
  has every worker asleep with no work (the check of `fib.sched-quiesce` without its wait); a pause point reached by a pool worker is
  ignored (a pool task is done when it returns). (b) **which generation a parked thread's frame is in is not known** (3.3 says "the one
  the pause point's frame is in"): `retire` keeps every generation of every family whose code reaches a pause point, by a fixed point
  over the references the session notes of each body and literal (direct calls of stubs, literals of its own, closure constants and
  vtables it names: serve.livesplit `code-refs`, serve.liveretire `pause-bearing`), and reports them `pinned (pause-point)`. Not followed:
  a closure or `dyn` value read from the heap and called on the way to a pause point, when the function that made it was swapped while
  the thread was parked below it; the lift is a walk of the heap from the def slots at quiescence, or stack scanning. (c) **a
  generation's statics outlive its code**: a swap's cut is split (serve.livesplit) into a data part, resident for good (new strings,
  keywords, a type's walkers and rows, and the stub, slot and generation-0 body of a function the base never reached), and a code part
  under the tracker (the bodies and literals that carry the generation, with the closure constants that name a literal); a code part
  that holds a literal or a closure constant is never retired (`pinned (closures)`), since a heap closure made by the body may still
  name it (the prototype's rule, FINDINGS §3), so a body with a lambda keeps its old generations until a restart; a body of calls and
  arithmetic (the stress's `f`, a handler of `str` and calls) is freed. (d) `status` reports counts, not bytes (ORC does not report them).
  (e) **transcript 7 is matched in its lines' shape, not its bytes**: `:status` prints `threads: 9 spawned threads (0 parked) and 28 pool
  workers (3 active)`, the refusal is `; not quiescent: no pause was requested; pause first [not-quiescent]` (or what still runs:
  `1 thread not at a pause point, pool 3 active`), `:pause` prints `; paused at 10 pause points; pool idle (28 workers)` or that it is
  not quiescent after the timeout (5 s, `:pause MS`; the request stands and `:resume` lifts it), `:retire` prints `; removed 20
  generations; 2 kept (generation 23)` with `, pinned: handle (pause-point)` when something is; the `; note:` after a swap comes when the
  pool has started; the transcript's `(def job (future (pmap ..)))` is not run as written (a `future` of a 1.5 s call is the long call
  of the test). (f) **a trap in a worker task is not fatal in a live program**: FINDINGS (4) and (3) say a trap on another thread is
  fatal; in stage 2's catching convention a spawned task's trap is its failure (ADR 0001) and a pool task's too (parallelism.md 8, the
  runner's own catch), and `try-join` answers `(Err (Trap "nth: index out of range"))` for both while the program runs on (the test's
  `trap in a spawned worker task`, `trap in a pool task`). (g) **what ThreadSanitizer covers** (stress.sh's tsan part, `scripts/tsan.sh`
  with `TSAN_APPEND`): the program is emitted in live mode with the real stubs, slots and runtime, and a swapper appended as lIR stores
  the two bodies' addresses into `f.work`'s slot with the runtime's `fib.live-patch` while 8 threads and 4 pool tasks call through the
  stub and the main thread runs a pause round; a plain store (the planted fault, the mutant) or a plain load (the mutant) is a report
  `data race | atomic read f.work | write fib.live-patch | global 'live.f.work'`. Not covered by TSAN: ORC's writing of a generation's
  code before its address is stored (only the session does that: the socket stress is the run), the removal of a tracker, and a
  weakened order among the atomics (which the socket stress and retire-without-pause are for, not a proof). (h) The pause point is also
  in a kept-alive connection's wait between requests (fib.http.server.connection), not only the accept loop: with a connection open the
  server is quiescent. (i) A swap of the stress app costs about 146 ms (check 26, emit about 90, jit under 10: the emit of the whole
  program, LV2 (e)), so the 1,000 swaps are about two and a half minutes of the socket part; the two parts are stamps of their own in
  mk/tools.mk (full gate only).
