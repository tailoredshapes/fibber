# A behaviour-spec harness for fibber: `fib.test` and `fibc test`

Status: design with a working prototype, 2026-10-05. **Measured** means a command was run in this session and its output is quoted
(section 8). **Prototype** means it exists in `lib/fib/test/` and `compiler/tests/harness-proto/` and `harness-proto/run.sh` passes.
Anything else is design and is marked "not built". The prototype was built and run with the tree's stage 2 (`F`) from
`~/.cache/fibber-scratch/rr1/tools/F`, `FIB_LIB` = this tree's `lib`. It is **not** in the gate (section 6.5 recommends a stage).

The owner's request: "something bdd like that lets us specify behavior that can be preserved across implementations", more than
units. The Rust oracles are gone (RR1), so the behaviour that `fibref` and the Rust `fibc` used to pin must be pinned by specs that
outlive any implementation of the thing they specify.

Contents: 0 short answer; 1 what exists and where `fib.test` sits; 2 the spec language; 3 contracts; 4 properties; 5 isolation;
6 runner, reporters, gate, mutation; 7 preservation; 8 the prototype and its output; 9 options rejected; 10 not done, risks,
decisions for the owner; 11 package plan.

## 0. The short answer

1. **`feature` / `scenario` with `given` / `upon` / `then` steps**, not `describe` / `it`. A scenario is a value (id, text, planned
   steps, covered rows, and a closure that runs it), built by a macro over the existing macro system; the steps a run executed come
   back as a vector of `StepRun` values, so a reporter prints "Given m = (assoc (make) 1 10), Then (some 10) = (get m 1)" without
   re-reading source. `when` is a library macro, so the step is `upon` (printed as "When").
2. **`expect` is a value, not a trap.** `(expect = 3 (f))` makes a `Broke expected actual` entry in the step log and the scenario
   goes on; no mechanism of the language is needed for ordinary failures.
3. **A contract is a function of the implementation's constructors** returning scenarios: `(defcontract MapContract [make] ..)`.
   The type checker needs *nothing new*: the function is generic, its protocol bounds are inferred, and each `(implements
   MapContract "AList" (fn () (alist-empty)))` is monomorphised by the existing machinery (types §4.3). Prototype: the Map contract
   passes against the library's `Map` and a 36-line association list; a planted fault in the list fails one scenario of one
   implementation.
4. **Properties are scenarios**: `(prop "name" [x (gen-vec (gen-i64))] body)` runs 100 seeded cases, shrinks the first failure
   greedily and prints the shrunk value; the seed is the run's seed and `--only ID --seed N` replays it. SplitMix64 as in fibgen.
5. **Isolation by `fork` without `exec`**, interim until L28 step 1: each scenario runs in a child, which writes its steps to a
   file and `_exit`s; a trap or hang becomes a `TRAP` / `TIME` row. Measured overhead 0.2 ms per scenario, 5 us in-process.
6. **`fibc test` is not built.** The prototype's entry is `run-main` in a spec file's `main` (`fibc build`, or `fibc run FILE -- ARGS`),
   with the command line the design gives `fibc test`. A spec file is *also* a `cases/` case today (section 2.6), which is the
   cheapest way into the existing gate while `fibc test` does not exist.
7. **Not replacing anything.** `cases/` stays the compiler's verdict tests; units (`ck`) keep working; specs are for library and tool
   behaviour that must survive a change of implementation.

Two facts found by building it that shape the design (both are limits of today's tools, not of the idea):

- **A stage-2 macro sees the prelude only.** `macros/runner.fib` checks the macro's own forms with the prelude; a function of a
  library module (even one the macro's module requires) is "not available at expansion time". So macros cannot print forms or share
  helpers; they build code that *quotes* the step's forms, and `fib.test.core` prints them at run time (`form-text`). Consequence: the
  step text is exact source, the cost is a few string builds when a scenario is constructed.
- **`fibc run FILE` does not return `main`'s value as its exit status** (it prints it; measured: exit status 0 with result 1), so a
  spec run under `fibc run` cannot gate by exit code. `fibc test` is therefore a real command (it builds or JITs the spec and returns
  the run's status), and `fibc cases` is the other route (the verdict is `;; result:`).

## 1. What exists, and where `fib.test` sits

| Mechanism | Judges | Strength | What it cannot do |
|---|---|---|---|
| `cases/` (`fibc cases`, headers `expect result audit allocs covers open stage`) | one whole program: does it compile, what does `main` return, what does the memory audit say | the compiler's own verdicts; allocation bounds; reject texts; both stages | state a behaviour of a library function over many inputs; run the same program against several implementations; say *which* check inside a program failed (the result is a bit mask, `tl.check`) |
| unit programs (`compiler/tests/*/unit-*.fib`, `ck.fib`, `heaptk.fib`, 82 files) | checks inside a program, counted by a `run.sh` that knows the expected count | cheap to write; fine for an internal helper | one failure aborts the file's remaining checks if it traps; names are strings the author types twice; a count in a shell script is the only link to intent; no JSON; no seeds |
| goldens (`compiler/tests/golden/`) | text equality against recorded output | pins a pass's whole output | says nothing about *why*; a re-record accepts a regression |
| mutant scripts (`scripts/mutant-*.sh`) | whether the cases notice a planted fault | shows a case can fail (method rule: a test that cannot fail is worse than none) | hard-wired to case numbers (`CASES="286- 287- .."`) |
| `fib.test` (this design) | the behaviour of a library function, a data structure, a tool or a protocol, stated once and runnable against any implementation | step texts, per-scenario status, contracts, properties, isolation, JSON | judge a program's compile verdict or its allocation count (use a case) |

### 1.1 Relation to `cases/`, and can one be written as the other

- **Spec as case (works today, prototype).** A spec file whose `main` is `(run-main (specs))` returns 0 when everything held and 1 when
  anything did not, so the header `;; expect: accept ;; result: 0 ;; audit: clean` makes it a case: the verdict is the run's exit value,
  and the memory audit covers the harness and the forked children. Three such cases are in `harness-proto/cases/`; `fibc cases` runs
  them (quoted in 8.5). A case whose header says `result: 1` over a deliberately broken spec *tests the harness itself*: if the
  harness stopped reporting a failure, that case fails.
- **Case as spec (partial, not built).** The kind `accept` with a result is one scenario `(then (expect = N (run-main-of "case.fib")))`;
  that needs a compile-and-run function in the library, i.e. the machinery of `driver.child`, and `reject` / `trap` verdicts need the
  compiler as a library. Neither is worth it: a case is already the right shape for "this program means this". **Do not convert cases.**
- **Where each belongs.** If the subject is the *compiler* (a program is accepted, rejected with this text, allocates at most N), it is a
  case. If the subject is a *library function, data structure or tool* whose behaviour another implementation must also have, it is a
  spec. The `covers:` header of a case and `(covers "assoc" "get")` of a scenario name the same rows of `spec/stdlib.md` §4 (section 7).

## 2. The spec language

### 2.1 Pick: `feature` / `scenario` / `given` / `upon` / `then`

`describe` / `it` (Jasmine, RSpec) nests lexically and names things after the fact: `it "returns 3"`. It fits unit tests and has no place
for a step. `feature` / `scenario` with steps (Gherkin, Cucumber) separates **setup, action, observation**, which is what a
*contract* needs: setup is the part that varies by implementation (`make`), the action is the call under test and the observation is
the behaviour that must not change. It also gives the reporter something better than a string: three kinds of step, each with text.
Reasons for picking it, in order of weight:

1. A contract's scenarios are read by someone adding an implementation who wants to know *what they lack*: "Given an empty map, When
   assoc 1 10, Then get 1 = (some 10)" is that sentence.
2. Steps as data. The macro turns every step into a `(kind, text)` entry of the scenario's plan and, when run, into a `StepRun`.
3. Statically typed fibber cannot thread an untyped context map between steps (Cucumber's World). Here a `given` or `upon` is a `let`
   that scopes over the steps after it, so the *type checker* sees the data flow: no step definitions, no regex glue, no runtime lookup.
   This is the one place the Gherkin model is deliberately changed.

`describe` / `it` is rejected as the primary form, not forbidden: a scenario with only a `then` already reads as an `it` (the second
scenario of `harness-proto/cases/003-*.fib` is `(scenario "still runs after it" (then (expect = 1 1)))`).

### 2.2 The forms (prototype: `lib/fib/test/core.fib`)

```clojure
(ns specs.map (:use fib.test.core))

(defspecs specs                                       ; (specs) -> (Vec Suite): what the runner runs
  (feature "Vec"
    (scenario "conj appends"
      (covers "conj")                                 ; optional: rows of spec/stdlib.md this scenario pins
      (id "vec-conj-appends")                         ; optional: a stable id (default: slug of feature + text)
      (given [v [1 2]])                               ; binds for the steps after it
      (upon  [w (conj v 3)])
      (then (expect = [1 2 3] w) (expect = 3 (count w))))
    (outline "quot and rem follow Clojure" [a b q r]  ; Scenario Outline: one scenario per row of the table
      [[7 2 3 1] [-7 2 -3 -1] [7 -2 -3 1] [-7 -2 3 -1]]
      (then (expect = q (quot a b)) (expect = r (rem a b))))
    (prop "reverse twice is the identity" [x (gen-vec (gen-i64))]
      (= (vec (reverse (vec (reverse x)))) x))))
```

| Form | Is | Notes |
|---|---|---|
| `(scenario "text" step..)` | macro, a `(Vec Scenario)` of one | steps: `given`, `upon`, `then`, `covers`, `id`. A scenario with **no `then`** is reported as failed ("a scenario that cannot fail"): the method's rule, enforced. |
| `(given [b e ..])`, `(upon [b e ..])` | step | a `let` over the rest of the scenario; text "Given b = e, .." / "When .."; the kinds are only labels (a `given` is not checked to precede an `upon`). |
| `(then item..)` | step | an item is `(expect rel expected actual)` or a bare boolean form. Each item is one reported step; every item runs even if an earlier one broke. |
| `(expect rel expected actual)` | read by `then` | `rel` is any two-argument function (`=`, `<`, a set-equality); both sides are evaluated once and printed with `pr-str` (so they need `Debug`, which `defrecord` derives). Outside a `then` it is unbound by design. |
| `(outline "text" [names] [[row]..] step..)` | macro | one scenario per row, text "text #n row", a `(given [name value ..])` of the row first. Types of a column come from the literals. |
| `(feature "name" scenario..)` | macro, a `Suite` | ids are prefixed with the slug of the name. |
| `(defspecs name suite..)` | macro | `(defun name () -> (Vec Suite) [..])`. |
| `(prop "text" [x gen] body..)` | macro (`fib.test.gen`) | section 4. |
| `(defcontract Name [params] scenario..)`, `(implements Name "impl" arg..)` | macros | section 3. |

Names were grepped against `compiler/` and `lib/` (the self-hosting trap): `feature`, `scenario`, `given`, `upon`, `then`, `outline`,
`prop`, `defcontract`, `implements`, `defspecs` are defined nowhere. Types that did collide were renamed (`Row`, `Ended`, `Job`, `Rec`
of the compiler's own modules: the library's are `ScenarioRow`, `ChildEnded`, `ScenarioJob`, `StepLog`), because type and variant
names are global across linked modules and a future `fibc test` links the runner into the driver. One name is shared on purpose:
`lib/fib/lacinia/parse/cursor.fib` defines a function `expect`; module scope keeps it apart from the form `expect` (which is read by
`then` and is never a binding), but a spec that `:use`s both modules should write `(:require [..lacinia.. :as g])`.

### 2.3 Steps as data

```
(defstruct StepRun  (kind: str text: str judged: Judged))   ; one executed step
(defenum   Judged   (Held) (Broke expected: str actual: str))
(defstruct Scenario (id: str text: str covers: (Vec str) steps: (Vec str) run: (fn (i64) (Vec StepRun))))
(defstruct Suite    (head: str impl: str scenarios: (Vec Scenario)))
```

`steps` is the *plan*: the texts, known without running (`--list` prints it). `run` takes the seed and returns the `StepRun`s. A
reporter never sees source; it sees data. Texts are made at run time from `(quote form)` (the macro limit above) and read like the
source: `Then (some 10) = (get m 1)` is "expected rel actual" in the order written.

### 2.4 Plain-text (Gherkin) front end: rejected for now

A `.feature` file would need step definitions: a regex-to-function table, i.e. the dynamic glue (2.1 point 3) that static typing
removes. It earns its keep only if non-programmers write specs, which is not the case here, and the owner asked for "fibber-native".
What the plain text *would* give (the sentence a reader sees) the plan and `--list` already print. **Not built; revisit only if
someone other than the maintainers must author scenarios.** If ever built, the front end should *generate fibber `scenario` forms* from
a restricted Gherkin, resolving step phrases by a table of macros (`given-an-empty "map"`), never by runtime lookup.

### 2.5 Tables

`outline` is the table form. Columns are typed by the literals; a row is data, so a table can be *generated* by a script (the oracle hook,
7.4) and checked in as source, which keeps the spec readable and the data auditable. Prototype output: 8.1 (rows ids `...-1-7-2-3-1`
... `-4-...`; the first version collided on rows with a minus sign, found by running it, so ids now carry the row number and the runner
refuses duplicate ids).

### 2.6 Spec files are cases

See 1.1. The convention that makes this work: a spec file ends with `(defun main () -> i64 (run-main (specs)))`. A *library* of specs
(contract modules, `map-contract.fib`) has no `main` and is `:use`d by the file that registers implementations.

## 3. Contracts

```clojure
(ns map-contract (:use fib.test.core))
(defcontract MapContract [make]                       ; => (defun MapContract (make) (flat [scenario..]))
  (scenario "assoc then get"
    (covers "assoc" "get")
    (given [m (assoc (make) 1 10)])
    (then (expect = (some 10) (get m 1)) (expect = true (contains? m 1)))))

;; in the spec file
(defspecs specs
  (implements MapContract "Map"   (fn () (std-map)))        ; (std-map) -> (Map i64 i64)
  (implements MapContract "AList" (fn () (alist-empty))))
```

### 3.1 How it is typed (verified by the prototype, not argued)

`defcontract` expands to a *plain generic function* from the factory to a vector of scenarios. Each scenario's body calls protocol
methods (`assoc`, `get`, `contains?`) on a value of the factory's return type `m`, which is a type variable; the checker generalises the function at its `defun` boundary with the protocol constraints
(`Assoc`, `Lookup`, `Keyed`, `Dissoc` at `m i64 i64`; spec/types.md §1.8, §3.6; the scheme was not printed, the function compiles
and runs) and `implements` instantiates it at `(Map i64 i64)` and at `AList`. Monomorphisation (§4.3) keys a protocol-bounded variable by
its full type, so each implementation gets its own specialisation in which every method call is a direct call: **the contract is checked
against each implementation's types, at compile time, and a missing `impl` is a type error naming the protocol**. Measured, with the
`(impl (Keyed i64) AList ..)` block deleted from a copy:

```
rejected:
.../lack/map-spec.fib:9:15: no implementation of Keyed for AList
```

(line 9 is the `implements MapContract "AList"` form), which is the right way for "this implementation lacks a behaviour" to appear
*before* the run. A lacking *behaviour* (wrong answer) is a failed scenario; a lacking *operation* is this compile error.

What a contract may rely on: only protocols and functions of the factory's type; any number of factories (`[make make-from-pairs]`) or
a record of functions (a `defstruct` of closures) if there is no protocol yet. What it cannot do: dispatch on an implementation chosen
at run time (that is `(dyn P)`, an escape hatch not needed here: `implements` forms are static), or put differently typed
implementations in one `Vec` — the `Suite` erases the type, because the generic function is *called* inside the suite's construction
(scenarios are closures over the factory), which is why `Suite` holds scenarios and not the factory.

Requirements on the type checker: **none new**. Two things to watch, found in the prototype: the factory's result must be
unambiguous (`std-map` is annotated `-> (Map i64 i64)` because `{}` would leave the key type open); and a contract that uses a *type
class* the implementation lacks (`Hash`, `Eq`) fails at the `implements`, with the checker's message, not at run time.

### 3.2 Registration and reporting

`implements` is a macro for `(contract-suite "MapContract" "AList" (MapContract factory))`; registration is the list in `defspecs`
(no global registry: fibber has no mutable globals, and a list is greppable). The runner flattens suites to jobs and reports **per
(contract, impl, scenario)**: ids are `MapContract[AList]/assoc-then-get`. Adding an implementation is one `implements` line and the
next run lists, for it, every scenario it fails or lacks (8.3). A *cross-implementation matrix* report (rows = scenarios, columns =
implementations) is a pure function of the JSON and is **not built**.

### 3.3 The implementation pairs the owner named

| Pair | How the contract is parametrised | State |
|---|---|---|
| `Vec` vs another persistent vector | factory `(fn () (vec-empty))` against the other type, protocols `Collection`, `Lookup`, `Stack`, `Emptyable` | prototype uses `Map` vs `AList` with the same protocols; the Vec contract is not written |
| compiled run vs a future interpreter (fibref-port.md) | the factory is *the runner*: `(fn (src) (run-program src))` returning a `Result` record; scenarios give a program text and the expected `(Ok n)` / `(Err text)` | not built. Needs a library entry point that runs a program and returns its outcome (today only `driver.child`); the contract would be the *spec of `fibc run`*, a replacement for the retired Rust-vs-fibber comparison |
| Clojure 1.12 oracle | not an implementation: a *generator of tables* (7.4) | hook designed; one table checked by hand (8.1) |
| language server over the protocol vs in-process | factory `(fn () (Session))` with `(open s uri text)`, `(hover s uri pos)`, where one implementation speaks JSON-RPC to a child and the other calls `lsp.analysis`; scenarios are the existing `lsp-transcripts` | not built; the transcripts in `compiler/tests/golden/fibref/` are the first scenarios |
| fibgen's two generators | contract "a generator": `(gen seed size)` is a pure function of two integers (fibgen-port.md section 1) and the output parses and type-checks; same seed gives the same text across runs | not built; the *determinism contract* is already stated there and fits one scenario per property |

## 4. Properties (prototype: `lib/fib/test/gen.fib`)

```clojure
(defstruct (Gen a) (make: (fn (TRng i64) a) shrink: (fn (a) (Vec a))))
```

A generator is two functions; `make` draws from a seeded generator and a *size* (the case index ramps it: 1..30, so the first cases
are small); `shrink` returns simpler candidates, simplest first. Built-ins: `gen-i64` (+-4 size, shrinks 0, n/2, n-+1), `gen-i64-between`,
`gen-bool`, `gen-str` (letters), `gen-vec g` (drop all, drop half, drop one, shrink one element), `gen-pair`. A property is a
`Scenario` (so it is listed, selected, isolated and reported like any): 100 cases, the first failing value shrunk greedily (the first
candidate that still fails replaces it, at most 500 steps), the report quoting the shrunk value and the original draw.

- **Seeds.** SplitMix64 with fibgen's constants and output function (`compiler/gen/rng.fib`; the prototype *copies* the 12 lines
  because the library cannot depend on `compiler/`; **move `gen.rng` to `lib/fib/test/rng.fib` and have fibgen require it** so one stream
  means one thing). A scenario's seed is `stream-seed(run seed, hash(local id))`: independent of which other scenarios were selected
  and **identical across the implementations of a contract** (the id is without the `[impl]` part), so every implementation sees the
  same random values. Case `i` uses `stream-seed(scenario seed, i)`. The run prints its seed (`... (seed 1)`) and a `replay:` line.
- **Records via `derive`** (not built): `(derive Arbitrary T)` generates `gen-T` from the fields' generators the way `defrecord`
  derives `Eq` and `Show` (`lib/fib/coll/record.fib`: a macro that returns `defstruct` plus `derive` forms). A record's shrink is
  field-wise through the `Gen` of each field, with lock-step shrinking by trying each field in turn. Floats: `gen-f64` over a
  finite set (0, +-1, small, large, subnormal, no NaN unless asked); shrinks toward 0 by halving.
- **Maps** `(gen-map gk gv)`: via `gen-vec (gen-pair gk gv)` then `into`; shrinks as the vector.
- **Limits of the prototype.** One binding per `prop` (use `gen-pair` for two); shrinking is greedy, not integrated; strings are
  letters only; a property returns `bool` (the failure shows the value, not which sub-expression broke).

## 5. Isolation

**Requirement:** a failing assertion must not abort the run; a trap must not either.

1. Assertions are values (2.2): `expect` appends `Broke`; no trap. Measured: a scenario with a wrong claim and the scenarios after it all
   report (8.1).
2. Unexpected traps (`nth` out of range, overflow, `(trap ..)`) end the process (spec/types.md §2.11: no handler), unless they happen on a
   task's thread: exceptions stage 1 returns a task's trap from `try-join`. **Built (2026-10-05): the default isolation is the task**,
   `(try-join (spawn (fn () (run seed))))` per scenario, `-j N` tasks at a time; `Scenario.run` and the `Gen` closures are `:send`.
   Measured (1000 trivial scenarios, `compiler/tests/harness-proto/stress.fib`): in-process 8 ms; task `-j 1` 113 ms, `-j 4` 41 ms, `-j 12`
   28 ms; fork `-j 1` 233 ms, `-j 4` 149 ms, `-j 12` 124 ms. What the task does not give: a trapped task's frames are abandoned (the audit
   reports `leaks`, case 003 of the prototype: 10 objects), a hang cannot be stopped, stack exhaustion and out-of-memory are fatal for the
   process, and the runtime also prints the trap on standard error.
3. **Fork, for a time limit or a crash** (`--isolate fork`, implied by `--timeout SECS`): `fib.os.process` has `fork-start`, `wait-any-child`,
   `fork-collect`, `fork-run` (cases 7531, 7532, mutants in `scripts/mutant-fork.sh`). The child *calls the scenario closure itself* (no
   re-entry by name, no second compile), writes `encode(steps)` to `DIR/N.res`, its standard output and error to `N.out`, `N.err`, and
   `_exit`s without the audit; an `alarm` kills a hang (status 142, reported `TIME`). A trap is exit 134 with the message as the last line of
   `N.err`. **Threads:** POSIX keeps only the calling thread in the child, so a lock another thread held stays locked and the run queue's
   workers are gone; `fork-start` refuses (an `Err`) while `/proc/self/status` shows another thread. A finished `spawn`'s thread is detached
   and gone, so fork after `join` is allowed; the pool's workers stay, so fork before the first `async`. No `pthread_atfork` is used (it
   would need the runtime to reset `fib.pool-started`, an `rt/` change this work did not make). Where `/proc` is absent (macOS) the count is
   unknown and the guard is the caller's. A runner that forks must not have started tasks first; hence `--isolate task` and `fork` are
   alternatives per run, not mixed.
4. **Why not re-enter the binary by name** (`fibc test FILE --only NAME`, the other option the owner named): it pays a process start and,
   under `fibc run`, a *whole front end and JIT* per scenario (0.18 s for `hello`, 1.8 s for the 29-line prototype spec, measured), so
   100 scenarios would cost minutes; with an AOT-built spec binary it is about 1 ms. Fork-without-exec (0.2 ms, measured) is about 5x cheaper than even
   that (the 1 ms is an estimate, not measured) and works the same under `fibc run`.
5. **Measured overhead** (8.4): 1000 trivial scenarios: 5 ms in-process (5 us each), 197 ms isolated `-j 1` (0.2 ms each), 100 ms at
   `-j 12`. Per-run cost is the compile (4.7 s `fibc build` of the prototype, 1.8 s `fibc run`), not the scenarios.
6. **Hazards, found or reasoned.** (a) FIB_TRACE: the child's allocation lines went to the parent's trace under reused ordinals ("free
   of an object not live") because the child allocated before it had its own stderr; fixed by making the path in the parent so the
   child's first act is `dup2` (found by running a spec as a `fibc cases` case; section 8.5). (b) Tasks: a scenario may `spawn` and
   `join` (measured, passes) because the parent has not started its pool; **the runner itself must never start threads before it forks**.
   (c) A scenario that touches a shared resource (a port, a file) needs `-j 1`; a `(serial)` tag is not built. (d) Output a scenario
   prints goes to the parent's stdout interleaved by `-j`; the child's stderr is captured, its stdout is not (not built: capture both).
   (e) Not Windows (the project's targets are Linux and macOS; `fork` is POSIX).

## 6. The runner, reporters and gate

### 6.1 Command line

```
fibc test [PATH..] [--only SUBSTR] [--seed N] [-j N] [--format plain|tap|json] [--list] [--no-isolate] [--timeout SECS]
          [--covers FILE.md]
```

`PATH` is a spec file, or a directory searched (at any depth, in name order) for files named **`*-spec.fib`** (the convention: hyphenated like the
rest of the tree, one suffix, so `fibc test specs/` and a shell glob agree; no PATH means `specs`). Each spec file is a program whose `main` is
`(run-main (specs))`; `fibc test` runs each as `fibc run FILE -- OPTIONS` in a child process of itself (the JIT: no build step; `run` prints
`main`'s result as its last line, which is where the status comes from) and puts the reports together. **Built (2026-10-05):**
`compiler/driver/test.fib`, `--only`, `--seed`, `-j`/`--jobs` (several files: N files at a time; one file: N scenarios at a time),
`--format plain|tap|json`, `--list`, `--isolate task|fork|none`, `--timeout` (selects fork), `-I`. **Not built:** `--covers`. `--only` is a substring of the full
id (`MapContract[AList]/assoc-on`), a file where it matches nothing is skipped while another matches. `--seed` defaults to 1: **deterministic runs,
not time-seeded** (a flaky test must be reproducible); a CI or a soak run passes a fresh seed and the report prints it.

Exit status: **0** every selected scenario held; **1** one failed, trapped or timed out, or a spec process died (a fatal runtime error, a
signal); **2** a usage error, an unreadable PATH, no spec file, a spec that does not compile, or a selection that matches nothing anywhere ("a
run with nothing to run is not a pass"), or duplicate ids. Checked by `compiler/tests/driver/test-cmd.sh` (29 checks over a holding, failing, trapping,
hanging and non-compiling spec, a directory, `--only`, the three formats, `-j`).

### 6.2 Plain reporter

Colourless, one row per scenario, failures expanded with the steps and the two sides, a counts line with the seed, and `replay:` lines
(8.1). Passing scenarios are one line; failing ones list every step so the reader sees the Given that led to the Then.

### 6.3 JSON schema (version 1; prototype)

```
{ "schema": 1, "seed": N,
  "summary": { "total": n, "pass": n, "fail": n, "trap": n, "timeout": n },
  "results": [ { "suite": str,            // feature name, or the contract's name
                 "impl": str,             // "" for a feature
                 "id": str,               // stable, unique, includes [impl]
                 "text": str, "covers": [str..],
                 "status": "pass"|"fail"|"trap"|"timeout",
                 "message": str,          // trap text, timeout reason, or "no Then step.."
                 "steps": [ { "kind": "Given"|"When"|"Then", "text": str,
                              "status": "held"|"broke", "expected": str, "actual": str } ] } ] }
```

`expected` and `actual` appear on a `broke` step. **Location (built):** a step's `"at"` is `"FILE:LINE:COL"`: of its `expect` (the macro's `(call-pos)`, spec/syntax.md §1.3, so an `expect` in a contract
reports the line in the contract's file), of the `scenario` or `prop` for a `then` that is a plain boolean form, `""` for a Given or When; a result's
`"at"` is its `scenario` or `prop`. A stage-2 macro still sees no position of a form it is handed, which is why `expect` became a macro of its own
(it expands to a function of the scenario's log; `scenario` calls it) and the surface forms did not change. The plain report prints the
step's `at` under the broken step and the scenario's after its id. `fibc test --format json` adds a top-level `"files"` array
(`{"file","status","outcome"}`) to the schema, whose `results` are those of all files.

TAP (`TAP version 13`, `1..N`, `# seed S`, `ok N - id` / `not ok N - id # status: message`, `# step: expected .., actual ..`) is built; `fibc test` renumbers across files.

### 6.4 Parallelism

`-j N` forks up to N scenario children at once, as `fibc cases -j` runs N case children, with the same waiting loop (`wait-any`
keyed by pid; rows put back in order). Measured: 1000 scenarios `-j 1` 197 ms, `-j 4` 167 ms, `-j 12` 101 ms (trivial scenarios: fork-bound). The
machine's rule (at most about 12 heavy jobs) applies to the *sum* of gate stages, not to this default.

### 6.5 Gate

Do **not** add to the gate yet (as asked). Recommendation: a `specs` stage in `scripts/gate.sh`, after the case directories,
quick and full: build each spec file (or `fibc test specs/` once it exists) with the stage-2 `F`, run with `-j $jobs --seed 1`
(deterministic), require exit 0, and print one timing line (`specs: 142 scenarios 3 s`). Failures are *not* compared with
`scripts/ci-stage2.expected` (a new spec failing is a regression, not a known gap); a known-bad scenario carries `(open "ITEM")` (not
built; same semantics as the case header `open`: a pass is a failure, "the item landed: remove `open`"). A `--seed $RANDOM` soak lane
belongs in the nightly full gate, with the seed printed.

### 6.6 Mutation

`scripts/mutant-*.sh` today list case numbers (`CASES="286- 287-"`). With specs a mutant script says *which specs must notice*: after
building the mutant stage 2 `F`, run `F test specs/ --only PREFIX..` (or the spec binary built by the *mutant's* compiler, because a
spec checks the library the compiler was built with) and require **exit 1 with at least one `fail` or `trap`**; the JSON gives the
scenario ids that killed it (`jq '.results[]|select(.status!="pass").id'`), which the script prints as the evidence. Prototype check
of this idea in the small: `run.sh` plants a fault in the association list in a *copy*, builds, and requires exactly one scenario to fail
in exactly one implementation (8.3).

## 7. Preservation

1. **Ids.** `feature slug / scenario slug` (letters and digits kept, other runs `-`), `[impl]` after the contract. The author may fix an
   id with `(id "..")`; **rename a scenario's text, keep its id**. The runner refuses duplicates (measured: the message in 8.6).
   An id is the key of a replay, of a seed stream, of a JSON consumer and of a mutation report; so changing one is a deliberate
   act, caught by a **checked-in ids list** (`specs/IDS` produced by `fibc test --list`) that the gate diffs: a removed id is a reviewed
   deletion (not built; the list output exists).
2. **Rows.** `(covers "assoc" "get")` names rows of the table in `spec/stdlib.md` §4, exactly as `;; covers:` does for cases; a name that
   is no row is an error. **Today nothing checks `covers:` against §4** (`compiler/driver/header.fib` only parses the key; the Rust
   `stdlib_table` test that did is retired with the crates; I found no fibber port). The coverage report below is that check, and serves
   cases and specs alike.
3. **Coverage report** (not built, 30 lines): `fibc test --covers spec/stdlib.md` lists (a) rows of §4 with **no scenario and no case**
   (the union of `covers:` headers and `covers` steps), (b) rows covered only by a case (not portable across implementations), (c)
   scenario `covers` names that are not rows. "Rows with no scenario" is then a number the gate can hold monotonic.
4. **Oracle hook (Clojure 1.12).** Never at run time (1.5 s of JVM per start, a dependency a `fibc test` must not have). A generator
   script `scripts/oracle/clj-table.sh` runs a batch of expressions through
   `java -cp clojure-1.12.0.jar:spec.alpha-0.5.238.jar:core.specs.alpha-0.4.74.jar clojure.main` and prints an `outline` form (the jars
   exist at `~/.cache/fibber-scratch/B4-D/clj/`; the memory note says the scratchpad copy may be gone, the Maven coordinates are in it).
   The output is *checked in*, with a comment naming the Clojure version and the command, so the table is auditable and runs in
   milliseconds. The prototype's `quot` / `rem` table was **checked against the jar** (`7 2 3 1`, `-7 2 -3 -1`, `7 -2 -3 1`,
   `-7 -2 3 -1`: output identical to the table); it was typed by hand from that run, the script is not built.
5. **What keeps a spec stable while implementations change:** (a) the scenario never names an implementation (only the factory); (b) it
   uses protocols, not the type's fields; (c) its ids and covers do not change; (d) the `open` marker (not built) is the only way to
   keep a known gap, and it expires when the gap closes; (e) the spec's *plan* (`--list`) is diffable text.

## 8. The prototype and its output

Files: `lib/fib/test/{core,gen,run}.fib` (249 + 127 + 277 lines, each function under 50 lines but `scenario`'s macro body, which is
inline because a stage-2 macro cannot call a helper); `compiler/tests/harness-proto/{map-contract,alist,map-spec,stress}.fib`,
`cases/001..003-*.fib`, `run.sh`. **Why `lib/fib/test/` for the library and `compiler/tests/harness-proto/` for the example:** the library
is what a user writes specs against and must ship with the release tarball (it is not compiler-internal and must not require
`compiler/`); the example, its planted faults and its self-test are tests of the harness and belong with the compiler's tests. The
`run.sh` there is the harness's own test (`FIBC=... compiler/tests/harness-proto/run.sh`).

### 8.1 The spec as written (21 scenarios: the contract twice, a feature with planted failures, an outline, two properties)

```
$ spec
MapContract[Map]
  pass  an empty map holds no key
  ...(6 pass)
MapContract[AList]
  ...(6 pass)
Vec
  pass  conj appends
  FAIL  a wrong claim is reported, not fatal (planted)
        Given v = [1 2]
      > Then 5 = (count v)
          expected: 5
          actual:   2
        at Vec/a-wrong-claim-is-reported-not-fatal-planted
  TRAP  a trap is isolated (planted)
        exit 134: trap: nth: index out of range
        at Vec/a-trap-is-isolated-planted
  pass  quot and rem follow Clojure's truncation #1 [7 2 3 1]
  ...(4 pass)
  pass  reverse twice is the identity
  FAIL  every vector is shorter than 3 (planted, false)
      > For all x, 100 cases
          expected: the property to hold
          actual:   it fails for x = [0 0 0] (case 5 drew [6 9 15 -14 14], shrunk in 5 steps)
        at Vec/every-vector-is-shorter-than-3-planted-false

21 scenarios: 18 pass, 2 fail, 1 trap, 0 timeout (seed 1)
replay: --only 'Vec/a-wrong-claim-is-reported-not-fatal-planted' --seed 1
replay: --only 'Vec/a-trap-is-isolated-planted' --seed 1
replay: --only 'Vec/every-vector-is-shorter-than-3-planted-false' --seed 1
[exit 1]
```

The wrong claim did not stop the run; the trap (`nth` out of range, exit 134) did not stop the run and the scenarios after it ran; the
false property shrank a 5-element draw `[6 9 15 -14 14]` to `[0 0 0]`, the smallest vector of length 3 (the property is "length < 3").
Seed 7 draws `[11 -5 12]` at case 2 and shrinks to `[0 0 0]` in 3 steps; seed 8 draws a different vector and reaches the same
minimum in 5 (output in `run.sh` check 4: same seed, same report; another seed, a different one).

### 8.2 Plan (`--list`) and JSON

```
$ spec --list --only Vec/quot
Vec/quot-and-rem-follow-Clojure-s-truncation-1-7-2-3-1
    Given a = 7, b = 2, q = 3, r = 1
    Then q = (quot a b)
    Then r = (rem a b)
...
$ spec --only 'MapContract[AList]/assoc-on' --format json        # (planted build, see 8.3)
{"schema":1,"seed":1,"summary":{"total":1,"pass":0,"fail":1,"trap":0,"timeout":0},"results":[{"suite":"MapContract","impl":"AList","id":"MapContract[AList]/assoc-on-an-existing-key-replaces-its-value","text":"assoc on an existing key replaces its value","covers":["assoc"],"status":"fail","message":"","steps":[{"kind":"Given","text":"Given m = (assoc (assoc (make) 1 10) 1 11)","status":"held"},{"kind":"Then","text":"Then (some 11) = (get m 1)","status":"broke","expected":"11","actual":"10"}]}]}
[exit 1]
```

### 8.3 The planted fault: fails when planted, passes when removed

The plant (`run.sh` makes it in a copy; the tree's `alist.fib` is untouched): in `assoc` of the association list the test
`(if (< i 0) ...)` ("key absent: append") becomes `(if (< i 1000) ...)` ("always append"), so a second `assoc` of a key leaves the first
value in front.

```
$ spec-plant --only MapContract
MapContract[Map]
  pass  ...(6 of 6)
MapContract[AList]
  pass  an empty map holds no key
  pass  assoc then get
  FAIL  assoc on an existing key replaces its value
        Given m = (assoc (assoc (make) 1 10) 1 11)
      > Then (some 11) = (get m 1)
          expected: 11
          actual:   10
        at MapContract[AList]/assoc-on-an-existing-key-replaces-its-value
  pass  assoc leaves the other keys alone
  pass  dissoc removes the key and only that key
  pass  dissoc of an absent key changes nothing

12 scenarios: 11 pass, 1 fail, 0 trap, 0 timeout (seed 1)
replay: --only 'MapContract[AList]/assoc-on-an-existing-key-replaces-its-value' --seed 1
[exit 1]
```

Removed (the tree as committed): `--only MapContract` gives `12 scenarios: 12 pass, 0 fail, 0 trap, 0 timeout (seed 1)`, exit 0
(`run.sh` check 2). `run.sh`, in full, with the stage-2 `F` of the tree:

```
ok    1 exit status of a run with failures
ok    1 counts
ok    1 the trap is a row
ok    1 the scenarios after the trap ran
ok    2 contract against both implementations: exit
ok    2 contract counts
ok    3 the plant is in the copy only
ok    3 planted fault: exit
ok    3 planted fault: one failure
ok    3 planted fault: in the AList, in the replace scenario
ok    4 same seed, same report
ok    4 another seed, another draw
ok    5 spec files as cases (verdict = the run's exit value, audit clean in the parent and the forked children)
harness-proto: 0 failed
```

### 8.4 Overhead (measured, this machine, the compiled spec)

```
1000 scenarios: 1000 pass ...   [--no-isolate] wall .007 s
1000 scenarios: 1000 pass ...   [-j 1]         wall .197 s
1000 scenarios: 1000 pass ...   [-j 4]         wall .167 s
1000 scenarios: 1000 pass ...   [-j 12]        wall .101 s
fibc run (JIT), 12 scenarios of the prototype spec: 1.78 s        fibc build of it: 4.72 s
```

### 8.5 Spec files as cases (`F cases compiler/tests/harness-proto/cases`)

```
case                                      status  detail
001-a-holding-spec-is-an-accept-case.fib  pass
002-a-broken-spec-fails-as-result-1.fib   pass
003-a-trapping-scenario-is-isolated.fib   pass

3 cases: 3 pass, 0 fail, 0 pending, 0 header error
```

The first version of this check *failed*: `audit: expected clean, got ... errors=1 [free of an object not live: F 230]`, the fork/trace
hazard of 5.6(a); after the fix, clean.

### 8.6 Refused: duplicate ids

```
fibc test: two scenarios have the same id; give one an (id "..")        [exit 2]
```

## 9. Options rejected

| Option | Why not |
|---|---|
| `describe` / `it` as the primary form | no setup/action/observation split; a contract's reader wants steps (2.1). Allowed degenerate form: a scenario of only a `then`. |
| Gherkin `.feature` files | needs a runtime step-definition table, i.e. exactly the dynamic glue static typing removes; audience (non-programmers) absent (2.4). |
| Contract as a *protocol* of the implementation (a `(defprotocol (Contract s) (make (self) ..))`) | a protocol method takes `self` first; a factory has no self; the witness-value trick (`(empty seed)`) works but forces every implementation to expose a throwaway instance. A function of closures is simpler, needs nothing, and works for non-protocol things (a runner, a server). |
| A global registry (`(implements ..)` pushes to a `def`) | no mutable globals; an order-dependent `def`; the `defspecs` list is greppable and testable. |
| Re-enter the binary by scenario name (`fibc test FILE --only NAME`) | one front end per scenario (0.2 to 1.8 s) vs fork 0.2 ms; kept as the fallback if `fork` ever cannot be used (Windows, or a threaded parent): it needs only `--only ID` and the same file protocol, both of which exist. |
| Threads as the isolation (spawn a task per scenario) | a trap aborts the process (types §2.11); impossible until L28 step 1. Then it replaces `fork`, and the file protocol disappears. |
| Run-time Clojure (shell out to the JVM) | 1.5 s per start, a Java dependency in the harness, flaky in CI; a generated, checked-in table has the same power and none of that. |
| Time-seeded properties | a failure must be reproducible from the log; the seed is fixed at 1 and printed; a soak lane passes another. |
| Shrinking integrated into the generator (Hedgehog-style) | a larger generator API for the benefit of less code per generator; the two-function `Gen` is 10 lines and every shrinker is testable alone. Revisit if user-defined generators become common. |
| Macro-time step texts | impossible: stage-2 macros see only the prelude (0). Run-time printing of quoted forms costs nothing that matters. |
| Spec failures as traps (`(assert ..)`) | aborts the process; no second failure reported; the whole point of 5. |
| `;; covers:` parsed from spec files by `fibc cases` | it would make the spec and the case harness one thing; keep two judges and let `covers` be a shared vocabulary. |

## 10. Not done, risks, decisions for the owner

**Not built** (everything not named in 8): the `fibc test` command (path handling, a stub `main`, `fibc test` returning the run's
status, `--format tap`, `--covers`); the coverage report; the checked-in id list and its gate diff; `(open ..)`; `(call-pos)` and
`file:line`; `Arbitrary` derivation and `gen-f64`; multi-binding `prop`; stdout capture of children; the Vec contract, the
interpreter contract, the language-server contract, fibgen contracts; a matrix report; the oracle generator script; any gate edit or
mutant-script edit (recommendations only). The prototype ran on `F` from the RR1 tree (`rr1/tools/F`) with `FIB_LIB` set; **it was not
built with the seed v0.1.5** (`unchecked-add` is not there: gen.fib needs a stage 2 from this tree, as `compiler/gen` does) and not on the Mac.

**Risks.** (1) `fork` in a parent with a started thread pool is undefined for the child; the runner starts none, but a spec file's
top-level `def` that spawns would. (2) `scenario`'s macro body is one 80-line function (a stage-2 macro cannot call a helper): it passes
and is the least readable code of the prototype. Fix: allow a macro to `:use` a *macro-time library* module (a decision in
`macros/runner.fib`: the macro module may include the requiring module's `defun`s that use only the prelude), after which `form.fib`-style
helpers return. (3) Compile cost: each `scenario`/`prop`/`outline` is a macro run (the 29-line spec compiles in 1.8 s under the JIT);
a spec file with hundreds of scenarios will be dominated by expansion, not by running. Measure before choosing a file size.
(4) `pr-str` of both sides needs `Debug`; a type without it cannot be an operand of `expect` (use a boolean).

**Decisions for the owner.** (a) Approve `feature`/`scenario`/`given`/`upon`/`then`/`expect` as the surface (2.1). (b) `lib/fib/test/` as
the home, with `fib.os.process` gaining a fork/spawn primitive so `driver.proc` and the runner share it (5.3). (c) A macro builtin
`(call-pos)` (6.3). (d) Whether `fibc test` is a compiler command (a Rust-free, fibber-only addition to `compiler/driver`, ~150 lines
around `run-main`) or a script around `fibc build`; the first is recommended because `fibc run` loses the exit status. (e) Moving
`gen.rng` into the library so fibgen and `fib.test.gen` share it (4). (f) Whether the gate's `specs` stage fails the gate (6.5: yes).

## 11. Package plan (not started)

| Step | Work | Test |
|---|---|---|
| S1 | land the prototype as is (it is committed on this branch); add `fib.os.process` fork-run; `fibc test` over `run-main` | `harness-proto/run.sh` plus one `driver` test that `fibc test` returns 1 for the broken spec |
| S2 | `specs/` directory: Vec and Map contracts first (they have two implementations the day a second persistent vector exists), stdlib rows with Clojure tables | planted fault in each implementation; `--covers` reports rows with no scenario |
| S3 | `specs` stage in `scripts/gate.sh` (quick and full); mutant scripts name specs | gate PASS; each mutant kills a spec |
| S4 | `(call-pos)`, `file:line` in JSON, VS Code test-explorer glue (the editor already reads LSP; a `.fib` spec's `--format json` is the data) | JSON schema test |
| S5 | the interpreter and language-server contracts (fibref-port.md, LS) | the same scenarios run against both |
