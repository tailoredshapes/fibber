# Live sessions: emit only what changed, and swap any module

Status: **design, 2026-10-15; nothing here is built.** Section 1 is measured; the experiment of 1.6 ran in a scratch copy and is not in
the tree. What this answers: the first real use of the live REPL (meshql-fib, `docs/live-dev.md` on its branch `live-dev`) found that
every input of an attached session re-emits the whole program (9.8 s on a 304-module server, 41 s with the spec loaded) while the check
is 160 ms (LV4), the JIT 27 ms and the patch 0; that only the session's own module can be swapped; and that `inspect` runs the front end.
Sections 2 and 3 design the fixes: an emitter that lowers only the redefined families against the session's resident state (the "queue
mode" of live-repl.md §4.4, which LV2 built as a whole-program emit with a per-family cut, §8 note (e)), and a `load` of any program
module. Section 4 is the package list, section 5 what the owner must decide.

**Measured** means the command was run on 2026-10-15 and its output is quoted. **Estimate** is arithmetic on measured numbers and says so.
Code read: `compiler/serve/{livesess,liveswap,livetable,liverules,live,liveclient,replcore,incr,repl}.fib`, `compiler/live/{extract,jit}.fib`,
`compiler/emit/{compile,program,mono,live,statics,assemble,lower/mod}.fib`, `compiler/emit/defs/{mod,jit,data}.fib`, `compiler/own/chain.fib`,
`compiler/expand/modules.fib`, `compiler/pkg/modules.fib`, `compiler/driver/front.fib`, live-repl.md, live-meshql.md, persistent-checker.md §8.

Contents: 0 the short answer; 1 the profile; 2 incremental emit; 3 module-level swap (`load`); 4 packages; 5 risks and decisions; 6 built.

## 0. The short answer

1. **The 9.8 s is not the lowering of the program. It is the constant `def`s.** Of the 9.3 to 9.4 s an input spends in the emitter, 8.3 to
   8.8 s is `make-defs`: the 205 constant `def`s of the program (`fib.*` and meshql's) are each evaluated again, through a fresh LLJIT, in a
   module that carries every object type of the session (1,230 of them, rendered with their walkers: about 8,000 items per def, where the
   base's own run of the same defs had 460 to 920). The base paid 768 ms for the same 205 defs. Lowering every body `main` reaches is
   0.9 s; the cut, the JIT of the new generation, the patch and the table are 50 to 80 ms together. (1.2, 1.3)
2. **Reusing the def values across the session's programs takes an input from 9.7 s to 1.0 s of emit; a swap is then 1.36 s end to end.**
   Measured with a 60-line experiment in a scratch build (1.6): the server answered right after it. That is package LE1, the smallest
   change with the largest effect.
3. **The remaining 0.95 s is the whole program's lowering, and the fix is the queue mode the design already named**: a request for a body
   whose code name is resident is answered by its name (the cut declares it), so the queue holds the redefined families, their new closure
   literals and any instantiation the session lacks. With it the emit of a swap is tens of milliseconds and a swap on the 304-module
   server is about 0.3 s end to end, the engine's load of 304 files (110 ms) its floor. (2, target 2.6; LE3)
4. **`load FILE` swaps functions of any live module of the program** (`f.meshql.restlette.on-item`): the session keeps the loaded text of
   each module as an override the engine checks instead of the file, the module's definitions are diffed against the session's copy, the
   changed families go out in one generation, all or nothing, with the rules of LV2 and diagnostics at the file's positions. A module that
   is not live (a dependency without `--live-deps`) is refused with the flag in the hint. (3; LM1)
5. **The first input's 21 s and `inspect`'s 10 s are two more wastes with cheap fixes**: warm the engine on the base program before the
   first connection is accepted (the app is already serving while it runs), and answer `inspect` from the live table. (3.7, 3.8; LE5, LE4)

## 1. The profile (measured)

### 1.1 Setup

lilnas, 28 threads, load average 1.5 to 4 during the runs. Stage 2 `F` = `/tank/data/fibber-scratch/live-demo/F`, built from fibber main
abc69a07 (0 modified) with the 0.1.16 seed (`buildF.log`: `rc=0 seconds=49.1`, `fibc 0.1.16`); the instrumented builds `Fp` and `Fp2` are the
same tree copied to `/tank/data/fibber-scratch/live-emit/tree` with `FIB_TIME=1` timers added to `emit.compile/compile-module-with`,
`emit.defs.jit/emit-def`, `serve.livesess/add-cut` and `serve.liveswap/define-input` (`Fp`), plus the experiment of 1.6 (`Fp2`); built with the
same seed (`rc=0 seconds=44.7`). The program: meshql-fib's `dev/live.fib` from `examples/farm` (branch `live-dev` at 2ff6af8), started as its
`scripts/live-demo.sh` starts it (`LD_PRELOAD` of libcrypto and libsqlite3, `FIB_LIB` the tree's `lib/`, `PORT`, `DB`), driven through
`F repl --connect PID --json` by a script that sends the demo's inputs and times each answer (`drive2.py` in the scratch directory).
`perf` (7.0.14) sampled the server during inputs, flat (`-F 499`) and with DWARF call graphs (`--call-graph dwarf,32768 -F 97`).

### 1.2 A plain emit of the same program, for comparison

```
$ FIB_TIME=1 F emit --time ../../dev/live.fib > emit-plain.lir            (examples/farm; three more runs under FIB_LIVE=1, FIB_PRUNE=0, both)
time front.expand 1859 ms  front.lower 686  front.infer 893  front.elaborate 43  front.own 2163  front 5826 ms  lir.emit 1899 ms   wall 8.62 s  rss 575572 KB   11536 lines
FIB_LIVE=1:   front 5784 ms  lir.emit 1886 ms   wall 8.60 s   11844 lines
FIB_PRUNE=0:  front 5810 ms  lir.emit 1649 ms   wall 8.42 s   11822 lines
both:         front 5883 ms  lir.emit 1774 ms   wall 8.64 s   12130 lines
```

The lIR emit of the whole program is 1.65 to 1.9 s; the stubs and the absence of the prune (an input's program is emitted with `keep-all`,
`emit.compile/finished`) change nothing measurable. So the session's 9.8 s is five times a whole-program emit, not one.

### 1.3 Where an input's time goes

The transcript with the instrumented `Fp` (`run2`): the base, then the demo's inputs. `check_ms` and `emit_ms` are the session's own figures
(`swap` in the answer); the `time` lines are the timers, with the counts they print.

| step | wall | the session's figures | the timers (ms) |
|---|---|---|---|
| start-up to the first answered request | 9.80 s | | base: `program-new` 12 (3,191 funs, 220 defs, 205 constant, 3,076 bodies), `inits-plan` 0, `make-defs` 768, `emit-all` 817 (4,050 functions, 3,304 instances, 1,230 objects), `finished` 189 |
| swap 1 (`handle` 404 to 410), the engine cold | 20.3 s | check 10,657 (expand 2,213, check 8,341), emit 9,404, jit 26, patch 0 | `input.front` 10,668; `rules` 8 (148 families); `program-new` 26 (7,890 bodies); `make-defs` **8,408**; `emit-all` 952; `finished` 9; `input.emit` 9,414; cut: `append-item` 7, `extract` 7 (1,943 items), `add-tracked` 6; `table-after` 6; `input.apply` 82 |
| swap 2 (back) | 9.64 s | check 160, emit 9,261, jit 34, patch 0 | `input.front` 192; `make-defs` **8,319**; `emit-all` 899; `input.emit` 9,272; cut 13+11+8; `input.apply` 82 |
| `(count [1])` | 10.08 s | front 161 | `make-defs` **8,812**; `emit-all` 889 |
| `inspect handle` | 9.93 s | front 162 | `make-defs` **8,302**; `emit-all` 879 |
| `(defun on-item ..)` (a new function, nothing swapped) | 9.61 s | front 160 | `make-defs` **8,315**; `emit-all` 895; `input.emit` 9,266; `input.apply` 51 |
| `(def demo-limit: i64 3)` | 9.60 s | front 158 | `make-defs` **8,301**; `emit-all` 930 |
| six more swaps of `handle` | 9.3 to 9.9 s each | emit 9,140 to 9,696 | |

Readings, in order of size:

1. **`make-defs`: 8.3 to 8.8 s of every input, swap or not.** `emit.compile/compile-module-with` calls `make-defs-at` before anything is
   requested: every constant `def` of the program (`emit.defs.data/def-ids`: 205 here) is lowered and evaluated through the JIT
   (`emit.defs.jit/emit-def`: a fresh `jit-new-fast-codegen`, `jit-add-module` of "the module so far", `jit-c-entry`, the value read out of
   memory). The per-def lines show why the session pays ten times what the base paid for the same defs:
   ```
   base:    time emit.def d.fib.os.backend.platform-name lower 0 ms jit+read 2 ms items 463     ..    d.meshql.graphlette.values.at-key  jit+read 3 ms items 920
   input:   time emit.def d.fib.os.backend.platform-name lower 0 ms jit+read 23 ms items 7736   ..    d.meshql.graphlette.values.at-key  jit+read 22 ms items 8026
            time emit.def d.fib.json.schema.index.list-keys lower 0 ms jit+read 325 ms items 7878   (the largest; 19 to 325 ms each, 40 ms on average)
   ```
   "The module so far" is `emit.assemble/module-parts-with`: the runtime, the externs, **the type table** (`objects-items-pb`: every object
   interned in the program's `Objects`, each as a struct, a row and its walkers), the statics, the constants so far and the bodies so far.
   An input's program is numbered in the session's shared `Tables` (`emit.program/program-new-with`), so its `Objects` already holds the
   base's 1,230 types when the first def is evaluated: 7,736 items instead of 463, and each of the 205 JIT sessions checks (`lir.check`),
   prunes (`native.jit.prune/dead-internals`), lowers and compiles that module. The DWARF call graph of a swap (`perf-swap-dwarf.data`,
   percentages of the samples of the input) puts it the same way: `emit-checked` 68.3%, of which `make-defs` 62.8% (`in-session` 47.6%:
   `session-add-module-live` 25.3%, `session-address` 21.5%; `session-module` 12.1%: `objects-items-pb` 11.4%, `render-tables` 10.3%),
   `emit-all` 5.2%; `lir.check` 6.5%, `dead-internals` 7.4%, LLVM's `SelectionDAGISel` 7.0%. Flat sampling (`-F 499`) has the runtime's
   count operations at the top (`fib.retain` 9.7%, `fib.defer` 6.1%, `fib.alloc-slow` 3.2%, `fib.drop` 3.2%, `fib.drop-one` 2.4%): the
   rendering, checking and pruning of 8,000-item modules 205 times is allocation-bound.
   **Nothing of it depends on the input.** The defs are the program's; their values were computed by the base, and by every input since.
2. **`emit-all`: 0.9 s, the lowering of every body `main` reaches** (3,304 instances, 4,050 functions with the stubs), plus the bodies the
   swap asks for. Of this only the redefined family, its closure literals and any instantiation the session lacks depend on the input;
   at 0.27 ms per instance (900 ms over 3,304) a family of two bodies and a few literals is about 2 ms (estimate).
3. **The rest is small and right-sized.** `program-new` 25 ms (it parses the runtime's lIR text twice, `runtime-items-target` and `-host`:
   constant per process); `finished` 8 to 9 ms (`module-parts`: the type table rendered once more); the cut 20 to 30 ms (`append-item`,
   `extract`, `export-resident`, `add-tracked`); the JIT of the generation 26 to 34 ms; the patch 0; `table-after` 6 to 7 ms (the schemes
   and summaries of 148 families as text); `rules` 8 ms. The re-assembly of the session's text (`rc/source-of`, 32 lines) and the
   expansion of the one changed module (16 ms) are inside the front end's 160 to 192 ms, of which the load of the 304 files is 106 to
   120 ms. **What the cut holds** (the histogram timer, swap 2): 1,942 items = 1,931 struct type declarations (`live.extract/structs-and-target`
   copies every struct of the program), 5 `declare`s of runtime functions, 2 of program stubs, 1 def slot, 1 string, **1 `define`:
   `f.handle.owned.g2`**, and the target. So LV2's cut is already one family; the per-family mechanism is not what is slow.
4. **The chain-checked program is bigger than the base's**: 7,890 decided bodies against 3,076 (LV4 checks every unit of every `fib.*`
   module whole and decides an all-owned twin for each, persistent-checker.md §8), and the first input makes four `def` slots the base
   never reached (`def.slot.8..11` in `status.values`). The emitter is demand-driven from `main`, so this costs the per-input emit nothing
   measurable (the same 3,304 instances); it costs the first input's check (reading 2 of §8).

### 1.4 The first input, expressions, `inspect`

- **The first input**: `input.front` 10,668 ms (`expand_ms` 2,213 + `check_ms` 8,341): the engine of the session starts cold (the base was
  compiled by the driver's front end, `front/check-file`, which keeps nothing in the engine; live-repl.md §9 LV1 note (g)) and the chain's
  first request checks every module whole (6.2 s against 2.7 for the whole check: persistent-checker.md §8 reading 2). Then the same 9.4 s
  emit. 20.3 s end to end here, 21.0 in meshql-fib's run.
- **An expression** (`(count [1])`): the same emit (8.8 s of `make-defs`, 0.9 of `emit-all`) for an entry of one call; 10.1 s.
- **`inspect handle`** (`serve.live/op-inspect`): it evaluates the name as an input (`lsw/input`), so it is an expression input (9.9 s) plus
  `type-of` (a second front end, 160 ms) plus the table lookup it wanted (0 ms).
- **A refusal** (the checker's, or `live/Def`): 0.19 s and 0 ms: nothing is emitted. Right.

### 1.5 Memory

`VmRSS` of the server (`/proc/PID/status`): **637,812 kB after start-up, before any input** (the base resident: its JIT, tables, the driver's
front end freed or not), **1,317,788 kB after the first input** (the engine's expander entries and the chain's 252 kept checker states), and
1,319,064 kB after six more swaps: about 210 KB per kept generation, which is ORC's module and the 1,931 struct declarations of its cut, not
code. A session of a thousand swaps is 200 MB of generations (estimate); the 680 MB of the engine is the price of LV4 and is paid once.

### 1.6 The experiment: def values reused across the session's programs

In the scratch copy, `emit.defs.jit/emit-def` keeps, under `FIB_LIVE_DEFCACHE=1`, the value (`DefValue`) of each constant def and the
constant items its reading appended, keyed by the def's body name (`d.fib.http.body.piece`: module-qualified, stable across the programs of
a session where the def ids are not), in a process-wide atom; a later program's `emit-def` of the same name replays the items into its
`Defs` (the `@def.N` names, the counter past them) and records the value, and still lowers the initialiser's body (0 to 2 ms: the statics it
names must exist in this program), skipping only the JIT. 60 lines; not for the tree (global state, and the closure problem of 2.4.5).
The same transcript with `Fp2` (`run3`):

```
swap 1:  check 10512 ms  emit 1076 ms  jit 42  patch 0     wall 11.8 s    GET /farm/api/nope -> 410
swap 2:  check   161 ms  emit 1001 ms  jit 25  patch 0     wall 1.36 s    GET /farm/api/nope -> 404
(count [1]) -> "1"  wall 1.32 s;   (defun on-item ..) 1.38 s;   (def demo-limit: i64 3) 1.34 s;   inspect handle 1.71 s
timers of swap 2:  program-new 25  make-defs 12  emit-all 946  finished 9  input.emit 1010  cut 9+8+7  input.apply (not changed)
```

`make-defs` goes from 8,319 ms to 12; the emit from 9.3 s to 1.0; a swap from 9.6 s to 1.36 end to end; the server answers as before.
The 206th `emit.def` line of the log is `demo-limit`'s, the one def the session added, evaluated through the JIT once. The LV2 test
`compiler/tests/live/swap.sh` under `Fp2` with the cache on: see 1.7.

### 1.7 Controls run

- `compiler/tests/live/swap.sh Fp2` with `FIB_LIVE_DEFCACHE=1` (the transcripts 1, 4, 5, the closure value, the method, the generic, 200 swaps
  under a worker, the refusals): `46 ok` lines, `live swap: ok`, rc=0 (`swap-test-cache.log` in the scratch directory). The cache
  changes no answer of the LV2 suite.
- Not run: the gate (this branch changes no code), the mutants (nothing to plant yet), meshql-fib's suite (unchanged tree).
- One observation outside the goal: the servers my script started exited silently some time after the last client disconnected (no line
  in their log; `kill` found no process); the demo script's server survives to be killed. Not investigated; likely the harness, noted so that
  the first LE package's test keeps a server across two connections.

## 2. Incremental emit

### 2.1 What the session already has, and what it emits again

The session keeps (serve.livesess `Sess`): the `Tables` every program is numbered in (object ids, strings, keywords, the generations and
the registered stubbed bodies), the resident index (`base`: every exported item of the base and of the kept modules, by name), the live
table (per family: generation, scheme, summary, the trackers), the engine (the chain), the session text. An input's program is emitted
whole (`ec/compile-executable-ast-with` with the swapped families' generations set ahead) and the cut declares what the index has. Of the
whole emit, what the input can change is: the bodies of the redefined families (and their closure literals, whose names carry the
generation: `l.f.handle.owned.g2.N`), the stub, slot and generation-0 body of a new function, the slot and init of a new `def`, an
expression's entry and its literals, and any specialisation or library body that the session has not made yet (a new body calls a
generic at a new type, or a `fib.*` function the base never instantiated). Everything else is a repeat: the 205 constant defs, the 15
run-time def inits, `main`, the 3,300 instances, the statics the base has.

### 2.2 The resident state in the `Tables` (LE1, LE2)

Three additions to `emit.program/Tables`, all written by the base's emit and read by every program of the session:

1. **The constant defs' values and items** (LE1): `defs: (Cell (Map str (Pair DefValue (Vec LItem))))` keyed by the def's body name
   (`en/body-name g (BodyDef d) []`: `d.MODULE.NAME`, what 1.6 keyed by), and `emit-def` consults it as the experiment did: a hit replays
   the items (their `@def.N` names into `constants`, the counter past the largest) and records the value; a miss evaluates through the
   JIT and records. The key is the name and not the id because the ids are the program's (`slots-agree` already refuses an input whose
   numbering moved) and a `require` input adds modules before the session's. The values are the lIR expressions of immortal constants:
   plain data, or `@def.N` and `@str.N` references (shared names), or `@clo.N` (not shared today: item 3). Under `FIB_LIVE_EMIT=verify`
   (2.5) a hit is also evaluated and the two values compared.
2. **The runtime's items** once per process (`runtime-items-target`, `-host`: 25 ms of parsing per program today): a cell in `Tables`
   or a lazily filled `def` of emit.rt (the runtime text is a constant of the build). Small; goes with LE1.
3. **Closure and vtable statics shared by name** (LE2): today `statics-closure` names an immortal closure `@clo.N` by the count of the
   program's own `closures` cell and `statics-vtable` names `@vt.N` the same way, so two programs of a session number them differently,
   which is why `program-tables` gives each program fresh closure and vtable cells and the session copies them into every cut (LV1 note
   (f); emit.statics `program-static?` is the one predicate: serve.livesess, serve.livesplit and emit.defs.cache read it). A quoted form
   `@q.N` (emit.program `program-fresh-quote`) is numbered per program the same way and is under the same rule since the swap crash of
   docs/design/live-meshql.md 10; LE2 shares it by content as it shares the others. In live mode (`p.live`) the name is the key instead: `clo.CODE` for the closure whose code is `CODE` (a stub name,
   resident), `vt.NAME` for the vtable of instance `NAME` (mangled as `emit.names` mangles). Then the cells can be shared, the cut
   declares a static the base has and copies only a new one, and a def value that holds `@clo.f.x.owned` means the same object in every
   program. Plain mode keeps the counters, so a plain emit stays byte-identical (the LV0 guard, `compiler/tests/live/stubs.sh`'s golden).
   Why this matters for LE1: a constant def whose value is a function value (a table of handlers) would otherwise replay a `@clo.N` of the
   base into a program where `N` is another closure. The experiment did not hit this on meshql-fib (the server answered right through the
   transcript and 1.7's test), but nothing proves no def of a program holds a closure, so LE2 precedes the gate stage of LE1.

### 2.3 The queue mode (LE3)

`emit.mono/queue-request` gives a body's name and queues it when new. In live mode with a session's `Tables`, a request is answered by its
name **without being queued** when the body's *code name* is resident: `live/body-code-name` of the instance (`f.handle.owned.g2` for a
family at generation 2, `f.main`, `d.fib.os.backend.platform-name`, `f.fib.prelude.vec-nth.str`) is in `Tables.resident` (a `(Cell (Set str))`
the session fills from its index: the base's exported names, then every kept module's). A swapped family's generation is set ahead of the
emit (`emit-swapping` does this today), so its code name is not resident and it is lowered; everything it calls is answered by name and
declared by the cut; a callee the session lacks (a new specialisation, a library body nobody instantiated, a method of a new instance) is
not resident and is lowered, stubbed if it is a program body (`add-stub` registers it at its family's generation, 0 for a family the
table has never swapped, which is what LV2 does for "a twin or specialisation the base lacked", note (k)), and exported by the cut as now.
Closure literals are requested only by lowered owners, so a resident body's literals are never asked for.

With that one rule the queue of a swap holds the family's bodies, their literals and the missing instantiations; `main`, the 15 inits and
the 205 constant defs are resident and never queued. `compile-module-with` keeps its shape: `inits-plan` (0 ms), `make-defs-at` (replays:
LE1), `inits-request`, `request-all`, `program-request main`, `emit-all`, `finished` (no prune, as now). What it stops doing is the
3,300 instances. `finished` still renders the type table (8 to 9 ms) because the cut's `append-item` reads the program's `fib.types` rows
to append the new ones; that can be narrowed later (render from the first new row).

Cost after LE1 and LE3 (estimate from 1.3): `program-new` 25 → under 5 ms (item 2 of 2.2), `make-defs` 12 (the replay of 205 defs; could be
0 by replaying lazily), `emit-all` 2 to 5 ms for a family and its literals (0.27 ms per instance measured), `finished` 9: **the emit of a
swap under 40 ms**, of an expression input about the same, of a new function the same plus its stub.

### 2.4 Correctness: what must stay true

1. **Generation naming.** A family's bodies are lowered under `NAME.gK` with `K` the table's generation for the family (`Tables.generations`),
   its literals `l.NAME.gK.N` (emit.names over the code name). In queue mode nothing changes: the code names of the swapped family are new
   (not resident), the rest are resident and declared. The names rules of spec/lir.md §11 hold through `names-check` as now: a cut defines
   only new names and declares the rest with the types the index has.
2. **Per-generation closure rows.** A literal's object type (`clo.l.NAME.gK.N`) is a row of the session's type table, appended by
   `append-item` before the generation runs (`live.types.append.N`); a later generation's literals are new rows. Unchanged.
3. **"The families swapped are those the text redefines" (LV2 note (a)).** The diff of the session's definitions decides what is swapped;
   an input whose `ns` clauses change what a name of an unchanged body resolves to, or a changed macro, changes that body's code without
   swapping it. Queue mode makes this hole *visible*: such a body is resident by name, so it is declared, and the new resolution is never
   lowered. The close (LM2): decide a family changed by comparing the **expanded form** of its definition (the engine's `Expanded` forms
   of the module, printed without positions and hashed) with the hash recorded at the family's last accepted generation, not its text. A
   definition whose text is the same but whose expansion differs (a `require` alias moved, a macro changed) is then swapped; a definition
   whose text changed only in a comment is not. This also lifts `live/Macro` for the bodies of the session module and of a loaded module
   (every body whose expansion changed is swapped, which is LV5's rule), and it is what makes 3.5's "changed" precise for a loaded file.
4. **Specialisations of generics.** A swapped generic `f` has in the table the specialisations the programs made (`LiveBody.generic`,
   note (k): a specialisation an earlier *input* instantiated cannot be asked for by type, since its types are that program's); a swap
   re-emits the ones the current program reaches from `main` and the requests, as now. In queue mode a specialisation reached only
   through a resident caller is **not** reached (the caller is not lowered), so the swap must request every registered specialisation of
   the family explicitly: `requests-of` already lists the family's non-generic registered bodies; it must list the generic ones with the
   types recorded at their registration. That needs `LiveBody` to carry the types as the mangled text of the name (`f.g.$i64_` is its own
   key: `queue-request` keys by `Inst`, which is a `BodyKey` and types; the mangled name is derivable from the name alone only if the
   emitter can parse it back, which it cannot). The honest option: record the `Ty` vector at registration in the *session's* terms (the
   printer's text, `printer-ty`, which the checker can read back as a type expression of the session module), and request it by text. A
   specialisation whose types name an input's private type (a `defstruct` of an expression) cannot be re-requested and keeps its slot, as
   now; a test covers both.
5. **Vtables and statics.** With LE2 shared by name, a swapped method body's vtable is the base's (declared); a new instance's is copied
   once and exported. Keyword ids are the program's table (sealed, shared). Strings are shared. A def value's `@clo.CODE` is the base's.
6. **The slot-agreement check** (`slots-agree`) stays: it is 0 ms and it is the guard for a `require` input that reorders the ids.
7. **An input's program is still checked whole** by the engine (LV4): the rules compare every family's scheme and summary; queue mode
   changes nothing before the emit.

### 2.5 The verify mode

`FIB_LIVE_EMIT=verify` (the pattern of `FIB_CHECK_CACHE=verify`): for every input the session also runs the emit of today (the whole
program with the same generations) and compares, byte for byte after printing, each item the incremental cut defines with the same-named
item of the whole emit (both from the same session text, so the positions agree); a difference, or a name the whole emit lacks, is an
internal error (`live/EmitVerify`) and the input is refused. It also compares a replayed def value with a fresh evaluation (LE1). The
live test suite runs its transcripts under `verify` as `compiler/tests/persist/run.sh` runs the chain's; the demo transcript on
meshql-fib under `verify` is the acceptance of LE3 on a real program (each input then costs 1 + 9 s, which is the point of a mode).

### 2.6 The target

**Swap-to-effect on meshql-fib's 304-module server under 300 ms**, from the measured parts: the front end 150 to 192 ms (the load of the
files 106 to 120 of it, the expansion of the session module 16, the check 16 to 18), the rules 8, the emit under 40 (2.3, estimate),
the cut and the JIT 30 to 45, the patch 0, the table 7, the client 1 to 8: 240 to 300 ms. The floor after that is the engine's load
(reading and parsing 304 files per input), which a loader memo by file content would take to about 10 ms (not in this plan; a line in
LM1's "later"). The intermediate point, LE1 alone, is measured: 1.36 s. An expression input: the same less the table. The first input
after LE5 (3.7): the same as any input. `inspect`: under 5 ms (3.8). With the spec loaded (335 modules, 41 s today): the same arithmetic
gives under 350 ms (estimate; the load is 335 files).

## 3. Module-level swap: `load`

### 3.1 The op and the client

| op | request | answer |
|---|---|---|
| `load` | `file` (a path, relative to the session's working directory, or absolute) | as `eval`'s: `module` (its `ns`), `defined`, `swapped` (`meshql.restlette/on-item`), `generation`, `swap`, `stats`; or a refusal with every diagnostic (3.5) |
| `reload` | | `load` of every file loaded in this session whose content differs from the loaded text, in load order; `loaded` lists them, each with its answer |

`fibc repl --connect` gets `:load FILE` and `:reload` (the plain REPL's words); `:status` prints the loaded files. `fibc watch --live` (LT5)
sends `load` on a save. A `load` of the session's own `main` module is the file form of an `eval` of its definitions (the same path).

### 3.2 Which modules, and the family names

A loaded file names its module in its `(ns ..)` form. The module must be one of the program's (the engine's `Loaded` list of the last
accepted program), else `live/NotLoaded` (`the program has no module meshql.restlette; (require [meshql.restlette :as r]) at the REPL loads it`),
and one of its **live** modules (`emit.live/program-module?`: in a project the modules under its `:paths`, and the libraries' under
`--live-deps`; outside a project every module not named `fib.*`), else `live/NotLive` (`lacewing.router is a library of the project; its
functions are direct calls in the running program. Start with --live-deps=lacewing to make it live`). A function of a live module has a
family in the live table already (the base stubbed it: `f.meshql.restlette.on-item`, `emit.names/body-name` with the module prefix;
a method's family is its body name, an `impl` keyed `impl:PROTO HEAD` program-wide), so the rules, the swap and `inspect` work on it as
on the session module's: `serve.liveswap/families-of-def` takes the module's prefix (today it assumes the main module's empty prefix), and
`BodyRequest` gets the module (today `program-fun typed name` looks a bare name up). A new function in a loaded module is a new family at
generation 0 with the module's prefix (`make-functions` with the prefix). A new `def` in a loaded module, or a redefined one, is LT4 (the
owner's decision 1 of live-repl.md §9) and is refused `live/Def` until it lands; this design does not re-decide it.

### 3.3 How the engine sees an edited module

The engine's job reads a module's text from its file (`serve.incr/text-of`: the request's own text for the main file, the file for the
rest). The session adds **module text overrides**: `Sess.module-texts: (Cell (Map str str))`, file path to text, given to the engine as
the job's `texts` (one field, `(Map str str)`; `text-of` consults it before the disk; `mods/try-load-in` already takes the main source
as text and must take the map for the others: the loader's `roots-find` returns the file and its source, `visit-source` gets the
override's source for that file). The keys change as they must: `module-key` is over the text, so the loaded module and every module
after it in the load order get new keys, the expander re-expands them (`reusable` stops at the first changed key) and the chain re-checks
from there (`own.chain/longest-kept`). The cost is LV4's, measured in live-repl.md §9: `meshql.restlette` (module 247 of 253) 128 ms of
check, 257 of front end; `meshql.store` (202) 0.9 s; `meshql.json` (137) 2.5 s. The order rule is the same as for the REPL: an early
module costs the modules after it; dependency-aware invalidation is not in this plan (persistent-checker.md §8 reading 1).

Positions: the diagnostics of a loaded module are at the file's positions (`src/meshql/restlette.fib:41:3`), since the engine names the
file, where the session module's are `<session>:L:C` (LV1 note (g)). Better than today for the file case.

### 3.4 The file on disk and the session's text

The session's view of a module is **what was last loaded**, not the file: `load` reads the file once and the text becomes the override;
an unrelated input later (an expression at the REPL) does not pick up a half-saved file. `reload` is how the disk's newer content comes
in; `watch` sends `load` on each save. `status` lists the loaded files with `stale: true` when the disk differs from the loaded text (a
content hash, read on `status`). On `:restart` the client restarts the process, which reads the files from disk: the loaded edits are on
disk already, so the replay of the session text covers only the REPL's definitions, as today.

### 3.5 All or nothing

A `load` is one input: the merged program (overrides, session text) is checked whole; every family's scheme and summary is compared with
the table (the LV2 rules); the module's definitions are diffed against the session's copy of the module (the text of the loaded file, or the
file as the base read it) by key (`rc/classify`'s keys, which the same reader gives for a file: `val:NAME`, `type:NAME`, `impl:P H`,
`text:SRC`); a changed `type:` key is `live/Layout`, a changed `defmacro` `live/Macro` (until LM2 swaps its users; since LM2 accepted), a changed `extern`
`live/Extern`, a changed or new `def` `live/Def` (until LT4), a removed function `live/Removed` (its stub stays and the running program
may call it: refused), `main` `live/Main`. If any rule refuses, **nothing is applied**: not the override, not the table, not a slot, and the
answer lists every refusal of the file, not the first (the transcript 6 of live-repl.md §5). If all pass, the changed families (with LM2:
the families whose expanded form changed) are emitted in one generation, one cut, one tracker, their addresses looked up, and then the
slots stored one after another. A caller between two stores can call a new `a` and an old `b`; the window is microseconds and the design
accepts it (LV2's per-family loop has it today); a generation pointer that every stub reads would close it at a load per call and is not
proposed.

### 3.6 The live-module rule and `--live-deps`

The rule stands as LT3 built it (`emit.live/program-module?`, `pkg.modules/live-roots`, `FIB_LIVE_ROOTS`): a project's own paths are live,
its libraries under `--live-deps` or `--live-deps=NAME,..`. `load` of a library module without it is `live/NotLive` with the flag in the hint
(3.2); the flag cannot be turned on in a running session (the library's bodies were compiled as direct calls: a restart). `status` prints
the live modules' count and the rule in force (`live: project paths (meshql 32 modules); libraries: none; --live-deps widens`), so the
refusal is not a surprise. The `require` input of a module the program lacks (the spec's `(require [gherkin :as g] ..)` of live-dev.md §4)
makes that module live by the same rule (it is loaded through the roots).

### 3.7 The first input: a warm engine (LE5)

The base is compiled by the driver's front end and the engine starts cold; the first input pays 10.7 s of front end (1.4). Two ways:

- (a) **Warm the engine before the first connection is accepted**: after `boot` and the start of the program's thread, `run-live` runs
  `inc/engine-front` on the base's text (the program's files, `mainmode "yes"`, `want true`) on the driver's thread, which is idle until
  a client connects; the expander's entries and the chain's states are then kept and the first input costs what any input costs. The app
  serves during it (its thread is its own); start-up to the first answered request stays 9.6 to 9.8 s; the first input, if it arrives
  during the warm-up, waits for it (the attach server is single-threaded anyway). Cost: the base's front end runs twice (CPU, 10.7 s, while
  the app starts), and the engine's memory (680 MB, 1.5) is reached before the first input rather than at it.
- (b) **Build the base through the engine** instead of the driver (one front end, the chain warm): the base's check through the chain is
  6.2 s against 2.7 (persistent-checker.md §8 reading 2), so start-up grows to about 13 s (estimate) and the emit is the same.

The design recommends (a): it keeps start-up and makes the first swap 1.4 s after LE1 and 0.3 s after LE3; (b) is simpler and costs 3 s
of start-up; the owner decides (5.3). Either way the `require` of the spec (45 s in live-dev.md §4) becomes its own cost: 335 modules
expanded and checked from the first new key, about 15 s (the measured `total_ms` 15,207 of that input), and the 33 s per input after it
becomes the same 0.3 s as any input.

### 3.8 `inspect` as a lookup (LE4)

`inspect NAME` answers from the session: a function of the running program (a family of the table, by name with the module prefix, or
the session module's) gives `generation`, `bodies`, `scheme` (the display text the table keeps, `Family.scheme` snd, which is the function's
type and not the `∀ς0. (fn :send () ..)` of the catalogue's wrapper that live-dev.md §6 item 5 saw) and `summary` (the parameters' modes),
in under 5 ms; a `def` of the session or the program gives its slot name and type from the last accepted program's catalogue (kept in the
session: `Made.c`); its **value** is an evaluation and stays `eval`'s (`inspect` with `value: true` runs the expression input, cost of an
expression). No front end runs for an `inspect` without `value`.

## 4. Packages

Sizes are fibber lines (files under 500, functions under 50). Each acceptance is executable and names its mutants (`scripts/mutant-live.sh
MODE` plants one fault in a copy, as live-repl.md §6 prescribes); the live stage of the gate runs them.

| Pkg | Scope | Files | Acceptance (executable) | Mutants that must be killed | Depends | Size |
|---|---|---|---|---|---|---|
| **LE1** def values in the Tables | `Tables.defs` by body name; `emit-def` replays a hit (items, constants, counter, value); the runtime's items parsed once; `FIB_LIVE_EMIT=verify` compares a replayed value with a fresh evaluation | `emit/program.fib` (`Tables`), `emit/defs/jit.fib`, `emit/defs/data.fib`, `emit/rt.fib`, `serve/livesess.fib` | `compiler/tests/live/swap.sh` unchanged and green; `compiler/tests/live/emit.sh` (new): a hello app with 6 constant defs (an array, a struct with a string, an `Option`, a scalar, a `str`, one holding a function value: LE2's guard) and a handler reading them: swap the handler three times and read every def through the handler and through `inspect value`: the same values each time; under `verify` byte-identical; the swap's `emit_ms` under a third of the first input's (the first evaluates, the rest replay); the meshql demo transcript (`scripts/live-demo.sh` on `live-dev`) with `emit_ms` under 1,200 on every input after the first (measured 1,001 to 1,076 in 1.6) | `def-cache-keyed-by-id` (keyed by the def id: a `(require ..)` input that adds a module before the session's shifts the ids: the test's `inspect value` reads another def's value), `replay-skips-constants` (the `@def.N` names not re-added: the next miss renumbers and the JIT's names-check refuses, or the value points at the wrong constant), `verify-ignores-value` (verify compares nothing: the keyed-by-id test under verify must refuse) | LV2 | 150 |
| **LE2** statics by name in live mode | `clo.CODE`, `vt.NAME` under `p.live`; the closure and vtable cells shared through `Tables`; the cut declares a static the base has | `emit/statics.fib`, `emit/program.fib`, `serve/livesess.fib` (`program-tables`), `live/extract.fib` (`own-static?` goes) | `stubs.sh`'s goldens unchanged (plain mode byte-identical); `emit.sh`: a def holding a named function as a value and a swapped body that calls through it: the base's closure object (one `clo.` static in `status`), the swapped body's call reaches the new generation; a method swap whose vtable is the base's (no `vt.` in the cut: the cut's item names are in the answer under `FIB_LIVE_DEBUG=cut`); a new instance in an input makes one new vtable | `closure-renumbered` (the counter kept in live mode: the def's function value calls the wrong code: the test's answer differs), `vtable-copied` (the cut copies the base's vtable: two `vt.` for one instance in `status`) | LE1 | 120 |
| **LE3** the queue mode | `Tables.resident`; a request answered by name when the code name is resident; the session fills the set from its index; registered specialisations requested by recorded type text; `FIB_LIVE_EMIT=verify` compares the cut's items with the whole emit's | `emit/mono.fib` or `emit/program.fib` (`program-request`), `emit/compile.fib`, `emit/live.fib` (`LiveBody.tys-text`), `serve/livesess.fib`, `serve/liveswap.fib` (`requests-of`) | `swap.sh` green, and under `verify`; `emit.sh`: the swap's `emit_ms` under 60 on the hello app (measured today 9,000 on meshql, estimate under 40 after); the generic test of `swap.sh` with a specialisation reached only through a resident caller (a planted "reached from main only" variant): both instantiations swap; a swapped body calling a library function at a new type (the body is lowered and exported, `status.values.resident` grows by one); the meshql demo with `emit_ms` under 100 on every input after the first and the whole transcript under `verify` byte-identical | `resident-by-family` (the family name checked instead of the code name: the swapped generation is never lowered: the 404 → 410 check fails), `skip-new-specialisation` (a missing specialisation answered by name: ORC's lookup fails: the library-at-new-type check), `literal-counter-per-program` (literal names restart per program: `names-check` refuses the second swap), `specialisation-not-requested` (only `main`-reached specialisations swap: the planted generic variant fails) | LE1, LE2 | 300 |
| **LE4** `inspect` as a lookup; expression inputs through the queue | `inspect` from the table and the last catalogue, `value: true` for an evaluation; the entry is the only body lowered for an expression | `serve/live.fib`, `serve/livesess.fib`, `serve/liveswap.fib` | `attach.sh` extended: `inspect handle` answers `generation`, `bodies`, `scheme` `(fn :send (Request) Response)` and no `stats`; wall under 20 ms in the test's harness; `inspect` with `value: true` of a def answers its `pr-str`; `(count [1])` on the hello app under 150 ms | `inspect-runs-input` (the answer carries `stats`) | LE3 | 80 |
| **LE5** the warm engine | `engine-front` on the base's files after `boot`, before `accept`; `hello` answers `warm: true/false` | `serve/live.fib` | `attach.sh`: the first input's `stats.reused` is 303 of 304 (the session module expanded, nothing else) and its `check_ms` under 300 on the hello app; start-up to the socket line unchanged within 10%; the meshql demo's first swap under 2 s after LE1 (measured 11.8 s with LE1 alone, of which 10.5 the cold front end) | `warm-skipped` (the first input's `reused` is 0) | LV4 | 60 |
| **LM1** `load`, `reload`, the module text overrides | the op and `:load`/`:reload`; `Sess.module-texts`; the engine's `texts`; the loader's override; `families-of-def` and `BodyRequest` with the module prefix; the module diff by key; `live/NotLoaded`, `live/NotLive`, `live/Removed`; all or nothing; `status` with the loaded files and `stale` | `serve/liveload.fib` (new), `serve/incr.fib`, `expand/modules.fib`, `serve/livesess.fib`, `serve/liveswap.fib`, `serve/liveclient.fib`, `emit/compile.fib` (`BodyRequest.module`) | `compiler/tests/live/load.sh` (new): a two-module app (`app.fib` requires `routes.fib`, a `-I` session): edit `routes.fib`'s `route` on disk, `load` it: `swapped: ["routes/route"]`, the next call answers the edit, a diagnostic of a bad edit is at `routes.fib:L:C`; a file with one refused definition (an arity change of a function `app.fib` calls) applies nothing, the answer lists it and the good one is not swapped; `reload` after a second edit swaps again, `reload` with no change swaps nothing; `load` of a file whose module the program lacks: `live/NotLoaded`; `project.sh` extended: a `load` of a library module under `--live` is `live/NotLive` with `--live-deps=NAME` in the hint, under `--live-deps` it swaps; an expression input between a disk edit and `reload` runs the loaded text (the override), not the disk's; the meshql scenario of live-meshql.md §7 (edit `on-item`, `:load src/meshql/restlette.fib`, curl 410) in `scripts/live-demo.sh` on `live-dev` | `partial-load` (the good definitions applied despite a refusal: the half-applied state is read), `load-reads-disk` (the override ignored: the disk-edit-then-expression check), `prefix-dropped` (the family named without the module prefix: `routes/route` is "defined", nothing swapped, the call answers the old value: live-dev.md §6 item 2 reproduced), `removed-ignored` (a removed function accepted: its stub calls a body the text no longer has: the test calls it and must be refused instead) | LE3 | 400 |
| **LM2** change by expanded form | a family changed iff the hash of its definition's expanded form (positions erased) differs from the one at its last accepted generation; `live/Macro` lifted for bodies whose expansion changed; the hashes in the table | `serve/livediff.fib` (new), `serve/livetable.fib`, `serve/liveswap.fib` | `swap.sh` extended: an input that changes `(:require [a :as x])` to module `b` with the same function name swaps the body that calls `x/f` (today it does not: the planted test fails on main); a redefined `defmacro` swaps its users and refuses nothing; a text change in a comment swaps nothing | `diff-by-text` (the text diff kept: the `require` test fails), `hash-with-positions` (positions in the hash: every input swaps every definition after the changed one: `swapped` lists them) | LM1 | 150 |

Order: LE1 (a day; measured effect 9.7 s → 1.0 s), LE2 (the guard LE1 needs), LE3 (the target), LE5 and LE4 (small), then LM1 (the
meshql scenario), LM2. LE1 through LE5 is about 700 lines; LM1 and LM2 550. Later, not in this list: a loader memo by file content (the
110 ms floor of every input), dependency-aware chain invalidation (an early module's `load` at 2.5 s), LT4 (`def`s in a loaded file), LT5
(`watch`), a generation pointer for atomic multi-family loads.

## 5. Risks, and what the owner must decide

Risks:

1. **A def value that names a closure** (2.2 item 3): LE1 without LE2 replays a `@clo.N` into a program where `N` is another closure. The
   experiment did not hit it on meshql-fib; the LE1 test plants one def that does, so LE1 ships with LE2 or with that def refused
   (`live/DefClosure`, a miss every time) until LE2.
2. **Queue mode trusts the resident index.** A name that is resident but whose definition the session changed without a new generation
   (the hole of 2.4.3) is declared and the change lost silently. Today the whole emit lowers it and the cut discards it, which is also
   lost silently; LM2 is the close, and `verify` mode finds both.
3. **The specialisation types as text** (2.4.4): a type that cannot be printed in the session module's terms (an input's private type)
   keeps its slot and is not swapped; the test shows it and the answer says it (`kept: ["f.g.$Priv_"]`).
4. **The chain's prefix-only invalidation** makes a `load` of an early module cost seconds (measured: `meshql.json` 2.5 s), and a `load`
   that touches a module two thirds of the way in, a second. The target of 2.6 is for the session module and the last modules; the
   design says so rather than promising 0.3 s for every file.
5. **Memory.** The engine is 680 MB on this program and the first input (or LE5's warm-up) reaches it; the kept generations are 210 KB
   each. Nothing here frees anything (LV3's `retire` is the plan for generations; the chain's 4,000-entry cap is the engine's).
6. **Two front ends of the base under LE5 (a)**: 10.7 s of CPU on the driver's thread while the app starts; on a loaded machine the
   app's own start-up competes with it. (b) avoids it for 3 s of start-up.
7. **The verify mode doubles each input in the tests** (1 + 9 s per input on meshql until LE3 lands, then 0.3 + 1); the gate runs it on
   the hello-sized apps and the meshql transcript stays a manual control.

Decisions for the owner:

1. **LE1 before anything else** (the def values in the `Tables`): yes or no. It is the one-day change that takes a swap from 9.7 s to 1.4.
2. **Statics named by code in live mode only** (LE2), so that plain emits and every golden stay byte-identical; or everywhere, one naming,
   every golden regenerated once.
3. **The warm engine (a) or the engine-built base (b)** for the first input (3.7).
4. **The session's view of a loaded file is the loaded text, not the disk** (3.4), with `reload` and `watch` the ways the disk comes in;
   or the disk always (every input re-reads the loaded files: simpler, and a half-saved file can break an unrelated input).
5. **A `load` is all or nothing with a per-slot window** (3.5), not a generation pointer.
6. **Change detection by expanded form** (LM2) as the rule for what a swap is, replacing the text diff; it is also what lifts `live/Macro`.
7. **`inspect` without a value is a lookup, with `value: true` an evaluation** (3.8); or `inspect` always evaluates (10 s today, 0.3 after LE3).
8. **The names**: ops `load`, `reload`; codes `live/NotLoaded`, `live/NotLive`, `live/Removed`, `live/EmitVerify`, `live/DefClosure`;
   `FIB_LIVE_EMIT=verify`; `FIB_LIVE_DEBUG=cut`. None is a core form or a builtin, and `compiler/` defines no function named `load` or
   `reload` as a top-level name of a module the session would capture (the self-hosting trap, CLAUDE.md): `expand.modules` has
   `load-program`, `serve.repl` has `cmd-load`; the op names are strings, not forms.

## 6. Built

- **Status 2026-10-15, LM1** (`compiler/serve/liveload.fib`; `Sess.texts` and `rc/front-texts`, the engine's `Job.texts` and
  `engine-front-texts`, the loader's `try-load-texts` (`expand.modules`: a `Loader.texts` read in place of a module's file, by module
  name); `BodyRequest.module` and `types.program/program-fun-in`; `LiveBody.fun` for every stubbed function with its `module`; the ops
  `load` and `reload`, `status.loaded` and `status.live` in `serve.live`; `:load FILE`, `:reload` and the `:status` line in
  `serve.liveclient`; `FIB_LIVE_LIBS` set by fibc in a project for the hint; the test `compiler/tests/live/load.sh` (load.py over
  load-app.fib and load-routes.fib, 39 checks), `project.sh` extended (`live/NotLive` with `--live-deps=acme/util` in the hint, the swap
  of `acme.util/util-value` under `--live-deps`), the mutants `partial-load`, `load-reads-disk`, `prefix-dropped`, `removed-ignored` of
  `scripts/mutant-live.sh`). Where it differs from section 3: (a) **the override is keyed by module name, not file path** (the loader
  finds the file as before and reads the text given instead; the path the user gave is kept for `status` and `reload`), so `:load` takes
  any spelling of the path and the diagnostics of a loaded module are at the loader's path (`src/routes.fib:L:C`); (b) **the session's
  copy of a module before its first load is the file as read once more right after the base is made** (`snapshot-base`, on the driver's
  thread before `main` runs: the base goes through the driver's front end, which keeps no text), and the program's module set is the
  base's (a module brought in by a later `require` input is `live/NotLoaded` to `load`); (c) **a new function or `impl` of a loaded
  file is accepted** (a family at generation 0, made resident as a session function is, named `routes/extra` in `defined`), a new type
  or macro too (the checker's rules apply), a new `def` or `extern` refused (`live/Def`, `live/Extern`; since LT4 a `def` is made or redefined, live-meshql.md 10, and a `def` whose type changes swaps its readers with the module's families); a `derive` or an `impl` of no
  head that changed is `live/Unsupported`; (d) **a changed function the running program never reached** (no registered body) swaps into
  the text only, as LV2 (a) says of a generic nobody instantiates; it is not in `swapped`; (e) the per-family rules refusal (`live/
  Signature`, `live/Summary`) is reported at the refused function's definition in the file (`FILE:1:1` until the live bugs' fix, below;
  still so for a family that is not the file's); (f) `status.live`
  is the rule's name and the count of live modules (`project paths (3 modules); --live-deps widens`, or `every module not named
  fib.*`), not the per-library listing of 3.6; (g) the main module's own file is loaded as an `eval` of the definitions that differ from
  the file as last loaded (the session's own inputs are not in that copy), with the file's `ns` clauses; a definition the file dropped is
  `live/Removed`; (h) `:type` reads the overrides as every input does (`rc/type-of-expr-texts`; it read the files until the live bugs' fix). Measured on the load test's two-module
  app on lilnas under a running gate (load average above 30): a `load` that swaps one function is `check 429 to 437 ms, emit 2 ms, jit
  2 ms` (the check is 59 modules, 8 expanded again: the loaded module and the 7 after it in the load order, 3.3); the meshql scenario of
  live-meshql.md 7 is a step of meshql-fib's `scripts/live-demo.py` on its `live-dev` branch (the on-item edit loaded, curl 410, the
  file restored and loaded again), guarded so that a fibc without the op skips it.
- **Status 2026-10-15, LM2** (`compiler/serve/livediff.fib`; `Table.hashes` and `with-hashes` in `serve.livetable`; `inc/Front.modules`,
  the expanded modules of a program that checked; `serve.livesess` `note-hashes` and `accepted-hashes`; `serve.liveswap` `changes-of`,
  `checked-input`, `merge-session`; `serve.liveload` `swap-checked`; `compiler/tests/live/swap.sh` extended (swap-app.fib requires
  `swapalt.one :as alt` and `swapalt.two :as two`; the `require` that moves `alt` swaps `alt-which` and nothing else, the redefined macro
  `seven` swaps `macro-user` and refuses nothing, a comment-only redefinition swaps nothing; `unit-livediff.fib` first), `load.sh` (a
  comment-only edit of `route` loaded swaps nothing), the mutants `diff-by-text` and `hash-with-positions` of `scripts/mutant-live.sh`,
  `prefix-dropped` moved to the hash key). The hash of a definition is over its expanded form printed without positions (syntax.print),
  with three normalisations section 2.4 did not name: (a) **`alias/name` resolved to the alias's module** (the expander leaves an
  ordinary call `x/f` as written; the checker resolves it, so without this the `require` input changes no expanded form); (b)
  **gensyms renumbered in their order in the form** (`#p.N` counts across the module, so a redefinition, which moves the definition to
  the end of the session's text, would renumber every later macro use); (c) **the module's `:use`s appended** (an unqualified name a
  used module gives is resolved after expansion too; a changed `:use` swaps every body of the module, an over-approximation). Keys are
  `f.PREFIX.NAME` and `impl:PROTO HEAD`; only the main module and the live modules are hashed. Where it differs: (d) **the base's hashes
  come from the warm engine** (LE5: the base goes through the driver's front end, which keeps no expanded forms); a session that was not
  warmed makes them on its first definition input (one more front end on the session's text); (e) **a redefined macro keeps its place**
  in the session's text (`merge-session`; serve.replcore `merge-defs` puts a redefinition last, which would expand its users before it:
  `unbound name`); a checker's diagnostic in such an input is moved to its line in the input (serve.liveswap `input-diag`; it was off
  by the macros kept in place until the live bugs' fix); (f) a changed `type:` or `extern` is
  still refused by its text (the rules of 3.5 run before the program is checked, so the expanded form is not there yet; moving them
  after the check is not cheap), and a `def` of a loaded file whose text changed is redefined only when its expanded form did too
  (key `d.PREFIX.NAME`, `Changes.defs`; a comment or layout edit keeps the value; a `def` with no recorded hash is redefined; since
  the live bugs' fix: it was by text). A `def` of an input at the REPL is still redefined by its text (Clojure's `def` re-evaluates),
  and so is one of the main module's file (a load of it is an `eval` of the definitions whose text changed); a `defonce` and a `def`
  of one value expand alike, so changing one to the other in a loaded file redefines nothing; (g) found on the way: a session's program does not find a module
  beside the main file that the base found there (`load/Missing`): swap.py and stress.py start swap-app.fib with `-I` its directory. (h)
  A redefinition with the text the definition has is accepted and swaps nothing (LV2 swapped it again): stress.py's swaps after the
  retirement now start from the other body. (i) A macro that quotes positions into its expansion (`expect` of fib.test) makes its user change when
  the user moves in the session's text: a redefinition moved to the end swaps the definitions after its old place that quote positions
  (test.py: redefining `classify` swaps `specs` too, whose reported positions were stale before).
- **Status 2026-10-15, LE5 and LE4** (`serve.live` `warm-engine`, `serve.livesess` `warm`; `serve.live` `op-inspect`, `serve.liveswap`
  `family-info` and `def-info`, `serve.liveclient` `:inspect`; `compiler/tests/live/attach.sh` extended, the mutants `warm-skipped` and
  `inspect-runs-input` of `scripts/mutant-live.sh`). LE5 is 3.7 (a): the socket line is printed and `main` started first, then the
  engine's front end runs on the session's text (the base's files and the loaded texts, `mainmode "yes"`, `want true`) on the driver's
  thread; a client that connects meanwhile waits in the listener's queue, and `hello` answers `warm`. Where it differs: (a) no warm-up
  under `--trace` (every child of `fibc cases` runs so, and the engine's macro sessions would trace on standard error) nor for a program
  that ends within 100 ms of its `def`s being made (a run nobody attaches to would otherwise wait for the warm-up to exit); `warm` is
  then false. LE4 is 3.8 with these choices: a function answers `kind`, `generation`, `bodies`, `scheme` (the table's display text) and
  `summary` (each parameter `borrowed` or `owned`, with ` escapes`); a `def` answers `kind`, `type` (the last accepted program's
  catalogue, `DefState.types`) and `slot` (`live.d.NAME` or `def.slot.N`; none for static data); a name that is neither is
  `unknown-name`; `value: true` is the old `inspect` (an expression input, with `stats`). The expression input needed no change: since
  LE3 its only request is the entry, and `main` and everything resident are answered by name. Measured on lilnas (attach.sh's app.fib,
  58 modules): the first input `reused 57, expanded 1, check_ms 5` (cold: `reused 0, check_ms 1013 to 1222`); `inspect` 0 ms in the
  probe's harness; `(count [1])` 31 to 54 ms. meshql-fib's `scripts/live-demo.sh` (305 modules): the first swap 335 to 362 ms wall
  (`check 157 to 172 ms`; 9,019 ms on the same machine without LE5), start-up to the first HTTP answer 8.51 to 9.07 s (8.42 s
  without); `hello` waits for the warm-up when it comes first (8.8 to 9.7 s). The demo's check "a session def is accepted once and
  refused again (live/Def)" fails with and without LE5: it predates LT4, which made a `def` redefinable.
- **Status 2026-10-15, live bugs** (one commit each; the tests and the mutants named are the pins). (1) A load's `live/Signature` or
  `live/Summary` refusal is at the refused function's definition in the file (serve.liverules `Refusal.family`, serve.liveload
  `family-pos`; load.py, the mutant `rules-at-file-start`). (2) `:type` reads the loaded texts (`rc/type-of-expr-texts`; load.py, the
  mutant `type-reads-disk`). (3) and (4) are live-meshql.md 10 (LT6 (b), (d); LT5 (a)). (5) A diagnostic of an input that redefines a
  macro is at the input's line (LM2 (e); swap.py). (6) A `def` of a loaded file is redefined by expanded form (LM2 (f); unit-livediff,
  load.py). (7) **The first redefinition of a function nothing reached answers `defined`, not `swapped`, and that is left as it is**:
  such a function has no registered body (the base pruned it: `main` does not reach it), so it is in no family of the live table and
  there is no slot to patch; the redefinition changes the session's text only (as 6 (d) says of a load), and the first program that
  reaches it (an expression input, or a later definition that calls it) lowers it from that text and registers it, after which a
  redefinition is a swap (`generation` 1). Measured on a probe (`unused`, never called by `main`): `(defun unused ..)` answers
  `defined ["unused"]` at generation 0, `(unused 1)` then answers the new body's value, the next redefinition `swapped ["unused"]` at
  generation 1; with `(unused 1)` first, the first redefinition is already `swapped`. Answering `swapped` would name a swap that did
  not happen (no generation, no patch); the answer is the truth of what the session did.

- **Status 2026-10-15, LE2** (emit.statics `statics-closure`/`statics-vtable` with `by-content`, `program-static?` now the quoted forms
  alone, `shared-static?`; serve.livesess `export-resident`, `shared-statics`, `note-cut`; serve.livesplit `code-static?` (its `code-refs` went with serve.livereach, section 7)
  over the base; serve.live `status` with `statics: true` and `cut` in an input's answer under `FIB_LIVE_DEBUG=cut`; the test
  `compiler/tests/live/statics.sh` (statics.py over statics-app.fib, 28 checks), the mutants `closure-renumbered` and `vtable-copied` of
  `scripts/mutant-live.sh`). In live mode a closure constant is `clo.CODE` (`clo.f.inc1.owned`) and a vtable `vt.NAME`
  (`vt.Shape.vt.Sq`); plain mode keeps `clo.N` and `vt.N`, and a plain emit is byte-identical (the golden checks of the passes, stubs.sh,
  and `emit` of three programs compared with the build before). The base's closures and vtables join the session's index, a cut
  declares them, and a new one (a closure of a function no program made a value of, the vtable of an instance an input adds) is exported
  with the data and declared by every later cut. Where it differs from 2.2 item 3: (a) **the closure and vtable cells stay the
  program's own**; the names agree by content, which is what the sharing was for, and a program renders only the statics it uses: the
  session's cells would put into every program a closure whose code the queue mode does not lower there, and the module a constant def
  is evaluated in (emit.defs.jit) would name it; (b) **a def value that names a closure or a vtable is still not kept by LE1's cache**
  (emit.defs.cache): it is evaluated by every program, and since LE2 the value names the base's closure, which the cut declares (a def
  read through its live slot is the base's value anyway). Keeping it would put `@clo.CODE` into a later program's constant-evaluation
  module, which has neither the static nor its code; a replay that also brings the closure and its code is the lift; (c) a closure of a
  generation's literal (`clo.l.NAME.gK.N`) goes with that generation's code (never exported), as before. Measured on meshql-fib's
  `scripts/live-demo.sh` (4 runs each, the build before and after interleaved, lilnas at load 2.5 to 10): the swaps' `emit_ms` median 45
  before and 41.5 after (39 to 55, 39 to 62), the wall time of a swap input median 320 ms before and 325 after; no change past the noise.


- **Status 2026-10-17, the loader memo** (the "later" line of the order above; `compiler/expand/parsed.fib`, `Loader.memo` and
  `try-load-memo` in expand.modules, `Engine.parsed` and `Stats.parsed` in serve.incr, `"parsed"` in an answer's `stats`;
  `compiler/tests/live/parsed.sh`, the mutants `parse-memo-off`, `parse-memo-by-name`, `parse-memo-by-length` of
  `scripts/mutant-live.sh`). The engine keeps, by file name, the text a file had when it was last parsed, its hash, its forms and
  whether they hold a regex literal; a load still reads every file (and finds it under the roots as before), and parses only a file
  whose text differs byte for byte from the kept one. The module keys take the text's hash from the same entry, so the files are
  read once per input, not twice, and hashed only when parsed. Nothing else changes: the keys, the expansion, the check and the
  rules see the same text as before. Measured on lilnas, meshql-fib's `scripts/live-demo.sh` (305 modules), the build before and
  after interleaved, two runs each at load average 3 to 5: `load_ms` 92 to 126 before, 5 to 9 after, on every input; the wall time
  of an input (the REPL client's) 241 to 415 ms before (the swaps 319 to 415), 98 to 154 after (the swaps 134 to 154); the refusal
  156 to 161 ms before, 29 to 32 after; the restlette `load` 463 to 475 ms before, 325 to 330 after. On the load test's app (59
  modules) `load_ms` 9 to 13 before and 0 after. Start-up and the first attach are unchanged (8.3 to 8.9 s): the warm-up parses
  every file once.

## 7. Retirement against what running state holds (2026-10-15)

**The bug.** A retirement (serve.liveretire, at quiescence) frees the code of every generation that no slot names and that no parked
thread can be in. Which generations a parked thread can be in was decided by following, from each family's bodies, the direct calls
and the closure constants and literals of the family's own, over the program's families alone. A thread can reach its pause point by
other calls: through a function value read from the heap (an atom or a `def` holding a function, a stored handler), a `dyn` method,
or a library function that calls back. Such a thread parked inside generation K of a family the analysis did not mark, the family was
swapped, the retirement freed K, and on `resume` the thread returned into freed code. Reproduced by `compiler/tests/live/retire.sh`
(retire-app.fib: the worker's `step` calls `(@hook)`, `hook` holds `park`, `park` calls the pause point; `step` swapped while the worker
is parked in it, then `retire`, then `resume`): before the fix the retirement answered `removed 1` with `step` not pinned, and the
program died with `fatal signal 11` on the resume. The def values of LT4 had the same hole: an old value whose slot a body read before
parking is held only while that body's family is pause-bearing (serve.livedefs `slots-read-by`).

**What was not broken.** A closure made by a swapped generation and held by running state (in an atom, in a `def`, run by a spawned
thread) points at a literal of that generation, and a generation whose code part holds a literal, a closure constant or a vtable is
never retired (serve.livesplit `pins`, `pinned (closures)`). retire.sh checks this too: the closures of a retired family's old
generations answer after the retirement, and the thread spawned by one counts on.

**The two choices, and the one taken.** (a) Refuse the swap (`live/*`) when it would strand a frame: the swap cannot know it, since
which body a thread will be parked in is decided at the pause, not at the swap; a swap refused for every body that may reach a pause
point through a call it does not name would refuse most handlers. (b) Keep the generation alive while something can be in it: by
counting (an entry and exit count per generation, paid by every call of every live body: against the owner's bar for swaps and for the
running program), or by pinning what a sound analysis says a parked thread can be in. Taken: (b) by pinning, the analysis made sound
(serve.livereach). It runs only at a retirement; a swap only records what its new code calls.

**The analysis (serve.livereach).** Over every definition the session holds (the base with the library and the runtime, every kept
module, a swap's data part as well as its code part), the graph of calls: a direct call is an edge to its callee; an indirect call of
lIR function type T is an edge to every definition of type T whose address some code or static takes (`@NAME` as a value). The
lowering calls a closure's code, a vtable's method and a task's runner with the type its definition has, so these are all the targets
an indirect call can have. Taking an address is no edge (a closure handed to `spawn` runs on another thread). Two indirect calls are
not followed: a stub's call through its slot (a family's stubs and bodies of every generation are one node, so that call is inside the
node) and a type walker's `trace.N` calling the visitor it is given (the runtime passes only its own visitors, never a task's runner,
whose type `(fn void (ptr))` the visitor shares; following it would make every body that releases an object pause-bearing). A closure
literal is a node of its own. The pause-bearing nodes are those from which `fib.os.live/pause-point` or `fib_live_ctl` is reached; every
generation of a pause-bearing family is kept (`pinned (pause-point)`). On the tests' apps the precision is what it was: stress-app's `f`
and swap-app's `handle` are still freed (stress.sh's checks unchanged), retire-app's `step` is pinned and `bump` (calls and arithmetic)
is freed.

**What it still keeps that a stack scan would free.** Every generation of a pause-bearing family, whether or not a thread is in it now,
and every generation holding a literal. Both lift with stack scanning (live-repl.md 3.3) and a heap walk from the def slots; neither is
built.

**Cost.** A swap records the calls of its new items (a walk of the cut it already made); the base's are recorded once at start-up. On
meshql-fib's `scripts/live-demo.sh` (305 modules, lilnas, three runs each, alternating, the same tree before and after): the swap of
`handle` end to end 360 to 471 ms before and 396 to 432 after (the second swap 322 to 413 and 336 to 364), the `load` of
`meshql.restlette` 0.5 to 0.7 s both, start-up to the first answer 10.4 to 10.6 s before and 10.1 to 11.6 after: within the runs'
noise, and a swap stays about 25 times faster than the 10 s start-up a recompile costs. The retirement's own cost on meshql is not
measured: its `dev/live.fib` does not reach quiescence (`pause` times out after 10 s, before and after), so it never retires.

**Test and mutant.** `compiler/tests/live/retire.sh` (retire.py over retire-app.fib; mk/tools.mk `sh-live-retire`, full gate); the mutant
`retire-dynamic-unfollowed` of `scripts/mutant-live.sh` (the analysis ignores indirect calls, as before) must make it fail.

## 8. The growth of a long session (2026-10-22)

**The complaint.** A `fibc run --live` session of fib-solid's worker grew about 14 MB per swap (RSS 491 MB after the first mesh, 595 after 10 swaps,
1001 after 40: `fib-solid/docs/repro/live-swap-rss.py`). Section 1.5 had estimated 210 KB per kept generation. `RAN` below means run in this session.

**Which of the four it was (measured).** (d), the engine's caches, and by design: not a leak (the memory was reachable), not the allocator.
- A reproducer in fibber alone (`compiler/tests/live/leak.py`: eight library-sized modules, `model` loaded 200 times, the main module after it):
  RSS 127 MB at start, 246 MB at swap 100, 1039 MB at swap 100 and 1862 MB at swap 200 with the unfixed stage 2: **8.2 MB per swap** (RAN, `F0` = the
  tree before this change). `FIB_ALLOC_DECAY_MS=0` (the large-block cache off) changed nothing (630 MB at swap 50 either way), so it is not the
  allocator's cache or glibc holding freed memory; `FIB_CHECK_CACHE=off` (no persistent checker) took the growth to 1 MB per swap (RAN), which named
  the cause.
- The cause: `serve.incr` keeps the expander's entry (`Engine.entries`) and the persistent checker's state (`own.chain`, `Memo.chain`) under the
  module key, which is a hash of the module's text and of every module before it. An edit of `model` gives `model` and every module after it a
  new key; the entries under the old keys were never looked at again but stayed until `entries-max`/`chain-max` (4000 each) cleared the whole map. A
  checker state is a copy of the library's tables (types.table copies an array at its first write after the copy), so a state costs megabytes, not the
  "what the module added" of own.chain's header (that measurement, 56 against 57 MB, was of a transcript of 431 states of a program with 253 small
  modules; a program that requires fib.json, fib.http and a geometry library has far larger tables). The macro runner (`macros.runner` `Session.mods`)
  keeps the same way a built JIT module per macro per module key.
- It is therefore retention by design, and the design is changed: **the engine keeps the keys of the last two requests of each file** (`Engine.recent`,
  at most 64 files) and forgets every expander entry, checker state, macro build and verdict whose key no recent request holds
  (`serve.incr/retain-keys`, `own.chain/chain-retain`, `macros.runner/keep-build`, section 10). The previous text stays so that editing back is still a cache
  hit; the library's state (`Memo.prefix`) is separate and stays. Unchanged modules are not expanded again (they never were: their keys are the same);
  the modules after the edited one are, because their key depends on it (section 2 of persistent-checker.md): that is the 0.2 s of `expand_ms` per swap.

**Result (RAN, `leak.sh`, 200 swaps).** Before: growth of the last 100 swaps 822.8 MB, `FAIL`. After: 246 MB at swap 100, 255 at swap 200, **9.5 MB**,
`ok`. fib-solid's repro (the plate): 491, 491, 492, 493, 494, 495 MB every 10 swaps to 40 with this section's change alone (against 595 after 10 and 1001 after 40), and with section 10's too 398 MB after the first mesh and 400 after 100 swaps (RAN).

**What is still growing.** After the fix of section 10 (the macro modules) the test's program grows 1.9 MB in the last 100 swaps (RAN, 205 to 207 MB): a
swap's generation (its ORC module with the cut's struct declarations, section 1.5) stays resident until a `retire`, which needs the program to reach a
pause point (section 7); a program that never pauses (the test's, fib-solid's worker) keeps them. Before section 10 the same run grew 9.5 MB: the other
7.6 MB were the JIT modules of the macro `try`, built again for every swap (below). Retiring on the next swap needs the stack scan of live-repl.md 3.3;
not built. A macro whose module depends on the edited one (its code can use it) is still built for each swap and its JIT module is never removed
(the macro runner has no removal); an unmeasured, small and bounded-by-edits cost in programs where that happens.
`Engine.verdicts` is pruned with the rest. The bound of the test (40 MB in the last 100 swaps) is twenty times the measured growth and a twentieth of the
unfixed one.

## 9. A removed function in a `load` (LM3, 2026-10-22)

**Before.** `load` of a module that had lost a `defun` (or a macro-made definition whose head starts with `def`, as fib-solid's `defparam`: the text
rules classify it as a function) was `live/Removed` whatever the rest of the program said; fib-solid's helper answered it by starting a new session (10 s).

**Now.** A removed function (and a macro-made `def`) is accepted when the program with the new text checks (`serve.liveload/load-module`). The
checker is the judge of "something still names it": a caller in the module, in another module, or in a session input (`(defun uses-helper ..)` typed
at the REPL is part of the program the engine checks) makes the program fail to check, and the load is refused with the checker's diagnostic
(`type/...`), nothing applied. When it checks, no text names the function any more, and the callers that did are in the same load (their expanded
form changed, so they are swapped with it). What stays: the function's stub, its slot and its last generation, because a running generation of a
caller, a closure made before, or a thread parked inside it may still reach it; they are retired with the others at a `retire`. What goes: the name.
Expression inputs and later loads see the file's text, so `(r/helper 1)` is unbound after the load; the answer lists `removed`; the session's copy of
the module is the new text. Putting the function back is an ordinary load: its family is still in the live table, so the same signature swaps it as a
new generation, and a different signature is `live/Signature` (callers compiled against the old one may still be running).
Still refused: a removed `impl` (its methods are in the dispatch rows of values that exist), a removed type (`live/Layout`), a removed function of
the **main** module's file (the session's own text keeps the definition: a `load` of the main file only evals what changed).
**Test.** `compiler/tests/live/load.sh` (load.py): the removal refused where the module calls it, where a session input calls it; accepted once the
session's caller is replaced; the call unbound afterwards and the app alive; put back; a removed `def` and a macro-made `def`, each refused while a
session input reads it; a caller changed and its callee removed in one load. Run against the stage 2 before the change: the three removal
checks and the three checks after them `FAIL` (`helper was removed ... [live/Removed]`).

## 10. Start-up, and a swap that expands again what did not change (2026-10-22)

**Where the 7.3 s went (RAN, `FIB_TIME=1`, fib-solid's `native/live/worker.fib` with the plate, lilnas at a load of 2 to 6, the same command before and after:
`FIBC=.. FIB_LIB=.. python3 startup.py`, a copy of `fib-solid/docs/repro/live-swap-rss.py` that stamps the events; seconds from the start of `fibc run --live`).**
The numbers in fib-solid's notes (attach 1.4, set-up 4-5, render 0.7, swap 1.2-3) were not the same partition; this one is by the session's phases:

| phase | s | what |
|---|---|---|
| base: `make-base` | 2.0 | the driver's front end on the file: expand 0.93 (macro builds 0.59 of it: `defparam` 0.1, `difference` 0.49), lower/infer/own 1.0; emit and JIT of the base 0.05 (the JIT compiles a function at its first call) |
| `wait-ready` and `grace` | 0.1 | `warm-engine` waits for the program's `def`s and 100 ms (a program that ends at once is not warmed) |
| warm-up: the engine's front end on the same program | **4.4** | `engine.load` 0.05, `engine.expand` 1.56 (macro builds 1.1: `defparam` 0.15, `difference` 0.76, `try` 0.19), `engine.check` 2.68 (the library's state 1.05, the chain over 69 modules 1.6) |
| the first input, `(mesh "{}")` | 0.85 | the input's front end, and the first call of the render path: the JIT compiles what it reaches |
| the first swap | 0.46 | `expand_ms` 0.22 (of which the macro `try` 0.19), `check_ms` 0.21 |
| total, start to the first mesh | 7.3-7.9 | |

**The biggest item was the duplicate.** The base was made by the driver's front end (`front/check-file`), which keeps nothing in the engine, and then the
engine's warm-up expanded, typed and decided the same 69 modules again so that the first input would not pay for it (LE5): 2.0 s and 4.4 s of the same work,
and the engine's pass is the slower (macros are built in two JIT sessions; the chain decides every unit whole and an all-owned twin of every function).

**The change.** `serve.livesess/checked-base` makes the base from the engine's own front end on the file's text (`inc/engine-front`, the real file name for
positions, FIB_LIVE_ROOTS applied as the driver does: `note-live-roots`); a program the engine does not take (an error, a missing file) goes to the driver's
front end as before, which reports it as `fibc run` does, and `FIB_LIVE_BASE=driver` takes that path always. The warm-up then finds every module but the
main one (the guard's functions are added to the text of the session, not of the base) expanded and decided, and costs 0.5 s instead of 4.4.

| (RAN, three alternating runs each) | before | after |
|---|---|---|
| start to the first mesh answered | 7.74, 7.77, 7.87 s | 4.13, 4.23, 4.45 s |
| the first swap (`source`), end to end | 0.48-0.52 s | 0.27 s |
| `live at` printed (the socket exists, the base resident) | 2.13-2.25 s | 2.8-3.1 s |

The socket exists later (the base now includes the work the warm-up did), but the first answer comes 3.3 s sooner; a client that connects at once waits in the
listener's queue as before.

**What loads precompiled today: the runtime only** (`compiler/emit/runtime.fib`, generated from `rt/*.lir`, parsed once per program). The library (`lib/`,
fib-solid's `src/`) is read, expanded, typed and decided at every start; nothing of the engine's state (the expanded modules, the macros' JIT modules, the
checker's tables) can be written down, since they are in-memory persistent structures with no serialiser. `FIB_LIRO_CACHE` is `fibc build`'s object cache for
the compiler's own units (mk/stage2.mk) and is not consulted by a live session. **What is missing for a precompiled library:** (1) a serialiser for the
expander's context after a module, for a module's expanded forms and for the checker's state after it (own.chain `Prefix`, types.table), keyed by the module
key that already exists; (2) the macro modules' lIR or object code under the same key (0.6 s of the base's expansion and the same again in the engine, `difference`
alone 0.5-0.75 s) loaded instead of checked, emitted and compiled; (3) object code for the library's functions so that the first call of the render path
(about 1.2 s of the 0.85 + the rest of the first mesh: the second `mesh` takes 0.04 s) finds them compiled; (4) the same objects for `fibc build --shared`.
Each is a design of its own; the work above removes only the duplicate. What remains of the 4.2 s: the engine's expansion 0.93 (macros 0.6) and check 1.76, the
first mesh 1.2.

**A swap expanded the macros of every module after the edited one.** A macro's module was kept under the macro and the *chain key of its module* (the text of
every module before it in the load order), so an edit of `model` rebuilt, by check, emit and JIT, the macros of every module that comes after it in the load
order (`try`, in `fib.ex`, which does not use `model`): 187 ms of the swap's 230 ms of `expand_ms`, and a JIT module that is never removed, 7.6 MB in 200 swaps.
`macros.runner/session-key` now keys a macro by the text of the modules its code can use: the closure of its module's requires, uses, exports and implicit modules
(`dep-closure`, the same set `helper-program` gives the macro's module), each by the hash of its own text (`session-note-text`, noted by the engine as it
expands a module). When a module of the closure has no text noted the key is the old one. Two builds of a macro are kept (`keep-build`: the current text and the one
before). Swap, `expand_ms` 220 to 36 ms and `total_ms` 440 to 225 ms on the worker. The test `compiler/tests/live/leak.sh` has a module `mac` after `model` that
does not use it and defines a macro: built once in 200 swaps (201 times before, 163-200 ms each).

**Tests.** The live suites on the final stage 2 (RAN): see the commit message. A project's live-module rule was the one regression found (`project.sh`:
the engine's program had no FIB_LIVE_ROOTS rule, so a library's module was live); fixed by `note-live-roots`.

## 11. A stack overflow in a live session (2026-10-22): not made catchable

Run (stage 2 of this tree, `(down 100000000 0)` evaluated over the attach socket of a live program, a recursion that adds one per frame):
`trap: stack overflow` on standard error, the connection closes without an answer and the process is gone (status 134). A depth of 100,000 answers.
This is the rule of docs/design/stack.md 3 and ADR 0009: an overflow is a SIGSEGV on the guard page, the handler runs on a signal stack of its own and ends in
`abort`; it is fatal in a `try` too. fib-solid's helper recovers by starting a new session (about 10 s, 0.7 s with a standby).

Why it is not made a caught failure in this slice, and what is needed:
- **From the signal handler** (continue at a recovery point on the faulting thread): the frames below the fault are abandoned, not unwound. The fault can be in
  any code: in `malloc` (the glibc arena of the thread stays locked and the next thread that gets the arena deadlocks), in the runtime's spinlocks (`fib.lock`,
  the atoms), in the middle of `fib.drop`/`fib.retain` or a free-list push (a reference count or a list head half-written, and the next allocation takes a
  block twice: the very crash of host-threads.md). A recursion that allocates in every frame reaches the guard inside an allocation a good part of the time.
  The handler can know only that the fault address is the guard, not what the thread was doing. Not safe, so not built.
- **As a trap that unwinds** (the catching convention, ADR 0009 mechanism b, which `--live` programs already use): the function must test the stack before it
  uses it. A prologue check in catching programs, `sp` against a limit, branching to the existing failure return with the message `stack overflow`, would make the
  overflow an ordinary caught trap with the audit exact. It needs (1) a per-thread limit readable without a call (a thread-local global in lIR, or the limit kept in
  a register-like slot of the thread's runtime state; `pthread_getspecific` per call is too slow), (2) the emitter to put the check in every function of a catching
  program that is not a leaf with a bounded frame, with the headroom the callee and the runtime need (the runtime's own functions use the stack too: the check must
  leave room for a `memcpy`, a `printf`, the unwinder), (3) a reversal of the rule "it cannot be caught" in docs/design/stack.md and ADR 0009, which the owner
  ruled; (4) a measurement of the cost (one load, compare and branch per call), which is paid by every `--live` program and could be left to a flag. The
  attach thread and the program's thread differ in stack size (both 256 MiB today); each thread's limit is its guard header (rt/thread.lir `fib.guard-enter`).
- **Not asked for here and not done:** a stack scan or a second process for the evaluation; both are larger.
