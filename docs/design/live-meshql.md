# Live meshql-fib: tests from a REPL attached to the running server

Status: **investigation and design, 2026-10-15; nothing here is built.** The owner's scenario: work on a real project (meshql-fib,
private: restlette REST and graphlette GraphQL over lacewing, stores fib-db-sqlite and fib-db-postgres, fib-hocon, fib-gherkin; its
tests run under `fibc test`, `scripts/test.sh`) from a live REPL attached to the RUNNING server: run a whole spec or one Gherkin
scenario from the REPL, change a function (a restlette or graphlette handler), see the change in the live server and in a re-run test,
with no restart. "Clojure ergonomics, Rust safety."

What this builds on: docs/design/live-repl.md (the design: one process, stubs and slots, the strict swap rule, the attach protocol,
packages LV0 to LV8), compiler/live/FINDINGS.md (the measured prototype and what broke), LV0 merged on main (`fibc run --live`,
`FIB_LIVE`: program bodies behind stubs and atomic slots, the catching convention), LV1 in progress on another branch (the attach socket
and `fibc repl --connect`). **Measured** means a command was run in this session and its output is quoted (section 1; the scripts and logs
are under `/tank/data/fibber-scratch/live-meshql/`). **Estimate** is arithmetic on measured numbers and says so. **Proposed** is design.
meshql-fib is private source: only short snippets are quoted, nothing of it is published.

Contents: 0 the short answer; 1 ground truth; 2 the acceptance scenario walked through the code; 3 running tests from the attached
REPL; 4 dependencies and the stub rule; 5 the prototype's blockers against meshql-fib; 6 the package list; 7 the end-to-end acceptance
script; 8 decisions for the owner; 9 not measured.

## 0. The short answer

1. **LV0 already compiles and runs meshql-fib's server and its whole spec suite in live mode.** `fibc run --live src/main.fib -- serve
   config.conf` serves the farm deployment with every request answered the same as the built binary; `FIB_LIVE=1 fibc test` passes all 59
   scenarios of `scripts/test.sh` step 1. Live mode costs nothing measurable on request latency (within noise of plain `fibc run`,
   section 1.3) and nothing on the suite (23.4 s against 23.6 s). What it does cost is the start: 9.9 s of front end and JIT where the
   built binary starts in 0.12 s; that is the price of a session, paid once.
2. **The dependencies are live too, by the current rule.** `expand.modules/sees-implicit` calls every module that is not `fib.*` a program
   module, so lacewing, hocon, sqlite, postgres, gherkin and the spec's own modules get stubs, not only meshql's: 700 of the server's 8,309
   bodies are behind a slot, 535 of them (76%) in dependencies; only fib-crypto-openssl escapes, because its modules are named
   `fib.crypto.openssl.*`. That is accidental, not designed. Section 4 says what it costs (swap-time checking of 8,600 program lines until
   LV4, a larger live table) and what it buys (a bug in a dependency fixed live: the cert scenarios found one in a forward once), and
   proposes the project's own paths as the default with `--live-deps` to widen it.
3. **The tests are a second program, and that is the design problem the live-repl design does not cover.** `specs/contract-spec.fib` is
   a program with its own `main` that reads `(args)`, builds a step registry and starts an HTTP server per scenario; `src/main.fib` is
   another program with another `main`. One live process must hold both. Three ways (section 2.3): a dev entry file in the project (what
   Clojure users do with `dev/user.clj`: no compiler work), `fibc run --live APP --with SPEC` (the loader takes the spec file under a
   module name of its own), or `fibc test --live` (the spec program is the session, the scenarios start the server). The recommendation
   is the first now and the second as the general form; it is the owner's decision (section 8).
4. **A test run in the live process is an `eval` with three things the library must offer**: an entry that takes the words instead of
   reading `(args)` and a report sink instead of `print` (`fib.test.run/run-main-into`, `gherkin.cli/run-into`: parameters, as docs/adr/0013
   wants; the session calls that entry, not the spec's `main`, 3.2), and the run on a thread of its own so the attach thread stays free.
   The `test` op (section 3.1) is that, with `--only`, `--tags`, `--name`, `--seed`, `-j`, `--isolate task|none` and the fib.test JSON report
   (schema 1, unchanged) in its answer, plus `generation_start`/`generation_end` and `stale`. Fork isolation is refused in a live process
   (the server's threads exist: `fork-start` refuses by rule, and the test-harness design says why); `task` is the default. The step
   registry is rebuilt on every run, which is what makes an edited step take effect: a step is a closure literal of `contract-steps`, and a
   closure made before a swap keeps old code.
5. **The gap between LV0 plus the prototype and the acceptance scenario is six packages, about 1,400 lines, after LV1 and LV2** (section
   6): LT0 the project's dev entry and `meshql.cli` (meshql-fib side, 40 lines), LT1 the library's argv override and report sink (fibber
   and fib-gherkin), LT2 the `test` op and `:test` with rerun and the stale flag, LT3 the project-paths live rule and `--live-deps`, LT4
   defs in a loaded file (new, and redefined with the same type: the prototype's two `def` blockers, an owner decision), LT5 `watch`
   (rerun on save; DV5's poll, not built). Two of the prototype's blockers need no package: "a long-running call blocks the swap" was the
   prototype's grace period, which the merged design replaced by keeping old generations; "closures keep the old body" is real for
   meshql-fib (the server's handler lambda and lacewing's resolver closures are made at start-up) and is answered by an app convention
   (keep what you store at start-up thin and named) plus the registry rebuild above, not by compiler work.

## 1. Ground truth (measured 2026-10-15)

Machine: lilnas, 28 threads, load average 2 to 5 (shared with another agent's work; the first latency run was perturbed by a concurrent
emit of mine and is not quoted; the second is). Stage 2 `F` built from this worktree (main at 6dfb13e7 plus nothing) by the 0.1.16 seed:

```
$ bash /tank/data/fibber-scratch/live-meshql/buildF.sh      # SEED build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F
rc=0 seconds=44.154972405
fibc 0.1.16
```

`FIB_LIB` is this worktree's `lib/`; every command below ran in `/tank/repos/tailoredshapes/meshql-fib` (project mode: `deps.fib` found
from the working directory, the six dependencies already in `~/.cache/fibber/git/...`, `fibc deps path` lists their `src` roots), with
`LD_PRELOAD` of libcrypto.so.3 and libsqlite3.so.0 as `scripts/test.sh` does.

### 1.1 What is stubbed: the rule and the counts

The rule (compiler/emit/live.fib `program-module?`, read): a body is stubbed when live mode is on, the module is not a macro module, and
`(mods/sees-implicit NAME (mods/implicit-lib))`, which is `(not (or (starts-with? ns "fib.") (= ns "Long") (= ns "Math") (has-name? implicit ns)))`
(compiler/expand/modules.fib:238). Nothing in it knows about `deps.fib`, the project's `:paths` or the library roots: a module is "the
library's" by its name alone. The dependencies' module names: `lacewing.*` (34 modules, 1,190 lines), `sqlite`, `sqlite.stmt`, `sqlite.ffi`
(277), `postgres.*` (12, 824), `hocon.*` (19, 2,434), `gherkin.*` (23, 2,381), `fib.crypto.openssl.*` (10, 761); the project's: `meshql.*`
and `main` (974 lines), `cert.world`, `cert.steps`, `cert.proposed` and the spec `main`s (506). So 8,586 lines are program modules under
the rule, 7,106 of them (83%) dependencies, and fib-crypto-openssl alone is library, by the accident of its name.

```
$ bash /tank/data/fibber-scratch/live-meshql/emit-probe.sh       # F emit FILE, plain and FIB_LIVE=1
emit src/main.fib plain: rc=0 8.49 s, 11537 lines        live: rc=0 8.43 s, 12937 lines     defines plain 7609, live 8309
emit specs/contract-spec.fib plain: rc=0 18.29 s, 16152 lines   live: rc=0 18.84 s, 17756 lines   defines plain 10171, live 10973
```

Unique slots (`@live.NAME` in the live lIR), by the module's first segment:

| program | slots | hocon | lacewing | meshql | postgres | sqlite | gherkin | cert | `fib.*` protocol methods of program types | main, serve |
|---|---|---|---|---|---|---|---|---|---|---|
| `src/main.fib` (the server) | 700 | 218 | 171 | 151 | 117 | 29 | | | 12 (`Eq` for lacewing's types, `Connection` for `sqlite`/`postgres`) | 2 |
| `specs/contract-spec.fib` | 802 | | 171 | 133 | 117 | 29 | 297 | 42 | 12 | 1 |

Of the server's 8,309 live-mode definitions, 700 are stubs (one `(global internal live.f.X ptr @f.X.g0)` and one stub body each: 8.4% of
the definitions, 12% of the lines) and 18 of them are `.owned` twins (a named function used as a value). 1,875 `f.fib.*` bodies (the
library and fib-crypto-openssl) are plain. The lIR of plain `fibc run` is unchanged by live mode existing (LV0's golden holds that).

### 1.2 Start-up and request latency

`scripts/server-probe.sh`: the farm deployment (`examples/farm/config.conf`, SQLite in a scratch file) started four ways, the time to the
first answered `GET /farm/api`, then from python3 over urllib: 100 `POST /farm/api`, 300 `GET /farm/api` (a list of 100), 200 `GET
/farm/api/ID`, 100 `POST /farm/graph` `getById`. Median and p90 in ms, one run each (the machine is shared: differences under 10% are noise).

| server | start-up | POST create | GET list | GET by id | GraphQL getById |
|---|---|---|---|---|---|
| `fibc build` (0.1.16 release), the binary | 0.12 s | 6.7 / 16.8 | 0.88 / 0.93 | 0.40 / 0.42 | 0.50 / 0.53 |
| `F run src/main.fib -- serve` (JIT, plain) | 10.4 s | 10.9 / 13.3 | 1.12 / 1.15 | 0.45 / 0.47 | 0.55 / 0.58 |
| `F run --live src/main.fib -- serve` | 9.9 s | 6.1 / 12.5 | 1.14 / 1.20 | 0.48 / 0.52 | 0.58 / 0.61 |
| `FIB_LIVE=1 F run src/main.fib -- serve` | 9.9 s | 5.8 / 12.3 | 1.16 / 1.24 | 0.49 / 0.52 | 0.61 / 0.66 |

Readings. The stub and the catching convention together cost under 10% on the JIT-run server's reads (0.45 to 0.48 ms by id, 0.55 to
0.58 GraphQL), which is inside the noise of one run; the JIT-run server is 10 to 30% slower than the `-O 2` binary on reads (the JIT runs
the program at `-O 0`: dev-loop.md 2.3). The POST medians vary 5.8 to 10.9 ms between runs of the same code (SQLite's WAL fsync dominates
a write; not a compiler cost). Start-up is the cost: 9.9 s against 0.12 s, of which the front end is 5.6 s and the lIR emit 1.8 s
(section 1.4); the rest is the JIT of 8,309 definitions. A live session pays this once, and again at `:restart`.

### 1.3 The test suite

`scripts/test.sh` step 1 (`fibc test specs/core-spec.fib specs/contract-spec.fib specs/props-spec.fib --seed 1 --jobs 4`), under
`flock /tmp/fibsuite.lock`, with three compilers:

```
== suite seed:   rc=0 19.8 s: total: 59 scenarios: 59 pass, 0 fail, 0 trap, 0 timeout in 3 spec files (seed 1)     # the 0.1.16 release
== suite F:      rc=0 23.6 s: total: 59 scenarios: 59 pass, ...                                                     # stage 2 of main
== suite F-live: rc=0 23.4 s: total: 59 scenarios: 59 pass, ...                                                     # FIB_LIVE=1 (each spec child in live mode)
```

Live mode changes nothing in the suite's rows or time. (The 4 s between the release and stage 2 of main is the compiler, not live mode:
the same without `FIB_LIVE`. Not investigated here.) One file alone, and one scenario alone (`--only Reading-an-envelope-by-ID`, an id
from `--list`, 41 listed):

```
== only F (all 41 of contract-spec): 23.3 s      F-live: 23.0 s
== only F (1 scenario):              20.0 s      F-live: 20.1 s     --format json: 20.0 s
```

**That is the number the live test runner exists for**: one scenario costs 20 s under `fibc test` today, of which the scenario is a few
milliseconds and the rest is the spec program's front end and JIT (1.4), paid again on every run; the 41 scenarios together add 3 s to the
same 20 s. In a live process the program is resident and a rerun is the scenario's own time plus the registry's rebuild (estimate: under
100 ms for the repository tier, which is in-process SQLite; an HTTP-tier scenario starts and stops a server, measured at 2 to 20 ms per
request above, so well under a second). The JSON report (`--format json`, schema 1 of test-harness.md 6.3, with `files`) is what the
`test` op carries unchanged (section 3.5).

### 1.4 The front end per input: the floor of a swap until LV4

`F emit --time`, the totals (per-module lines omitted), plain and live:

```
src/main.fib             plain: front.expand 2101 ms  front 5807 ms  lir.emit 1780 ms     live: front.expand 1754  front 5596  lir.emit 1764
specs/contract-spec.fib  plain: front.expand 3200 ms  front 7335 ms  lir.emit 9917 ms
```

The front end of the server's program is 5.6 to 5.8 s, of which expansion is 1.8 to 2.1 s and lowering, inference, elaboration and
ownership 3.5 to 3.7 s; the spec program's is 7.3 s. In a live session the library's state is kept (the persistent checker; the expansion
is memoised per module by `serve.incr`), so a swap re-checks the program modules: **estimate** 3.5 to 4 s per swap in meshql-fib until
LV4's program-prefix memo (live-repl.md 4.1 estimated 0.46 ms per line: 8,586 lines give 3.9 s, consistent with the measurement). With
LV4, the dependencies are a prefix the app never changes (they come first in dependency order), and a swap in `meshql.restlette` re-checks
`meshql.*` after it and `cert.*`: estimate 0.3 to 0.7 s; a swap in `cert.steps` alone, under 0.3 s. The emit and JIT of one function are
milliseconds (FINDINGS: 0.3 ms per swap at the lIR level; live-repl.md 1: 2 to 3 ms of code generation for a 15-line function). **LV4 is
on the critical path of this scenario**, not a convenience: a 4 s swap is tolerable, a 0.5 s one is Clojure.

**Measured with LV4 built (2026-10-15, live-repl.md 8, the table there)**: through `F serve`, a function appended to a module, the engine's
check time: the last module 8 to 9 ms (the front end 112 to 129 ms, the load of the 304 files most of it), `meshql.restlette` (module 247
of 253) 128 ms (front end 0.26 s), `meshql.store` (202) 0.9 s, `meshql.json` (137) 2.5 s, that is the whole check again; the first request
6.2 s against 2.7 before (every kept module checked whole and decided all-owned). The estimate above held for the handler (0.3 to 0.7 s
estimated, 0.26 measured) and for the session module; an edit early in the order is not helped, which the dependency-aware invalidation
noted there would fix.

## 2. The acceptance scenario, walked through the code

### 2.1 The server: where a swap is seen, and where it is not

`main/serve` (src/main.fib) reads the environment and the HOCON file, `config/build` makes the `App` (the graphlettes with their compiled
lacewing schemas and resolver closures, the restlettes with their JSON Schemas), `server/start` calls `(s/run-server (handler app) opts)`
and `main` parks in `hs/wait-server`. `fib.http.server.api/run-server` (library, read) spawns one acceptor task and one task per
connection; each calls the handler it was given. The handler is **the closure `(fn (req) (cors (if OPTIONS .. (try (route app req) ..))))`
that `meshql.server/handler` returns**: a closure literal of `handler`'s generation 0, holding `app`, made once at start and held by the
server for the life of the process.

So, in live mode, a request goes: the server's task calls the closure (old code, forever) which calls `cors`, `route` and `rl/error-response`
through their stubs; `route` calls `graph` and `rl/handle` through stubs; `rl/handle` calls `on-item`, `document`, `store/fetch`, the
`sqlite` driver, all through stubs. **A swap of any named function on that path is seen by the next request**: `rl/handle`, `on-item`
(a 404 becomes a 410: the transcript of section 7), `route`, `cors`, `gl/answer`, a store function, a lacewing executor function. **A swap
of `handler` itself is not** (its lambda is what the server holds), nor of `no-identity`/`header-identity` (lambdas stored in `App`), nor of
the resolver closures `gl/build` makes (src/meshql/graphlette.fib has 16 `(fn (` forms; the resolvers at lines 35, 44 and 56 are closures
over the query definition and the store that lacewing's compiled schema holds). This is FINDINGS item 4 of "what broke" and live-repl.md
3.3 ("a heap closure made by an old body keeps naming old code"), met in a real app on the first screen. It is not a defect of the
mechanism: the closure is a value made by the old body, and replacing values in the heap is not what a swap does. Those lambdas are thin
(one call each into named functions), so in practice an edit goes into the named function and takes effect. The convention for an app
that wants to be live (section 5, blocker 4): what is stored at start-up should be a named function (a closure constant jumps through the
`.owned` stub: 18 of them exist in the server's lIR) or should read an atom; a lambda stored at start-up is frozen until the expression
that made it is run again (`(reset! app-atom (config/build ..))` from the REPL, when the app holds its `App` in an atom; meshql-fib does
not yet).

The pool: lacewing's `execute-with` runs resolvers in parallel (`MESHQL_PARALLELISM`, `spawn` in lacewing.engine); each request's tasks
are short-lived, so quiescence for `retire` (live-repl.md 3.3) is reachable between requests only if the acceptor has a pause point, which
LV3 gives `run-server`.

### 2.2 The tests: how a spec runs, and what it shares with the server

`specs/contract-spec.fib` is `(ns main (:use fib.core fib.test.run cert.steps) (:require [gherkin :as g]))` with
`(defun main () -> i64 (g/run "contract/features/*.feature" (contract-steps)))`. Read in fib-gherkin v0.1.0 and lib/fib/test/run.fib:

- `g/run` is `(run-with pattern reg (vec (args)))`: `gherkin.cli/split-cli` takes `--tags`, `--name`, `--dry-run`, `--messages` (also from
  `GHERKIN_TAGS`, `GHERKIN_NAME`, ...) and leaves the rest to `fib.test.run/run-main-with suites argv`, which parses `--only SUBSTR`,
  `--seed N`, `-j N`, `--format plain|tap|json`, `--list`, `--isolate task|fork|none`, `--timeout SECS`, `--no-isolate`. **Both runners
  already have an entry that takes the argv explicitly** (`run-with`, `run-main-with`); only the outermost `run`/`run-main` read `(args)`.
- `gherkin.runner/suites` reads the feature files every run and makes a fib.test `Scenario` per pickle whose `run` closure is
  `(fn (seed) (live-run reg pk))`: make the world (`(g/world (fn [] (fresh)))`: a `World` with a fresh store, SQLite `:memory:` with one
  connection, or a fresh PostgreSQL table), run the `before` hooks, the steps, the `after` hooks (`(g/after (fn [w] (stop-server w)))`).
- `contract-steps` (specs/cert/steps.fib, 54 `g/given`/`g/when`/`g/then` calls, each with a `(fn [w ..] ..)` literal) builds the registry:
  **every step body is a closure literal of `contract-steps`'s generation**. The repository and searcher tiers call `store/*` directly (in
  process); the authorization, versions and schema tiers call `start-server-with`, which builds an `App` and `(server/start app "127.0.0.1"
  0 8)`: a real `run-server` on a free port, with the same `handler`, `route`, `rl/handle` the farm server runs, stopped in the after hook,
  and the steps call it over HTTP (`fib.http.client`).
- `fib.test.run/run-jobs`: `none` runs on the calling thread; `task` (the default) `spawn`s each scenario and `try-join`s it, `-j N` at a
  time, a trap being a `trap` row; `fork` (`--isolate fork`, implied by `--timeout`) uses `fib.os.process/fork-start`, which **refuses
  while the process has another thread** (lib/fib/os/process.fib: "this process has other threads; fork before the first async"): in a
  live process the attach thread, the server's acceptor and its connection tasks exist, so fork isolation is refused by the library's
  own rule, correctly. `scripts/test.sh` uses `--isolate fork --timeout 120` only for the proposed tier (1b, 2b).
- The report: `report` prints `json`/`tap`/`plain-summary` with `print` (the runtime's stdout, process-wide) and returns the status; the
  rows stream through `streamer` as they are made. `json`, `tap`, `plain`, `plain-rows`, `run-tasks`, `label`, `count-of` are public;
  `jobs-of`, `report`, `run-jobs`, `parse-args` are private.

What the tests share with the server in one process: the code behind every slot. The in-process server a scenario starts and the farm
server run the same `rl/handle` generation; a swap is seen by both on their next call. What they do not share: the stores (a scenario's
`:memory:` SQLite against the farm's file) and the `App` values. That is exactly what the acceptance asks: "see the change reflected
immediately in the live server and in a re-run test".

### 2.3 One process, two programs: which is the live program?

The server is `src/main.fib` (`ns main`), the tests are `specs/contract-spec.fib` (also `ns main`, another `main`); a live session is one
program (live-repl.md 2.1). Three ways to hold both, from the least to the most compiler work:

**(a) A dev entry in the project** (Clojure's `dev/user.clj`): meshql-fib adds `meshql.cli` (`serve`, today inline in `src/main.fib`) and
`dev/live.fib`, a `main` that starts the server from its arguments and `:require`s `cert.steps`, `cert.proposed` and `gherkin`, so the
session's program holds the app and the step registries. The REPL runs tests by evaluating
`(g/run-with "contract/features/*.feature" (cert.steps/contract-steps) ["--only" ID "--format" "json"])`; the `test` op (3.1) is sugar
over exactly that expression. Cost: 40 lines in meshql-fib, no loader change. Limit: each project writes its entry (a convention, as
Clojure's is).

**(b) `fibc run --live src/main.fib --with specs/contract-spec.fib`**: the loader reads each `--with` file as a module named after it
(`spec.contract-spec`; `expand.modules/spec-in forms "main"` is where a file's `ns main` is accepted, so the loader can take it under a
given name), its `main` becomes `spec.contract-spec/main`, an entry the session knows as a spec. The `test` op names the spec by file. Cost:
a loader change (about 60 lines in expand.modules and the driver), the live table keeps the spec entries. Benefit: no dev file, any spec
of any project; `fibc test --live` is this with no app (the scenarios start the server, as meshql-fib's do).

**(c) `fibc test --live SPEC..` only**: the spec program is the session; "the running server" is the one the scenarios start. It answers
the test half of the scenario but not "attach to the running `meshql serve` and hit an endpoint" unless the project's `serve` is callable
from the spec program, which is (a)'s `meshql.cli` again.

**Recommendation**: (a) now, because it needs LV1 and LV2 and nothing else, and (b) as the general form in LT2's second half; the `test`
op is the same in both (it takes a spec entry: an expression in (a), a file in (b)). The owner decides (section 8, decision 1).

## 3. Running tests from the attached REPL (proposed)

### 3.1 The `test` op

Added to the attach protocol of live-repl.md 2.3 (one JSON object per line, `id`, `ok`, `error.code`, `stats`):

| op | request | answer |
|---|---|---|
| `test` | `spec` (a file under (b), or omitted for the session's registered entry under (a)), `only`, `tags`, `name`, `seed`, `jobs`, `isolate` (`task` or `none`), `list` (bool), `dry` (bool); or `last: true` to repeat the previous request's selection | `report` (the fib.test JSON, schema 1, with `files` as `fibc test` writes it), `status` (0, 1 or 2 as `fibc test` exits), `generation_start`, `generation_end`, `stale` (they differ), `ms`, `stdout`, `stderr` (what the scenarios printed), `threads_after` (`fib.threads-live` when the run ended, for a scenario that left a server running) |
| `test` with `cancel: true` | | `cancelled: false` always in v1: a running test cannot be stopped (3.3); the answer says what is running |

Errors (`ok: false`, `error.code`): `test/NoSpec` (no entry registered and no `spec`), `test/NothingSelected` (status 2 from the runner:
"a run with nothing to run is not a pass", the same words), `test/NoFork` (`isolate: fork` or `timeout` asked: the hint names `task`),
`busy` (a test is running: the answer carries its `id` and elapsed time), `test/Trapped` (the entry itself trapped outside a scenario: a
before-all hook, a feature file that does not parse: the message is gherkin's).

### 3.2 How it runs

1. The attach thread takes the request, refuses if a test is running (`busy`), records `generation_start`, makes the **words** `fibc test`
   would pass a child (`driver.test/passed`: `--format json --seed N -j N [--only ..] [--list] [--isolate ..]`, plus
   `--tags`/`--name`/`--dry-run` for gherkin) and a **sink** that appends to a buffer, and `spawn`s the test thread; it answers nothing yet
   and goes back to its socket (a `status` or a swap may arrive meanwhile).
2. The test thread calls the entry with the words and the sink: under (a) the registered entry (a session `defun` the dev file names, or
   the expression the client gave at `hello`), under (b) the spec's suites. The entry is `(fib.test.run/run-main-into SUITES WORDS SINK)` for
   a fib.test spec and `(gherkin.cli/run-into PATTERN REGISTRY WORDS SINK)` for a gherkin one: the run reads the words instead of `(args)`
   and writes every byte of the report through the sink instead of `print`. Scenarios run as `run-jobs` runs them today: `task` spawns
   and joins each, `none` runs them on the test thread. The entry runs under `catch-run` (the live program is catching), so a trap outside
   a scenario is `test/Trapped`, not the app's end.
3. The test thread ends: the sink holds the report, the entry's status is the exit code; `generation_end` is read; the answer is sent on
   the connection that asked (with the request's `id`), as the `timeout` answers of live-repl.md 2.3 arrive later as events.

The library changes this needs (LT1, built): in `lib/fib/test/run.fib`, `run-main-into suites argv out` takes the words and the sink
(`out: (fn (str) unit)`) as parameters, and `run-main-with` and `run-main` are it with standard output; `report`, `streamer` and the
`--list` listing write through `out`. In fib-gherkin, `gherkin.cli/run-into pattern reg argv out` the same, `run-with` and `run` over it
(docs/design/fib-gherkin-lt1.patch, a release v0.1.1; meshql-fib pins v0.1.0 in deps.lock: a bump). Nothing changes for `fibc test`,
whose children call `run-main` and print. Why parameters and not atoms a session would set: a library keeps no global mutable state
(docs/adr/0013; the first cut used two atoms and the fitness check refused them). The consequence for the session: it cannot call an
unmodified spec `main` (that one reads `(args)` and prints), so the entry it calls is the explicit one. **The convention**: a fib.test
spec exposes its suites as a function, which `defspecs`/`defspecs-main` already define publicly (`(specs)` in a spec file, so
`(run-main-into (specs) WORDS SINK)` runs the file); a gherkin spec exposes its registry (`(contract-steps)` in meshql-fib) and the
session calls `run-into` with the feature pattern. Under (a) the dev entry registers that expression; under (b) `--with FILE` takes the
file's `specs` (or a function the request names) as the entry, and a spec file whose suites are not exposed is refused (`test/NoSpec`,
the hint names `defspecs`). One test at a time (3.3) stays a rule of the session: the sink is the session's buffer, not the library's.

### 3.3 Isolation in the live process

`--isolate task` (default): each scenario in a task, `try-join` makes a trap a row, `-j N` at a time. `none`: on the test thread, the first
trap ends the run as `test/Trapped` with the rows so far. `fork`: refused (`test/NoFork`): the live process has the attach thread and
the app's threads, `fork-start` refuses by its own rule, and a forked child could not see the slots' later generations anyway. What `task`
cannot do, as test-harness.md 5 says: stop a hang, survive stack exhaustion or out-of-memory. A hung scenario holds the test thread and
the `busy` state for good; `status` shows it (`tests: running ID for 312 s`); `:restart` is the way out. A `--timeout` in a live test is
therefore refused, not silently dropped; the client's hint says to run that scenario under `fibc test --timeout` in a process of its own.

### 3.4 Scenario state and threads

Each scenario makes its world from `(world (fn [] (fresh)))`: a fresh store, no server; the HTTP tiers start one on port 0 and the after
hook stops it. Nothing of a scenario survives it except what the app's own state keeps (the farm's SQLite file, if a scenario used it; the
contract's do not). A session `def` (live-repl.md 3.5) is visible to a step only if the step's text names it, so `(def w (cert.world/fresh))`
at the REPL and `(cert.steps/create-rows w table false)` are the way to poke at one step by hand. Threads: a scenario's server spawns an
acceptor and connection tasks and `stop-server` joins the acceptor with a drain; after a run `fib.threads-live` is back to the app's
count, and the answer's `threads_after` says so (a scenario that forgot its after hook leaves a server: visible, not fatal). A test run is
never quiescent, so `retire` during one refuses (3.3 of live-repl.md), which is right.

### 3.5 Output

The answer's `report` is the fib.test JSON document (schema 1 of test-harness.md 6.3; `fibc test --format json` adds `files`, and the live
answer adds it too, with one row per spec entry, `outcome: "ran"`) **unchanged**, so an agent that reads `fibc test --format json` today
reads a live run with the same code. `fibc repl --connect` prints a live run as `fibc test` prints a file: `plain-rows` and `plain-summary`
(the client renders the JSON with the library's own functions: the rows are the same text), with the summary's `replay:` lines rewritten
as `:test --only ID --seed N`. `fibc repl --connect --json` passes the answer through. Rows stream: with `-j 1` the sink receives each row
as `streamer` makes it, and the session forwards them as `{"event":"row", ...}` on the connection before the answer, so a 41-scenario
run shows progress.

### 3.6 A changed step, a changed handler, a changed def

- **A changed handler** (`src/meshql/restlette.fib`, `on-item`): `load` the file (LV5; or paste the `defun` as an `eval`): the diff of the
  session is `{on-item}`, same scheme and summary, swapped; the farm server's next request and the next test's in-process server both run
  it. Measured floor of the check: 1.4.
- **A changed step** (`specs/cert/steps.fib`, the lambda of one `g/then`): the diff is `{contract-steps}` (the lambda is inside it): one
  swap of a 54-literal function (signature `() -> (Registry World)` unchanged). The registry the previous test run built holds the old
  literals; **the next `test` calls `(contract-steps)` again** through the stub and gets new ones: the edited step runs. This is why the
  entry is re-evaluated per run and never cached (mutant `rerun-old-registry`, section 6).
- **A changed `def`** (`templates` in cert.steps, `rest-path` in cert.world, the SDL strings of cert.proposed; `columns` in meshql.store.sql):
  refused today by `live/Def` (live-repl.md 3.4): a program `def` is immortal and read count-free. For a spec file that is a real edit to
  refuse (a new step often needs a new constant, and `templates` is the map every searcher step reads). Section 5, blocker 1 and 2, and
  LT4: a `def` of the same type can be re-evaluated into a new immortal value and the slot stored with release order, because the old value
  is immortal and is never freed, so a reader that loaded the old pointer keeps a valid object; and a new `def` in a loaded file gets a
  slot in the new generation's module with its initialiser run once on the attach thread, the prototype's `live.types.append.N` pattern.
  Both are an owner decision because live-repl.md's rule says otherwise (section 8, decision 3).
- **A changed feature file** (`contract/features/*.feature`): no code changes; `gherkin.runner/suites` reads the files on every run, so the
  next `test` sees it with no swap at all.

### 3.7 A test running during a swap

A swap is one store into a slot; nothing waits for a test (live-repl.md 3.1, 3.3: old generations are kept, so there is no grace period
to block on). A scenario in flight keeps its step closures (old `contract-steps` literals) and the app functions it calls go through the
stubs: a step before the swap ran the old `on-item`, a step after it the new one. The answer says so: `generation_start: 3, generation_end:
4, stale: true`, and the client prints `; generation changed during the run (3 -> 4); :test to rerun` after the summary; `watch` (3.8)
reruns by itself. A swap of `contract-steps` during a run changes nothing of that run (the registry is a value the run holds). What is
not offered: pausing swaps while a test runs (an agent that swaps and tests in a loop would deadlock itself) or marking rows by
generation (a row's steps straddle it; the whole run is stale or not).

### 3.8 Rerun and watch

- `test` with `last: true` (`:test` with no words): the previous request's selection and seed, so "fix, rerun" is two keystrokes; the
  failing row's `replay:` line gives `--only ID --seed N` for a narrower rerun.
- `watch` (`:watch on [DIR..]`, LT5): the poll of dev-loop.md 4.2 (DV5, not built: `stat` once a second over the program's files, which the
  engine knows), on a change: `load` the file (LV5's all-or-nothing unit; a refusal is reported and nothing reruns), then `test` with
  `last: true`; answers arrive as `{"event":"test", ...}` on the watching connection. A feature file change reruns without a load. `:watch
  off`. Not inotify: vanilla, the dev-loop design's choice.

### 3.9 The REPL commands (`fibc repl --connect`)

```
:test [SPEC] [--only ID] [--tags EXPR] [--name RE] [--seed N] [-j N] [--isolate task|none] [--list] [--dry-run]
:test                     rerun the last selection
:watch on [DIR..] | off   rerun the last :test when a file changes (after :load of a changed program file)
:status                   adds `tests: idle | running ID for N s`
```

The words after `:test` are `fibc test`'s, so a person who knows one knows the other; `--format`, `--timeout` and `--isolate fork` are the
three it refuses, each with the reason.

## 4. Dependencies and the stub rule: cost and benefit

What LV0 does today (1.1): a dependency is a program module unless its modules are named `fib.*`. For meshql-fib that makes 76% of the
server's slots dependency slots.

Costs, measured or estimated: (1) start-up: none measurable (9.9 s live against 10.4 s plain, the JIT of 700 stubs is noise); (2) request
latency: none measurable (1.2); (3) **the check per swap**: until LV4 every program module is checked on every input, 8,586 lines, 3.5 to
4 s (1.4); with LV4 the dependencies are an unchanged prefix and cost nothing per swap, so this cost is LV4's absence, not the rule's;
(4) the live table: 700 or 802 entries with their schemes and summaries (live-repl.md 4.3), a few hundred KB, built once; (5) the
`live/Summary` rule applies to a dependency's functions too, which is right and free.

Benefits: a bug in lacewing, hocon or a store driver can be fixed and tried in the running app without a release of the dependency (the
proposed-tier scenarios found a fault in `meshql.graphlette.forward` once; the next one may be lacewing's); `trace` (LV6) reaches a
dependency's function; the test harness itself (gherkin, 297 slots) is swappable, which is how fib-gherkin would be developed against a
real spec.

Options: (i) keep the rule (everything not `fib.*` is live); (ii) **the project's own paths are live, dependencies are not, `--live-deps`
widens it to every library of `deps.lock` and `--live-deps NAME..` to some**: the loader knows a module's owner (`pkg.modules/search-dirs`
orders the project's roots before each library's; `owner-text` names the library a module came from), so `program-module?` can ask "is
this module under the project's `:paths` or test paths" instead of "is it named `fib.*`"; outside a project (`-I` only) the rule stays
the name rule; (iii) a `:live` key per dependency in `deps.fib`. The design recommends (ii): the default is the smaller live table and the
smaller swap-time check, and widening is a flag, not a config edit. It also fixes the accident that fib-crypto-openssl is library by name
while fib-hocon is program by name. Cost of (ii): LT3, about 120 lines; the name rule stays for `fibc run --live hello.fib`.

## 5. The prototype's blockers (FINDINGS.md) against meshql-fib

| blocker (FINDINGS) | hits meshql-fib where | options |
|---|---|---|
| 1. a `def` added after the session started is refused | a new constant in `cert.steps` or `cert.world` for a new step; a new `def` in a handler module | LT4: a new `def` in a loaded file gets a slot in the new generation's module, its initialiser run once on the attach thread (the prototype's appended-rows pattern); or keep refused and `:restart` (the session text is replayed) |
| 2. a redefined `def` is not applied (the slot keeps its object) | `templates` (every searcher step reads it), `rest-path`, the SDL strings, `columns` | LT4: same type, new immortal value, one release store into the slot, the old one leaked and counted (`status.values.leaked_bytes`); readers hold immortal objects, so no use-after-free; a `def` whose type changes stays `live/Def`. Owner decision 3 |
| 3. statics are never freed (a body with a string literal, a closure of a named function, or a table row is pinned) | every handler has string literals, so every swapped generation of `on-item` or `route` stays mapped | accept for a development mode: estimate 5 to 20 KB per swapped function (live-repl.md 3.3), a day of swaps is megabytes; `status` prints it; LV3's `retire` at a pause point (the acceptor between connections) frees what the escape rule allows; no package |
| 4. closures keep the old body | the server's handler lambda, `App.identity`, lacewing's resolver closures (2.1); the step registry (2.2) | the test runner rebuilds the registry per run (LT2); the app convention: a lambda stored at start-up is frozen, so keep it thin and put the logic in named functions (meshql-fib already does, by habit), or hold the `App` in an atom and rebuild it from the REPL (LT0 adds `(meshql.cli/rebuild! path)` as the example); no compiler package; a `live/FrozenLiteral` note at swap time ("handler's literal l.1 of generation 0 may be held by values made before this swap") is cheap and honest and goes into LV2's `swapped` answer |
| 5. a long-running call blocks the swap (the prototype's grace period waited for every call of the old body to return) | `serve` and `main` are in flight for the life of the process; a connection task in a slow handler | not a blocker in the merged design: LV2 keeps old generations and never waits (live-repl.md 3.1); `serve`/`main` swap into the session text only and take effect at `:restart` (`live/Main`); a slow handler finishes in the old body |
| 6. dependencies are not live | the opposite is true under LV0: they are (1.1) | section 4: the project-paths rule with `--live-deps` (LT3); the owner chooses the default |

Also from FINDINGS, not blockers but costs to keep in view: the catching ABI is the session's ABI (`--live` compiles the whole program
catching: measured at no visible cost on meshql-fib's requests, 1.2); keyword ids interned after the boot are untested; the rows of every
expression's lambdas accumulate in the session's type table (a leak per input: a long agent session should `:restart` daily).

## 6. The package list (prioritised, sized; after LV1 and LV2)

Sizes are fibber lines (files under 500, functions under 50). Each acceptance is executable and names its mutants (`scripts/mutant-live.sh
MODE` plants one fault in a copy, as live-repl.md 6 prescribes); the live test suite `compiler/tests/live/test.sh` joins `tools` in
`scripts/gate.sh --full` through LV2's stage.

| Pkg | Scope | Files | Acceptance (executable) | Mutants that must be killed | Depends | Size |
|---|---|---|---|---|---|---|
| **LT0** the project's dev entry (meshql-fib, not fibber) | `meshql.cli/serve` (moved from `src/main.fib`), `dev/live.fib`: a `main` that serves and `:require`s `cert.steps`, `cert.proposed`, `gherkin`; `(meshql.cli/rebuild! path)` holding the `App` in an atom so the handler reads it per request (the frozen-closure convention, 2.1) | meshql-fib `src/meshql/cli.fib`, `dev/live.fib`, `src/main.fib` | `scripts/test.sh` unchanged and green; `fibc run --live dev/live.fib -- serve examples/farm/config.conf` answers the farm-test's first query; `scripts/live-test.sh` (new): the acceptance script of section 7, run end to end once LT2 exists, its transcript diffed against the one in section 7 (timings masked) | `rebuild-not-read` (the handler keeps its first `App`: the script's "rebuild" step still answers the old route) | LV1 | 40 (+ the script 60) |
| **LT1** the words and the report sink as parameters (built) | `fib.test.run/run-main-into suites argv out`: the words in place of `(args)`, every byte of the report through `out` (`report`, `streamer`, `--list`); `run-main-with` and `run-main` over it; `gherkin.cli/run-into` the same (fib-gherkin v0.1.1, docs/design/fib-gherkin-lt1.patch); nothing changes for `fibc test`. Parameters, not atoms: docs/adr/0013 (3.2 says what the session calls instead of `main`) | `lib/fib/test/run.fib`; fib-gherkin `src/gherkin/cli.fib` | `compiler/tests/driver/test-cmd.sh` unchanged (44 checks); `cases/stdlib/8423`: `run-main-into` with `--only x --format json` and a buffer: the buffer is fib.test's document for the one scenario, status 0, and `run-main` reads `(args)` after; `compiler/tests/live/hooks.sh`: the collected document is byte for byte what `fibc run SPEC -- --format json --seed 1 -j 1` prints (plain too), nothing on stdout, `--list` and a selection that matches nothing (status 2) through the sink | `sink-ignored` (the report is printed anyway: the case reads an empty buffer), `override-ignored` (`(args)` parsed instead of the words: three plain rows in the buffer) | none | 60 (+ 15 in fib-gherkin) |
| **LT2** the `test` op, `:test`, rerun, stale | `serve/livetest.fib` (new): the request, the test thread, the override and sink, `generation_start/end`, `stale`, `threads_after`, the refusals (`test/NoFork` ..), `last`; the client's rendering through `plain-rows`/`plain-summary` and the `replay` rewrite; row events at `-j 1`; (b) of 2.3 as its second half: `--with` in the loader, `spec.X/main` as the entry | `serve/livetest.fib`, `serve/liveclient.fib`, `serve/repl.fib`, `expand/modules.fib` (for `--with`) | `compiler/tests/live/test.sh`: a 4-scenario spec (one planted failing step, one trapping, one printing) under a live hello app with a `(def hits (atom 0))` the scenarios bump: the JSON answer has the four rows with the statuses `fibc test` gives them, `stdout` holds the print; `--only` the failing row; swap the function under test (an `eval` of the `defun`), `:test` (rerun-last): the row passes; a `def` at the REPL read by a scenario; `isolate: fork` answers `test/NoFork`; a 2 s scenario with a swap sent at 1 s: `stale: true`, the rows show both generations' answers; a second `test` during it: `busy` with the elapsed time; `status` during a run answers within 100 ms; `--with` loads a spec file beside the app and `test` runs it | `stale-never` (generations not compared), `rerun-old-registry` (the entry's registry cached: the swapped step is not seen: a planted "step edit" in the test fails), `test-on-attach-thread` (the `status` during a run times out), `sink-shared-run` (two runs allowed at once: the reports interleave: the JSON of the second fails to parse) | LV1, LV2, LT1 | 500 (+ 60 for `--with`) |
| **LT3** the project-paths live rule, `--live-deps` | `program-module?` asks the loader's owner (project paths or test paths) when in project mode; `--live-deps [NAME..]`/`FIB_LIVE_DEPS`; the name rule outside a project | `emit/live.fib`, `pkg/modules.fib` (owner per module), `driver/args.fib`, `driver/commands.fib` | `compiler/tests/live/stubs.sh` extended: a fixture project with one git dependency (the `scripts/mutant-deps.sh` fixtures): `emit` under `--live` has stubs for the project's modules only, under `--live-deps` for the dependency's too, the counts fixed in a golden; outside the project (`-I`), the name rule as today (the `hello-stubs.golden`) | `deps-stubbed-by-default` (the golden's count), `project-paths-not-stubbed` (the fixture's own module has no slot) | LV0 | 120 |
| **LT4** defs in a loaded file | a new `def` in a loaded program file: a slot in the generation's module, the initialiser run once on the attach thread, immortalised; a redefined `def` of the same type: new immortal value, release store, the old leaked and counted; a changed type stays `live/Def` | `serve/livetable.fib`, `emit/live.fib`, `emit/inits.fib` | `compiler/tests/live/defs.sh`: a program whose handler reads `(def limit: i64 3)`; `load` the file with `limit` 5: the next call answers with 5 and `status.values.leaked_bytes` grew; a new `def` added and read by a swapped body works; a `def` whose type changes is refused `live/Def`; under the LV3 stress (8 threads reading the def while it is redefined 1,000 times) every read is 3 or 5 and nothing crashes; `scripts/tsan.sh` on the stress reports nothing | `def-slot-plain-store` (TSAN), `old-def-freed` (the stress crashes), `init-run-twice` (an initialiser with a side effect counted once) | LV2, LV3 | 250 |
| **LT5** `watch` | the poll (DV5's shape), `load` on change, `test last: true`, events on the watching connection; a feature file change reruns without a load; `:watch on|off` | `serve/livewatch.fib`, `serve/liveclient.fib` | `compiler/tests/live/watch.sh`: touch the handler's file: within 1.5 s the event carries a rerun whose row reflects the edit; touch a feature file: a rerun with no `swapped`; a file edit that is refused: the refusal event and no rerun; `:watch off`: a touch reruns nothing in 3 s | `watch-no-load` (the rerun runs old code: the row does not change), `watch-reruns-on-refusal` | LT2, LV5 | 200 |
| LT6 `fibc test --live` | the spec program as the session with no app: `fibc test --live SPEC` starts one live process for the files, prints the socket, and `fibc test` with `--connect` sends `test` requests to it instead of spawning children | `driver/test.fib`, `serve/livetest.fib` | the `test-cmd.sh` rows reproduced through a live session, the second run of one scenario under 1 s where the first is 20 s (1.3) | `child-spawned-anyway` (the second run is 20 s again) | LT2 | 200 |

Order: LT1 can go now (it is library only and `fibc test` does not change); LT0 is the owner's project and can go now; LT3 is small and
independent of LV1/LV2 and should go before LV2 fixes the live table's size; LT2 after LV2; LT4 with LV3's stress; LT5 after LV5; LT6
last. **LV4 (the program-prefix memo) is not in this list because it is already in live-repl.md's, but for meshql-fib it is the package
that turns a 4 s swap into a 0.5 s one** (1.4) and should be scheduled before LT5 (a watch that takes 4 s per save is a watch nobody keeps on).

## 7. The end-to-end acceptance script (expected transcript; nothing of it runs today)

`scripts/live-test.sh` of LT0, by hand. `$` lines are the shell, `>` the app's standard error, `fib>` inputs, `;` the client's lines. The
timings in the `; swapped` lines are **estimates** (1.4: check 3.5 to 4 s without LV4, under 0.7 s with it; emit and JIT milliseconds);
everything else is what the code paths of section 2 give.

```
$ cd meshql-fib && FIBC=$(scripts/fetch-fibc.sh .fibc)
$ cd examples/farm && PORT=3033 $FIBC run --live ../../dev/live.fib -- serve config.conf &
> fibc: live at /run/user/1000/fib-live/48213.sock
> meshql-fib listening on 0.0.0.0:3033 (4 graphlettes, 4 restlettes)
$ curl -s -X POST localhost:3033/farm/api -d '{"name": "Emmerdale"}'
{"name":"Emmerdale","id":"4f1c...-...-...-...-7a2e"}
$ curl -si localhost:3033/farm/api/nope | head -1
HTTP/1.1 404 Not Found

$ $FIBC repl --connect 48213
; attached to dev/live.fib (pid 48213, generation 0, catching); 3 spawned threads, pool idle; 802 live functions (meshql 151, cert 42, deps 609)
fib> :test --only One-caller-s-record-is-invisible-to-another
End-to-end authorization with differentiated credentials
  pass  One caller's record is invisible to another
1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout (seed 1)
; 1 scenario in 31 ms (generation 0)

(edit src/meshql/restlette.fib: in on-item, a GET of nothing visible answers (empty-response 410) instead of 404)
fib> :load src/meshql/restlette.fib
; src/meshql/restlette.fib: 17 definitions, 1 changed: swapped on-item (generation 1): check 3900 ms, emit 5 ms, jit 7 ms
$ curl -si localhost:3033/farm/api/nope | head -1
HTTP/1.1 410 Gone
fib> :test
End-to-end authorization with differentiated credentials
  FAIL  One caller's record is invisible to another
      > Then "bob" cannot read widget "alpha" by id
          expected: the step to pass
          actual:   expected status 404, got 410
          at contract/features/authz.feature:23:5
        id End-to-end-authorization-with-differentiated-credentials/One-caller-s-record-is-invisible-to-another  (contract/features/authz.feature:17:3)
1 scenarios: 0 pass, 1 fail, 0 trap, 0 timeout (seed 1)
replay: :test --only 'End-to-end-authorization-with-differentiated-credentials/One-caller-s-record-is-invisible-to-another' --seed 1
; 1 scenario in 34 ms (generation 1)

(edit specs/cert/steps.fib: the step "cannot read widget" accepts 404 or 410)
fib> :load specs/cert/steps.fib
; specs/cert/steps.fib: 10 definitions, 1 changed: swapped contract-steps (generation 2): check 300 ms, emit 40 ms, jit 55 ms
fib> :test
End-to-end authorization with differentiated credentials
  pass  One caller's record is invisible to another
1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout (seed 1)
; 1 scenario in 33 ms (generation 2)

(edit specs/cert/steps.fib again: templates gets a new entry)
fib> :load specs/cert/steps.fib
specs/cert/steps.fib:5:1: templates is a program def; its value is read count-free by every function [live/Def]
  hint: :restart replays the session; LT4 lifts this for a def of the same type
; refused; nothing of specs/cert/steps.fib was applied (generation 2)

fib> :test specs/contract-spec.fib --tags "@restlette" -j 4
A restlette enforces its JSON Schema
  pass  A widget the schema accepts is created
  ...
6 scenarios: 6 pass, 0 fail, 0 trap, 0 timeout (seed 1)
; 6 scenarios in 180 ms (generation 2)
fib> :status
; generation 2; generations kept 3 (41 KB); threads: 3 spawned, pool idle; values: 0 defs; tests: idle (last: 6 scenarios, 180 ms)
fib> :quit
$ kill %1
```

What the script checks mechanically (LT0's `scripts/live-test.sh`): the two curl status lines (404 then 410), the four `:test` summaries
(pass, FAIL with the `actual:` line, pass, 6 pass), the `live/Def` refusal, and that the farm server answered the whole time (a curl
every second in the background, all 200 or 404/410, none refused). What it does not check: the timings.

## 8. Decisions for the owner

1. **Which program is live** (2.3): a dev entry in the project (a), `--with SPEC` in the loader (b), or `fibc test --live` only (c). The
   design says (a) now and (b) as LT2's second half.
2. **The live module rule in a project** (section 4): the project's own paths with `--live-deps` (the design's recommendation), or
   everything not `fib.*` (what LV0 does), or a `:live` key per dependency.
3. **Defs in a loaded file** (LT4, FINDINGS blockers 1 and 2): lift `live/Def` for a new `def` and for a redefinition of the same type
   (immortal values make the store safe; the old value leaks and is counted), or keep live-repl.md 3.4's rule and `:restart`. This
   contradicts a rule in the merged design, so it is reported here, not changed.
4. **Fork isolation is refused in a live process** (3.3), and with it `--timeout`: a hung scenario is a `:restart`. Yes, or fund a way to
   kill a task (not in any plan).
5. **Tests do not block swaps and swaps do not wait for tests** (3.7): a run that straddles a swap is marked `stale` and rerun. Yes, or
   serialise them (an agent that swaps and tests in a loop would wait on itself).
6. **LV4 before LT5** (1.4, section 6): a 4 s swap is the floor without the program-prefix memo; the owner's "immediately" needs it.
7. **The names**: op `test`, `:test`, `:watch`, codes `test/NoSpec`, `test/NothingSelected`, `test/NoFork`, `test/Trapped`; `--with`;
   `--live-deps`. None is a core form or a builtin, so the self-hosting trap (CLAUDE.md) does not apply; `compiler/` defines no function
   named `test` or `watch` (`test-args`, `test-block`, `test-val` are private and differently named), and `test` is a driver command word
   already, which is a reason to keep it.

## 9. Not measured, and what was not done

- The attach socket, the `test` op, a swap in meshql-fib: nothing of LV1 or LV2 exists on main, so no swap of a meshql function was made
  and no live test was run; the swap-time numbers of section 7 are estimates from 1.4 and FINDINGS.
- The check time of the program modules alone with the library's state kept (1.4 is the whole front end; the estimate assumes the
  persistent checker keeps the library, as live-repl.md 4 says it does for a REPL session).
- PostgreSQL (`MESHQL_CERT_STORE=postgres`, steps 2 to 4 of `scripts/test.sh`), the proposed tier under `--isolate fork`, the TLS steps:
  not run; `fork` is refused in a live process by rule and that part of the suite stays a `fibc test` matter.
- Memory of kept generations over a session of swaps (FINDINGS 3): no swap exists to measure.
- A second latency run per row to put error bars on 1.2; the machine was shared and one run of 100 to 300 requests is what is quoted.

## 10. Built

- **Status 2026-10-15, LT1** (lib/fib/test/run.fib `run-main-into`; fib-gherkin `gherkin.cli/run-into`, the patch of
  docs/design/fib-gherkin-lt1.patch applied on a branch of fib-gherkin's head, which is v0.2.0, so the release is v0.2.1, not v0.1.1;
  its `specs/into-spec.fib` is the acceptance of the patch's last paragraph: the JSON of one scenario, a failing one, the plain rows,
  `--tags` and `--list`, `--messages`, nothing selected, all through the sink).
- **Status 2026-10-15, LT2** (serve.livetest: the request, the thread, the finish; serve.livewords: the selection and its refusals, the
  words, the entry's text, the client's rendering, without LLVM; the `test` op and `status.tests` in serve.live; `:test` in
  serve.liveclient; `--with` in expand.roots `Roots.with`, expand.modules `visit-with`, fibc `FIB_LIVE_WITH`; the test
  compiler/tests/live/test.sh over test-app.fib and test-spec.fib with unit-livetest.fib; the mutants stale-never,
  rerun-old-registry, test-on-attach-thread, sink-shared-run of scripts/mutant-live.sh). Where it differs from sections 3 and 6:
  (a) **the entry**: `fib.test.run/run-main-into SUITES WORDS SINK` where SUITES is an expression of the request (`suites`), a
  `--with` file's `(spec.NAME/specs)` (`spec`, by file or module name), or, with neither, the program's own `(specs)` when its text
  defines one (a `defspecs` in the app: the dev entry of 2.3 (a)), else the first `--with` file's; `runner` is an expression of a
  function of the words and the sink for a gherkin spec (`(fn (argv out) (g/run-into "features/*.feature" (steps) argv out))`),
  which the session calls as it is; no entry is `test/NoSpec`. The session's text gains `(:require [fib.test.run :as live-test-run])`
  and `(:require [spec.NAME :as spec.NAME])` for each `--with` file at the first run (a module is named in full only by a module that
  requires it: types.names `lookup`), and the `--with` modules are read before the main module's own dependencies so that require
  finds them. (b) **the entry is a definition of the session's program**, `live-test-N`, compiled as an expression input is
  (serve.livesess `program-of`, `add-cut`), entered through a `ccc` function of two descriptors (the report's file and the trap's),
  called on a thread of its own with native.call as `main` is (serve.live `start-program`); the sink is `live-write` of the guard;
  the entry's module holds a closure literal (the sink) and is kept for good, as an expression's is; its copy of fib.test.run's bodies
  is emitted per run (about 250 to 300 ms for one scenario on lilnas, the check of the program 30 to 50 ms of it: the transcript's
  `; 1 scenario in 304 ms (generation 0)`, where `fibc test` costs the spec's front end and JIT again). (c) **the answer**: `report`
  (the document, when the sink holds one), `text` (what the sink holds: the `--list` listing, or nothing after a trap), `status`,
  `generation_start`, `generation_end`, `stale`, `ms`, `threads_after`; status 2 is `test/NothingSelected` (the runner's message is on
  the program's standard error, not in the answer), 134 `test/Trapped` with the message; a second request is `busy` with the running
  request's `id` and `elapsed_ms`; `last: true` repeats the previous selection (`test/NoLast` before any). It is written on the
  connection that asked, by the connection's number (a descriptor is reused); a run whose connection closed goes on, its result is
  recorded for `status` and its answer dropped. (d) **not built**: `stdout`/`stderr` of the scenarios (a scenario's `println` goes
  where the program's output goes, as an expression's does: live-repl.md 9 (c)); the row events at `-j 1` (the JSON reporter streams
  no rows); `cancel`. (e) **the rows' positions are the session's text's** (`<repl>:L:C` in `at` and `id`), as live-repl.md 9 (g)
  says of every input's program; a `--with` file's rows carry its own file. (f) The client refuses `--format`, `--timeout` and
  `--isolate fork` with the reason, as the program does (`test/Format`, `test/NoFork`); `:test SPEC` takes a file or, in brackets,
  an expression of the suites.
- **Found by LT2, fixed (the quote statics across a swap)**: after a swap of a function that makes closures, a call of the base's
  closures crashed the program (`fatal signal 11 at 0xe`), deterministically: in test-app.fib, `(defspecs edited ..)` at the REPL with
  one scenario, the same `defspecs` again with the step's expectation changed (a swap of `edited`; no run of either is needed), then
  a call of a scenario `run` of the base's `specs`: the crash; `specs` itself swapped from a scenario of three `given` bindings to one
  of one crashed the same way, and from one to three answered `no match clause matched`. A function returning plain closures
  (`(let ((a x)) (fn () a))` swapped to three captures and back) did not reproduce it. The cause was not the closures: the `expect`
  macro quotes its form (`(then-line (quote ..))`), and a quoted form is a constant `@q.N` numbered per program (emit.program
  `program-fresh-quote`), like `@clo.N` and `@vt.N` and unlike `@str.N`. LV3's split of a swap's cut (serve.livesplit) exported every
  global that was not the generation's code into the session's base, `q.N` among them, so the next input's cut declared its own `q.N`
  (another form, of another shape) from the base instead of copying it: the base's `specs`, pruned out of the base (`main` does not
  reach it) and first lowered by that input, read program 2's `q.8` as its own. The fix: `emit.statics/program-static?` names the
  statics numbered per program (`clo.`, `vt.`, `q.`), and the split, the base index (serve.livesess) and the def cache (emit.defs.cache)
  all use it; a nested closure literal (`l.l.NAME.gK.N.M`) now carries its generation too (serve.livetable `strip-literal`), so the
  split keeps it with the code. compiler/tests/live/quotes.sh pins it (the mutant `quote-shared` of scripts/mutant-live.sh), and
  test.sh runs the base's spec after its step edit.
- **Status 2026-10-15, LT4** (emit.program `live-def-slot` and `live-def-setter`, emit.lower.objects `live-read`, emit.inits, emit.prune
  `Init.slot`, emit.compile `request-def` (`BodyRequest.impl` "def:"); serve.livedefs, serve.liveexpr (the expressions, out of
  serve.livesess); serve.liveswap, serve.liveload, serve.liveretire; the test compiler/tests/live/defs.sh over defs-app.fib and
  defs-cfg.fib, its TSAN part over defs-tsan.fib with defs-swapper.lir; the mutants `def-slot-plain-store`, `old-def-freed`,
  `init-run-twice` of scripts/mutant-live.sh). Built as the owner's decision 1 of docs/design/live-repl.md 9 says, with these
  differences from the row above: (a) **the old value is dropped, not leaked**: a redefinition stores the new value with the setter
  `live.d.NAME.set` (a release store; every body of the program's own reads `live.d.NAME` with an acquire load, live mode only) and puts
  the old object on the session's list; `retire` (at quiescence, serve.liveretire) releases it (`fib.release`, the destructor included)
  unless the code of a family a parked thread can be in reads the slot (its frame may borrow the value count-free across the pause point;
  the value waits for the next retirement). A copy the program stored holds a count of its own (a borrowed `def` stored is retained),
  so a drop frees only what nobody holds. A new value is shared (`fib.share`) and keeps its count; the program's first value, made before
  `main` and immortal, or static data, is never freed (`status.defs.immortal` counts it). `status.defs` and `retire`'s `defs`:
  `redefined`, `retiring`, `dropped`, `immortal`. (b) **A type change is refused, not re-checked**: `live/Def` names both types
  (`limit is a def of type i64 in the running program and str here`); a change its dependents do not survive is the checker's
  refusal first. (c) **A value of more than one word** (an `Option`, a `dyn`, a `bool`'s `i1`) has no slot (resident code holds it
  inline): its redefinition is `live/Def`. (d) **`defonce`** (the library's macro, a `def`) keeps the value: the text is accepted and
  nothing runs; a plain `def` re-evaluates (Clojure's `def`). (e) **A constant** is stored, not run: its slot's initial value is the
  constant, and any input whose program has the slot with another value than the running program holds (a redefinition, or a
  required module edited on disk and read anew: compiler/tests/live/defcache.sh) stores the program's value first (serve.livedefs
  `sync-constants`; the old constant is static data, never freed). A constant no resident body reads has no slot: the text is enough,
  and the next program that reads it makes the slot. A run-time `def` is evaluated by an entry of the input. (f) A new `def`
  of a loaded file is made as a session `def` is (serve.livedefs `make-slots`), its initialiser run once; a removed `def` is accepted
  (the checker refuses a removal that a remaining definition needs). (g) An initialiser that traps aborts the program, as a session
  `def`'s does (LV1 (d)). (h) A plain emit is byte-identical: `fibc emit compiler/fibc.fib` by this F and by main's F compare equal,
  and defs.sh's guard checks that a plain emit of the app has no `live.d.` and that a second `def` of a name is still `x is already
  defined`. Measured on lilnas: 1,000 redefinitions of an i64 (a constant, stored) and a 256-element vector (made at run time) in one
  input each while 8 threads read both, 53 to 58 s (about 55 ms each); every read was 3 or 5 and the sum 768 or 1280; `retire` dropped
  every old vector made at run time. Each redefinition keeps its module for good (the new initialiser, its entry, a new constant's
  static data), as LV1 (e) keeps an input's: a thousand redefinitions are a thousand kept modules.
- **Status 2026-10-16, LT4's two limits lifted** (serve.liveretype, new; emit.livedef, new, with the live-def functions out of
  emit.program; serve.livedefs, serve.liveswap, serve.liveload, serve.livetable `DefState.slots`, `RetiredDef.drop`, `defs-of-base`;
  serve.livesess `set-generations` and `install-base`; the tests compiler/tests/live/defs.sh (defs.py, defs-app.fib, defs-tsan.fib,
  defs-swapper.lir) and load.sh; the mutants `boxed-in-place`, `retype-no-readers`, `unswappable-reader-accepted` of
  scripts/mutant-live.sh). (b) and (c) above no longer hold:
  - **A type change is applied when its readers check.** The session's whole program is checked with the new type, as every input's
    is; a diagnostic in another definition that names the def becomes `live/Def` naming that reader (`count-base changes type here,
    and its reader plus-one would not type-check against it: ...`; in a loaded file at the def's line). Else the def gets a new slot
    `live.d.NAME.K` (K the input's generation: the session's generations carry it under the def's body name, so every later program
    reads it), and every family whose current code reads the old slot (serve.livereach's record of each resident definition) is
    emitted again under the input's generation and swapped with the input's families. A reader that cannot be swapped refuses the
    input, named: `main` (its frame never returns), a closure made by another def's initialiser, anything not in the live table, a
    family the program made no body of. One-shot code (initialisers, the session's entries) is no reader. The old slot keeps the old
    value for code still in an old generation; that value is retired as above. A `defonce` cannot change type (`live/Def`), nor a def
    that had no live slot. A type change no resident code reads (a session def, a def only the REPL reads) just takes the new slot.
  - **A value of more than one word is boxed.** A `bool`, an `Option` held inline, a `dyn`, a lane vector: the slot holds a box's
    address; the setter mallocs a new box for every store, writes the value, shares an object in it, and publishes the address with a
    release store; a reader acquire-loads the address, then reads the box. A box is never written once published, so a reader sees the
    whole old value or the whole new one. The old box is retired and freed by the slot's `live.d.NAME.drop` (the object in it
    released); a constant's first box is static (`live.d.NAME.box`) and is kept. A raw pointer still has no slot (`live/Def`).
  - Also fixed: a constant redefinition that equals the zero of a slot the base fills at run time (`(def made: (Option (Vec i64))
    nil)` after a run-time value) was not stored; the base's run-time slots are now noted as holding no constant.
  - Tests (lilnas, this branch's stage 2): defs.sh 33 checks `live defs: ok` and `ok tsan: no report beyond the baseline (0 known) in
    2 runs` (pair: nil and `(some 5)` stored 600 times under 8 readers); the stress is now 1,000 inputs of three redefinitions (an
    i64 constant, a vector and a boxed Option made at run time) under 8 readers, 63 to 67 s, every read good, 2,003 old values retired. All
    six def mutants are killed (boxed-in-place by TSAN, 4 reports, and by torn reads: the program's bad count 36 and 28). A type change
    with one reader swapped: 40 to 84 ms end to end (check 24 to 51 ms, emit 4 to 8, JIT 2 to 3). Main's defs test (two redefinitions per
    input, 1,000 inputs) by main's F and this F alternately: 49.3, 51.7 s against 53.2, 47.6 s; swap.py 35.6, 36.7 s against 36.5,
    37.4 s: no change beyond the noise. A plain emit of defs-app, stress-app and swap-app, and a live emit of swap-app, are
    byte-identical to main's.
  - On meshql-fib (live-dev, dev/live.fib under `fibc run --live`, the farm on SQLite; a scratch copy, the demo driver in the
    scratch of this work): `load` of src/meshql/store/sql.fib with `columns` (the SQL of every list and lookup) edited to
    `json_set(payload, '$.name', 'live') AS payload`: accepted, 1.8 s end to end (the first load: the engine expands 53 modules),
    no swap (a constant stored), and the next GET lists every farm as `live`; `columns` made a `(Vec str)` with `latest` joining it
    wrongly: `live/Def ... its reader latest would not type-check` in 0.7 s, nothing changed; joined rightly: accepted in 2.1 s (check
    1,524 ms, emit 359, JIT 75), swapping `latest` and the two readers of another module, `meshql.store/versions-of` and
    `meshql.store/create`; the next GET lists `typed`; the original file again: 0.39 s, the farms' names back. Every next request
    answered in about 1 ms. Against the 10 s of a restart, a redefinition is 0.3 to 2.1 s.
- **Status 2026-10-15, LT6** (driver.testlive; `--live` kept in `test`'s words by fibc `test-words?`, which also sets FIB_LIVE so the
  project's live roots resolve; driver.test `report-runs`; the test compiler/tests/live/testlive.sh; the mutant `child-spawned-anyway` of
  scripts/mutant-live.sh). `fibc test --live [PATH..]` finds the spec files as `fibc test` does and runs serve.live `run-live` in its own
  process over a host program with no app (`test-host.fib` in the live directory, shared by the user's sessions and written through a
  rename; its `main` waits until `(reset! live-test-stop true)` is evaluated, or the process is killed), each spec file loaded by `--with`,
  the module roots the command line's, each spec's directory and its project's (driver.test `project-roots`). `fibc test --connect
  SOCK|PID [PATH..] [options]` sends one `test` request per file (`spec`, `seed`, `jobs`, `only`, `list`, `isolate`) on one connection and
  reads each answer as a child's end (0 and 1 ran, 2 nothing selected, 134 died, a refusal an error), rendering the document in the
  format asked for with the library's own `plain` and `tap` (json and `--list` as the runner wrote them); the report and the exit status are
  driver.test's. Measured on lilnas (testlive.sh, the specs of test-cmd.sh): every row of test-cmd.sh that a session can hold is the same
  report byte for byte and the same status; the session is up in 1.5 s; one scenario 352 ms through the session against 1,570 ms
  spawned (the spec is tiny: the spawned run is the front end of fib.test). Not measured: meshql-fib's 20 s scenario (1.3), because its
  specs need OpenSSL, whose symbols the JIT does not find on lilnas (`undefined symbol @OpenSSL_version_num`, spawned and live alike).
  Not built: (a) a gherkin spec (the request names `spec`, so the entry is `(spec.NAME/specs)`; a `runner` is not offered by `fibc test`);
  (b) a spec that does not compile is not loaded (each spec file is checked alone first, with one engine, before the session starts;
  this costs the session's start about 2.5 s for testlive.sh's six files on lilnas), the session serves the others, and a run of it
  answers status 3 with the diagnostics as `fibc run` prints them (`test/NotCompiled`), so `--connect` reports it as one file's ERROR
  as `fibc test` does (it refused the whole session, exit 3, until the live bugs' fix); a spec
  whose `main` does not run its `specs` (test-cmd.sh's dying spec) is not reproduced (the session never calls `main`); (c) `--timeout`
  and `--isolate fork` are refused (exit 2), as the `test` op refuses them; (d) two spec files of one base name are one module
  `spec.NAME`: the first is loaded and a run of the second answers status 4 with the reason (`test/NotHeld`: `its module spec.NAME is
  FILE's too; a session holds one spec module of a name`), an ERROR of that file; naming the modules by path would change the name a
  `:test x-spec.fib` or `--with` user gives, so it was not done. `--connect` sends each spec as an absolute path; (e) the runs are sequential (one test at a time in a session), `-j N` is each file's scenarios N at a time.
- **Status 2026-10-15, LT5** (serve.livewatch: the stamps, the scan, the poll, the events; serve.live `op-watch`, `watch-step`,
  `status.watch`; serve.liveclient `:watch on [DIR..]|off` and the events between inputs; the test compiler/tests/live/watch.sh over
  watch-app.fib and watch-handler.fib; the mutants `watch-no-load` and `watch-reruns-on-refusal` of scripts/mutant-live.sh). The op is
  `{"op":"watch","on":true,"dirs":[DIR..]}` (answered `watching`, `files`: how many are watched) and `"on":false`. Where it differs from
  3.8: (a) **no thread**: the poll runs on the attach thread in the watching connection's loop, which already wakes every 250 ms, at most
  once per 200 ms and never while a test runs (a change made meanwhile is seen when the run ends); a scan is one `stat` per file (inode,
  size, modification time with nanoseconds; the `struct stat` offsets of Linux and Darwin), so it does not hold the attach thread
  (`status` is answered while watching), and nothing of the session crosses a thread. A change is acted on only when the file's stamp
  is the same in two consecutive scans (the debounce, added by the live bugs' fix: before it a file caught half-written was loaded,
  refused, then loaded again), so a change is seen one scan (about 250 ms) later. (b) **The files**: the session's live module files
  (serve.liveload's copies, the main file among them) and the regular files under the `dirs` (depth first, dot names skipped: an
  editor's temporary files); a file in the `dirs` that is not a module file is a "feature file": its change reruns without a load. No
  default directory: `:watch on` alone watches the module files. (c) **The events** are the answers with an `event` field first:
  `{"event":"load", "ok":.., "file":.., "swapped":..}` per changed module file (as `load` answers), then, when every load was accepted
  and a test ran before in the session, `{"event":"test", "id":"watch", ..}` when the rerun (`last: true`) ends; a refused load emits
  its refusal and reruns nothing, and a change before any test reruns nothing (no event but the loads). (d) The watch belongs to the
  connection: it ends with `watch off` or when the connection closes. The terminal client prints the events as they come between
  inputs (the prompt written again after each); `--json` forwards them as lines. Measured on lilnas under a running gate (load average
  above 60): an edit of the handler's file to the rerun's event, 3.4 s (the design's 1.5 s is the idle figure; the test asserts 10 s).
