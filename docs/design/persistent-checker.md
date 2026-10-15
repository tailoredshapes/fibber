# The persistent checker: the library lowered, typed and decided once

Measured 2026-10-12 on the lilnas box (28 threads, shared with other agents, every heavy job under `nice`), stage 2 built from this tree by the seed v0.1.15.
Numbers are the quoted outputs of `FIB_TIME=1` (the phases of the front end, `time front.PHASE N ms`, new in this change) and of the commands named.
Branch `worktree-agent-a4d57e26ccac7cac2`.

Every compile type-checks, ownership-checks and lowers the library definitions the program reaches again, from scratch. The expander's context is kept
across programs (`serve.snap`, `expand.shared`); the checker's was not (dev-loop.md 8.1, 11.1). This record is what was measured, what was built, what
it costs and what it is checked by.

## 1. Where the time goes

`fibc emit FILE --time` with `FIB_TIME=1` prints `front.expand`, `front.lower` (the prelude and every module declared and defined into the globals),
`front.infer`, `front.elaborate` and `front.own` (the ownership pass); `front` is the sum with the read and load. `LIB` below is the library: the
prelude and the implicit modules (`fib.core`, `fib.seq`, `fib.coll`, `fib.print` and their parts), whose units are the same in every run. The split
of inference and of the ownership pass by module came from a temporary per-unit timer (not kept).

| Run | front | expand | lower | infer | elab | own | check+lower share of front | library part of infer + own |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| (a) `main` printing hello | 136 ms | 37 | 35 | 12 | 3 | 25 | 55% | all of it (75 ms) |
| (a) 20-line program (a record, a map, closures) | 391 ms | 189 | 37 | 37 | 4 | 90 | 43% | 151 of 168 ms (infer USER 4, own USER 13) |
| (a) 149-line program (`scripts/bench/windows.fib`) | 1097 ms | 878 | 44 | 41 | 5 | 84 | 16% | 125 of 174 ms |
| (b) the `types` unit group (40 modules, `UNITS_DIR=types`) | 5.9 to 6.4 s | 1.4 | 0.48 | 1.15 to 1.28 | 0.04 | 2.6 to 2.9 | 70% | 0.16 of 1.2 s and 0.48 of 2.8 s: 0.64 s |
| (c) one `emit-dump` input (compiler/tests/emit, no demand) | 0.67 s wall | | 55 | 126 | 4 | 347 | 79% | all of it (1,000 units: 909 decided) |
| (d) a REPL input (`(f 3)`) | | | ~40 | ~16 | 3 | ~30 | | all of it; the whole input costs 205 ms with the JIT |

Reading:

- (b) is the record's one negative result. The 4.1 s of check of the `types` group is the compiler's own modules (40 test modules and the
  compiler they import): user code to the checker, different in every group. The library is 0.64 s of it, about 10% of the front end: below the 25% that
  makes a snapshot worth building for `units-all`, `units-*` or a one-shot `fibc build`. Nothing here helps those; see section 6.
- (c) is the case the idea was made for. A tool that checks many programs in one process (`emit-dump --batch`, `types --batch`, the golden suites) and
  checks the whole library for each (its `library` is nil: no demand) spends 0.53 of 0.67 s on it, and 105 inputs cost 51.8 s in one process
  (`emit-tool --batch --sections fns` over `emit-fns.files`, measured before the change).
- (d) and the server: the library is all of the check, 60 to 120 ms of 205.
- A one-shot `fibc run` of the 20-line program takes 0.43 to 0.48 s wall; its check is 168 ms. There is nothing to reuse in one process, and the
  on-disk cache (section 6) would have to load the state faster than the 75 to 150 ms it saves.

## 2. What is kept, and what it depends on

The first stage: the prelude and the first `k` modules of the program (the implicit library) lowered, typed with no demand (every unit), elaborated, and
decided by the ownership pass. The second stage, for each program: a copy of that state continues with the program's own modules. `own.staged` has both;
`types.persist` the copies.

The state, and why a copy of it is a state (the audit the dev-loop record asked for):

| Part | Representation | Copy |
|---|---|---|
| `Globals`: nine tables | `(Table a)`: a Cell over an array that is copied by the first write to a shared one (`types.table`) | `table-copy`: a new Cell over the same array |
| `Globals`: `modules` | a table whose rows hold `Names`, six Cells | `modules-copy`: the rows rebuilt with fresh Cells |
| `Globals`: counters and indexes (`main`, `instance-index`, `option`, `form`, `vec`, `deref-proto`, `expr-count`, `elab-count`, `target`, `warnings`, `kernels`) | Cells of persistent values | a new Cell |
| `Checker`: `env` (2), `st` (the substitution: 4), `t` (the tables: 6), `errors`, `order`, `failed` | Cells of persistent maps and vectors | new Cells (`checker-copy`) |
| ownership: `TopState` (summaries, bodies, order, unit facts, the sets taken as values, the methods decided ahead) | a value | none needed |

No top-level mutable state exists in `types/`, `own/` (a `def` may not hold a Cell; the grep for atoms is empty). The constructors in `types.persist` are
positional: a field added to `Globals`, `Names`, `Env`, `Store`, `Tables` or `Checker` makes the file fail to compile.

What the library's results would depend on in the program, and how each is handled:

- **The functions the program takes as values, and the methods a method value may run** (`own.taken`): the library's units are all decided with an
  all-owned body as well (`top-start` with every function and method of the library), so the program's takes need nothing from the library again. The sets
  in the finished program are the true ones: the library's takes, the program's, and the library's method values against the program's instances
  (`methods-taken-of` with its first instance row).
- **Instances the program adds**: a library body never names one (it would not have checked), so the library's resolutions are final.
- **The expression ids**: the library is lowered first in every flow and keeps its ids. The elaboration made new nodes with ids from the counter of
  lowering, after the program was lowered; kept after the library it would shift the program's ids, and the closure names carry them (`l.f.main.17137`).
  The elaboration now takes its ids from its own range (`elab-id-base`, 2^40). Nothing prints them but the `tables` and `ast` dumps of nodes the
  elaboration made (no golden holds one).
- **Demand** (`types.infer.demand`): the server and the CLI check only the library units the program can reach. The kept state checks all of the
  first `k` modules. The program's modules are checked with the demand computed over everything; the checked program has more units (the library's
  all), and the emitter reaches by call, so the lIR is the same (section 4).

The key: `expand.shared/content-key` for the tools (the names and files of the first `k` modules, the text of each, the text of `lib/prelude.fib`), the
module-key chain of `serve.incr` for the server and the REPL (the content of the prelude and of every module up to the last implicit one). The compiler's
own build is the process: the kept state lives in memory and is never written, so a rebuilt compiler is a new process with nothing kept. A cache on disk
would need the build in its key (the hash of the executable), and was not built.

Where it is kept: `own.staged/Memo`, one prefix, in the tool's loop (`emit --batch`, `types --batch`), in the engine (`serve`, `repl`). Not kept: the
program's modules, anything of a program that did not check (a prefix that does not check is remembered as bad and the whole check is used).

## 3. Diagnostics

Positions come from the lowered forms, unchanged. The order of errors is that of the whole check: the `&` checks of the program's rows, inference, the
kernels, the ownership pass, the view rules (`stage-check` follows `own-check-lowered-with` step by step). The library's rows are not checked again
(`syntactic-check-from`, `views-syntactic-from`, `kernel-check-from`, `views-owned-from`, `elaborate-from`, each with the first row of the
program); a library that does not check is not kept. The checker's messages print unification variables by their order of appearance, not by id, so
the substitution carrying the library's variables changes nothing.

## 4. The controls

`FIB_CHECK_CACHE`: unset or `on`, the tools, the server and the REPL use the kept state; `off`, the whole check each time; `verify`, both, and a
difference is a failure. The command line (`fibc run|build|emit|check`) is off unless `on` or `verify`: one process has nothing to reuse.

1. `verify` in `fibc` (`driver.front`): the program checked both ways, the lIR of each compared byte for byte (the error text when it is rejected), and
   the kept state's sizes compared before and after (`state-print`: a run that wrote into it changes a size). `emit --batch` and `types --batch` do the
   same per input, on what they print (`#### CHECK-CACHE MISMATCH`, status 97). The server answers an internal error.
2. `compiler/tests/own/unit-staged.fib` (in `units-all`): ten programs through the kept state of the prelude and through the whole check (shadowing a
   library name, an instance for a library type, a library function taken as a value, a method value, a closure, three rejections), each twice; the
   state unchanged; a different key is a different state; the library key changes when a module's text, the prelude, a file name or the number of modules does.
3. `compiler/tests/persist/run.sh` (`tools`): the programs of that directory through `fibc emit` unset, on and verify; the same and the first 20 inputs of
   `emit-fns` through `emit --batch` under verify, against `off` and against one process each; a server whose prelude, and whose implicit module
   `fib.seq.consumers2`, is edited: the program is rejected after the edit and accepted again after it is undone.
4. `scripts/mutant-persist.sh`: planted faults, one at a time in a copy: the copy of the globals drops the instance index, the expression counter, the
   elaboration counter; the copy of the checker drops the substitution or the resolutions; the ownership pass starts without the summaries, without
   the program's takes, without the library's method values against the program's instances; the decided-ahead set or the known methods are lost; a
   table copy shares the table; the key is ignored; the library key ignores the prelude or the texts; the elaboration takes its ids from lowering; the
   program's elaboration starts at row 0. Results in section 7.
5. The goldens: `golden.sh` runs every 40th input of a batch suite as a process of its own and fails when the blocks differ (`GOLDEN_CONTROL`).
   `types-infer`, `types-top-*`, `emit-fns` and `emit-defs` pass with the kept state.

## 5. What was built

| File | What |
|---|---|
| `compiler/types/persist.fib` | the copies (above) |
| `compiler/own/staged.fib` | `Prefix`, `Memo`, `prefix-build`, `stage-lower/infer/own/check`, `staged-check`, `staged-typed`, the hint entry points |
| `compiler/types/driver.fib` | `lower-first`, `lower-more`, `phase-note` |
| `compiler/types/elab.fib`, `decls.fib` | `elaborate-from`; the elaboration's own id range |
| `compiler/own/{top,taken,syntactic,views,kernel}.fib` | the `-from` and `-of` forms of what ran over the whole program |
| `compiler/expand/shared.fib` | `content-key`, the hint the loader of a batch gives its check |
| `compiler/{emit,types}.fib`, `emit/dump.fib`, `types/dump.fib` | the batch tools use the kept state |
| `compiler/serve/incr.fib` | the engine keeps a memo; `typecheck` is staged, whole or verified; the program's modules through the chain (section 8) |
| `compiler/own/chain.fib` | the program-prefix memo (section 8): `staged-check-chain`, `prefix-extend` |
| `compiler/driver/front.fib` | `check-modules`: whole, `on`, `verify` |

## 6. Results

Before: the sources of 492da102 (tools built from them by the same F); after: this tree. Solo runs of the box (load 2 to 5), wall seconds.

| | before | after |
|---|---:|---:|
| `golden/emit-fns` (105 inputs, `golden.sh --tools .. --only`) | 56.6 s | 6.4 s |
| `golden/types-infer` (54) | 13.6 s | 1.9 s |
| `golden/emit-defs` (3) | 2.6 s | 1.8 s |
| `emit --batch --sections fns` over the 105 inputs, one process | 51.8 s | 4.3 s |
| `golden/tools/types`, `golden/tools/emit` (tool builds) | about 35 s each | about 35 s each |
| `tools/units-all` (79 modules, 3,218 assertions pass) | | 65 s (build 57, run 8); it does not use the kept state, and the library is 10% of its front end |
| server: `check` of an edited 1-function program, warm (median of 11) | 165 ms | 28 ms; the first request builds the state: 657 ms against 199 |
| REPL: cost of each input after the first (31 inputs against 1) | 188 ms | 87 ms; the first input +500 ms |
| `fibc emit` hello, `fibc run` hello (median of 5) | 188, 227 ms | the same (the kept state is off on the command line); with `FIB_CHECK_CACHE=on` 630, 756 ms |
| `fibc emit`, `fibc run` of the 149-line `windows.fib` | 1111, 1766 ms | the same; `on`: 1560, 2460 ms |
| `make -j6 quick` | | 53 s, GATE PASS (stamps already current were not rerun) |

The fixed point holds: `make fixed-point` built F and F3 and both emit the same lIR for `compiler/fibc.fib` (31,931,595 bytes).

The new floor, and what remains:

- The first program of a process pays 0.46 to 0.55 s for the state (the whole library checked and decided with an all-owned body for every function,
  once). A tool, server or REPL that checks fewer than two or three programs loses.
- A batch input costs about 40 ms in all (4.3 s for 105: the program's lowering, units and copying of the kept cells, and the emitter); the split
  between the check and the emitter was not measured.
- The REPL's 87 ms per input is no longer the check (`stage.*` notes: 1 to 2 ms); the rest (expansion, lIR emission, JIT) was not split.
- The one-shot command line and `units-all`, `units-*`, `fibc build` of one program have nothing to reuse. A cache on disk would have to load a state
  of about 35 MB of RSS (125 MB against 92 MB for hello) faster than the 75 to 170 ms the check takes; fibber has no serialiser for its structs, and
  loading the whole library's state to use 52 of its units (hello) is the wrong shape. Not built.
- The demand pruning of the one-shot path stays as it is; the kept state checks the whole library, which is why it needs the first-program cost.

## 7. The planted faults and the full runs

`scripts/mutant-persist.sh` (16 plants; 12 run by default, `no-intact-check` on request) with the unit program and `emit --batch` under verify:

```
KILLED instance-index (unit: failed, batch: failed)    KILLED tables-shared (unit: failed, batch: failed)
KILLED expr-counter (unit: failed, batch: failed)      KILLED key-ignored (unit: failed, batch: differs*)
KILLED elab-counter (unit: lived, batch: failed)       KILLED key-no-prelude (unit: failed, batch: differs*)
KILLED store-nodes (unit: lived, batch: failed)        KILLED key-no-texts (unit: failed, batch: differs*)
KILLED resolutions (unit: failed, batch: failed)       KILLED elab-ids (unit: failed, batch: failed)
KILLED summaries (unit: failed, batch: failed)         KILLED no-intact-check (unit: failed, batch: lived)
KILLED program-takes (unit: lived, batch: failed)
```

`*` the first run compared the batch output with the baseline's without allowing for the copy of `lib/` at another path, so "differs" was not a kill; the
unit program killed all three on its own. The script now normalises the path. Four plants SURVIVE and are documented in the script, not run by default,
because they change nothing a program can observe today: `method-takes` (no library body can take a method value without a resolved instance: the
emitter refuses it), `hoisted` and `known` (a method decided ahead of its place is decided again with the same result), `reelaborate` (a body elaborated
twice is the same body).

Full runs with `FIB_CHECK_CACHE=verify` (every program checked both ways in its own process, the lIR compared byte for byte, the kept state's sizes
compared before and after): `cases/ownership` 375 pass; `cases/modules` 32 pass; the quick stdlib sample 99 pass and 1 open, the same table as without;
all of `cases/stdlib` (`fibc cases -j 6`): 1,500 cases, 1,482 pass, 18 open, a table identical to the run without verify.
The command line runs the kept state through `check-staged` with a fresh memo, so these runs prove the second stage against the whole check, not a hit
across programs; the hit across programs is `emit --batch` (golden controls, `persist/run.sh`) and the server.

Default on or off: on for `emit --batch`, `types --batch`, `fibc serve` and `fibc repl` (every control above is clean); off for `fibc run|build|emit|check`
(it costs 0.4 s there); `FIB_CHECK_CACHE=off` turns it off everywhere, `verify` checks both ways.

## 8. The program-prefix memo (2026-10-15, package LV4 of live-repl.md)

The server and the REPL checked a program's own modules whole at every request (section 6: the REPL's 87 ms per input was not the check,
but a live session of a real app re-checks the app: live-meshql.md 1.4 measured 3.5 to 4 s per input for meshql-fib). `own.chain` keeps
the checker's state after each of the program's modules as well, not only after the library:

- **What is kept.** For a request whose modules are `m_0 .. m_{n-1}` (the first `k` the implicit library), the state after `m_0 .. m_i` for
  each `i` from `k` to `n - 2`, as a `Prefix` of own.staged (the same copies: types.persist), under the key serve.incr gives `m_i`
  (`module-key`: its text and name after the key of every module before it). The last module is never kept: it is the one a swap edits
  and the only one `main` is required of. `Memo` holds them (`chain`, at most 4,000 as the expander's entries; past that the chain starts
  again, the library's state stays) and the keys of the modules that did not check (`chain-bad`).
- **A request** (`own.chain/staged-check-chain`, the engine's `typecheck-staged`): from the longest kept prefix of its modules
  (`longest-kept`: the keys from the last but one backwards), the modules up to the last but one are checked one at a time and each state
  kept (`prefix-extend`); then the rest, the last module at least, by `stage-check` as before. So an edit of `m_i` re-checks `m_i .. m_{n-1}`;
  an edit of the last module re-checks it alone.
- **How a kept module is checked.** As the library is (`prefix-build`): every unit, no demand, and every function and method decided with
  an all-owned body as well (`extend-own`: a later module may take it as a value; the true sets of what is taken are recorded after, as
  `prefix-own` does). The server's `library-flags` flag only the modules that do not see the implicit library (`fib.*`), so an app's own
  modules were checked whole already, and for them this is the same check; a `fib.*` module the program pulls in (not implicit: `fib.tls`,
  `fib.json.schema`, `fib.http.client` ...) was checked on demand and is now checked whole. One that does not check whole (a unit nothing
  reaches that does not type) is remembered as bad, and the rest of the program is checked from the state before it with the library
  flags, which is the whole check's answer for it (the unit test's `chain-bad`).
- **The rest of the program** is checked with the library flags only when a module of it is the library's; when the kept modules reach the
  last but one, the demand (`demand-of`, a walk over the whole program) is not computed at all.
- **The ids.** As for the library (section 2): a module is lowered in the same order as in the whole run, so its expression, function, def
  and instance ids are the same; the elaboration's ids are in its own range; the binding ids of the elaboration's `or` bindings shift the
  later modules' (nothing the emitter prints holds one). The controls below compare the lIR byte for byte.
- `FIB_TIME=1` prints `stage.extend NS` for each module kept on the way and `stage.rest R of P program modules (J kept)`.

Controls, all with stage 2 of this tree (`F`, the 0.1.16 seed's build), 2026-10-15:

1. `compiler/tests/own/unit-staged.fib`, the steps `chain` and `chain-bad` (13 assertions): a program of three modules through the chain equals
   the whole check, twice; the last, then the first module edited (the keys change): the edited program's answer (the edit changes the lIR,
   checked); the first restored: its kept state used again; a middle module with an unreached unit that does not type: the chain answers as
   the whole check under the library flags (accepted), both reject it with every unit checked; `memo-intact?` covers the chain's states.
   Output: `1 scenarios: 1 pass` with the 39 steps `held`.
2. `compiler/tests/persist/run.sh F prefix` (part 4): the compiler's `syntax/` modules (12 modules, 2,087 lines with `util/text` and
   `app/main.fib`) as the app, three servers (`FIB_CHECK_CACHE` on, off, verify) over one transcript (a swap of `answer` in the last
   module three times, a function appended to `syntax/print.fib`, a swap again), by `--server emit`:
   ```
   ok prefix: every step of the transcript holds under on, off and verify (2087 lines, 12 syntax modules)
   ok prefix: the lIR of each step is byte-identical under on, off and verify
   ok prefix: and the last step's equals fibc emit's
     prefix timing: swap of one function in the last module, check ms (median of 3): on 1 (front total 28), off 236; cold on 884 1009, middle-module edit on 259 334
   ok prefix: the swap's check with the kept state is under 80 ms (1 ms; the whole check 236 ms)
   ```
   The swap's check is 1 ms against 236 for the whole program (the front end 28 ms in all: the load and the expansion of the one module).
   The first request costs 884 ms (the library's state plus eleven modules checked whole and kept); an edit of `syntax.print` (the third
   module in the order) re-checks the ten after it: 259 ms, above the whole check's 236, since each is decided with an all-owned body as
   well and a state is copied after each.
3. `compiler/tests/persist/run.sh F server` (part 3, extended): a program of two modules; its last module edited checks; its first module
   loses a function: rejected; restored: accepted (the kept state is under the text's key).
4. `scripts/mutant-persist.sh`: two new plants, `chain-key-ignored` (own.chain uses a kept state whatever the key: the unit program's
   `chain` step reads a stale answer) and `prefix-key-ignores-text` (serve.incr keys the chain by the module's name only: the server
   control, part 3 run with a `fibc` built from the planted copy, is accepted a program it must reject); the twelve plants of section 7
   run again with the chain tests in the unit program. The run of 2026-10-15 (`MUT_JOBS=3`, `FIBC` stage 2 of this tree), all killed:
   ```
   KILLED instance-index (unit: failed, batch: failed, server: not-run)   KILLED tables-shared (unit: failed, batch: failed, server: not-run)
   KILLED expr-counter (unit: failed, batch: failed, server: not-run)     KILLED key-ignored (unit: failed, batch: lived, server: not-run)
   KILLED elab-counter (unit: lived, batch: failed, server: not-run)      KILLED key-no-prelude (unit: failed, batch: lived, server: not-run)
   KILLED store-nodes (unit: lived, batch: failed, server: not-run)       KILLED key-no-texts (unit: failed, batch: lived, server: not-run)
   KILLED resolutions (unit: failed, batch: failed, server: not-run)      KILLED elab-ids (unit: failed, batch: failed, server: not-run)
   KILLED summaries (unit: failed, batch: failed, server: not-run)        KILLED chain-key-ignored (unit: failed, batch: lived, server: not-run)
   KILLED program-takes (unit: lived, batch: failed, server: not-run)     KILLED prefix-key-ignores-text (unit: lived, batch: lived, server: failed)
   ```
   `prefix-key-ignores-text` lives through the unit program and the batch (neither goes through the engine) and is killed by the server
   control alone, at the step it is planted for: `FAIL server: an edit of the first module was not seen (a stale kept state answered)`.
   The three key plants of the library (`key-ignored`, `key-no-prelude`, `key-no-texts`) now read `batch: lived` where section 7 had
   `differs*`: with the path normalised the batch cannot see them (its inputs share one library), and the unit program kills them, as
   section 7 said it did on its own.
