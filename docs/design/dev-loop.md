# The development loop: from edit to result

Status: design 2026-10-04; **sections 8 to 11 (2026-10-07) record what was built: the compile server `fibc serve`, its client, the REPL `fibc repl`, with measurements, the protocol and what remains.** Sections 0 to 7 are the design as written, unchanged except where section 8 says it differs. Docs only up to section 7; no `.rs` or `.fib` source changed by it. It replaces `docs/design/fibber-interpreter.md` (written for the
rule 6 oracle goal) now that the owner has decided the interpreter is a development tool ("Rule 6 can suck it. We're past that. The
interpreter is a development tool. It helps agents and meat developers as quickly as possible", `decisions-2026-10-04.md`, amendment).
The old document's independence, divergence table, `compare` and audit-fidelity machinery are dropped; its speed measurements (section
5) and sizes (section 8) are reused where they still apply.

The question: what gets an agent or a human from an edit to a result fastest? This document measures first, then compares four
options with the numbers, then recommends. **Measured** means a command was run in this session and its output is quoted. **Estimate**
means derived from measured numbers by the stated arithmetic. Sections 0 to 7: nothing there is implemented (DV1 was, see `git log`); sections 8 to 11: built.

Contents: 0 the short answer; 1 method; 2 measurements; 3 the four options; 4 errors, tests, editor, notebooks; 5 recommendation and
package plan; 6 decisions for the owner; 7 what was not measured; 8 the compile server; 9 the REPL; 10 tests and planted faults; 11 what remains.

## Original plan (sections 0–7; historical)

Unmarked code fences are sketches or dated transcripts; use the current
[tooling guide](../guide/tooling.md) for commands.

## 0. The short answer

1. The cost of a run is not the JIT. For a 2-line program, 100 of 152 ms is the front end, most of it the library; for a 176-line
   program 229 of 447 ms is the front end, 172 ms is LLVM code generation. At `-O 0` the JIT uses LLVM's full instruction selector;
   the compiler's own sessions already use the fast one (FastISel) and `fibc run` does not. **One line** (measured with a scratch
   build) takes the 176-line program from 501 to 330 ms.
2. The front end is the floor, and it has two parts: the library (about 70 ms of every run, even for `(defun main () -> i64 0)`) and
   the user's own code (about 0.7 ms per line: 0.46 ms to type and check ownership, 0.2 ms to lower). A cache of the library helps
   every command; only per-definition incrementality helps the edit loop.
3. A tree-walking interpreter does not fix either. It reuses the same front end, so it pays the same floor, and it loses on compute by
   a factor the old design put at 100 to 300. Measured with the frozen Rust interpreter: it beats the JIT by 90 ms on `hello` (66 ms
   against 157 ms) and loses by 170 ms on a 76-line program doing small loops (547 ms against 375 ms). **Option C is not
   recommended.**
4. Recommended: **A now, then D with an incremental engine, then B on that engine**: (A) fast JIT path, a `--time` phase report,
   `fibc check --json`; (D) a forking compile server that keeps the library's checked state, with `cases --watch` on it; (B) a REPL,
   a stage 2 language server and a notebook controller as clients of the same engine. The first package, **DV1** (fast code
   generation for `fibc run` and a benchmark row that can fail), is about 10 lines and takes `fibc run` of a 176-line program from
   501 to 330 ms. The biggest win, DV6 (per-definition incremental checking), is the large package; the estimate for an edit of one
   function in a warm server is under 30 ms, against 447 ms now.

## 1. Method

Machine: the dev host, 28 cores, 61 GB, shared with other agents; every timing run held `/tmp/fibsuite.lock`; `ulimit -v 16000000`.
Wall time by `date +%s%N` around the command, **median of 7 runs** (the `min` is also kept in the scratch outputs). Expect 10% noise:
`F run -O 0 p200` was 447 ms in one batch and 501 ms in another. Scratch: `~/.cache/fibber-scratch/dev-loop/` (`F`, `Ffast`,
`Finstr`, `p/*.fib`, `m1.sh` to `m9.sh`, `*.out`).

Compiler under test: stage 2 built from this tree (main at c8f12f9) by the seed named by `SEED` (v0.1.5):

```
scripts/fetch-seed.sh ~/.cache/fibber-scratch/dev-loop/seed                      # 2.1 s
fibc build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F   # 57.1 s wall, F is 7.3 MB (LLVM shared)
```

Every timed command ran with `FIB_LIB=<tree>/lib` (stage 2 reads `lib/prelude.fib` from there; the library is not embedded in this
`F`), except the Rust `fibref`, which carries its own older library and was run without it. `F` is `fibc`; `lairf` (13.5 s to build with
`F build compiler/lairf.fib ...`) is the stage 2 tool for `.lir` files; `read`, `expand`, `types`, `own` are the pass tools
(`compiler/*.fib`, 2 s to 55 s to build).

Programs (in `~/.cache/fibber-scratch/dev-loop/p/`):

| Name | Lines | What |
|---|---|---|
| `empty` | 2 | `(ns main) (defun main () -> i64 0)` |
| `hello` | 2 | `scripts/bench/hello.fib`: `(println "hi")` |
| `pu` | 133 | all user code: the n-body of `scripts/shootout/n-body/n-body.fib` (arrays, structs, `fib.math`, `fib.fmt`) and an `Expr` enum evaluator; loops of 1000 |
| `p200` | 176 | `pu` plus protocol dispatch over an enum, closures, `str/split`, `str/join`, `(Map i64 i64)`, `Vec`, `reduce`; all loops of 1000, so the run itself takes under 10 ms |
| `pold` | 76 | the part of `p200` that the Rust interpreter's older library can read (no `fib.math`, no `fib.fmt`) |

`p200` is 176 lines, not the 200 asked for; the cost is linear in lines (section 2.4) so scale if needed.

## 2. Measurements

### 2.1 Whole commands

Every row: `bash time.sh 7 LABEL CMD`, median of 7; commands run in the scratch directory.

| Command | `empty` | `hello` | `p200` |
|---|---|---|---|
| `F --version` (process start) | 9 ms | | |
| `F run -O 0 p/X.fib` | 109 | 152 | 447 |
| `F run -O 1 p/X.fib` | 110 | 161 | 892 |
| `F run -O 2 p/X.fib` | 105 | 176 | 996 |
| `F run -O 3 p/X.fib` | 106 | 163 | 1040 |
| `F emit p/X.fib` (front end, lIR text out) | 82 | 110 | 248 |
| `F explain p/X.fib` (checks the whole library) | 296 | 317 | 339 |
| `F build p/X.fib -O 0 -o p/X.bin0` | 121 | 144 | 346 |
| `F build p/X.fib -O 2 -o p/X.bin2` | 123 | 167 | 1041 |
| `./p/X.bin2` (the built program) | 2 | 2 | 4 |

(`m1.out`.) Two readings: the `-O` level does nothing for a program of two lines and costs 450 to 550 ms on a 176-line one (the IR
optimiser over the library code the program instantiates); and `build -O 0` is cheaper than `run -O 0` for `p200` (346 against
447 ms) because the JIT path generates code with the slower instruction selector (section 2.3).

`lairf` on the emitted text (`F emit p/X.fib > p/X.ll`; `lairf run -O N p/X.ll`, `m2.out`): `check` (parse and check the lIR) 15, 13, 34
ms; `emit-llvm` 18, 18, 56 ms; `run -O 0` 34, 55, 250 ms; `run -O 2` 36, 65, 786 ms. The lIR of `p200` is 14,369 lines, of `hello`
2,483, of `empty` 2,345: the runtime and tables are there for every program.

### 2.2 Where the time goes: phases

A scratch copy of `compiler/` (`exp/`) with `eprintln` ticks of `sys-clock-now` in `driver.front` `front-check` and `lower-kind` and
in `driver.native` `run-lir` (the patch is `instr.py` in the scratch directory; ticks named in the table), built with the seed-built
`F` (57 s), `Finstr run -O N p/X.fib`, three runs each, the deltas between ticks, in ms (`m4.out`, `m5.out`):

| Phase (tick that ends it) | `empty` -O 0 | `hello` -O 0 | `pu` -O 0 | `p200` -O 0 | `p200` -O 2 |
|---|---|---|---|---|---|
| prelude read and expanded (`prelude-forms`) | 7 | 5 | 5 | 5 | 6 |
| user module loaded, all modules expanded (`expanded`) | 33 | 31 | 42 | 44 | 44 |
| typed and ownership-checked, on demand (`typed+owned`) | 31 | 58 | 92 | 134 | 137 |
| lowered to lIR text (`emitted`) | 2 | 2 | 29 | 46 | 47 |
| JIT session made (`jit-new`) | 2 | 2 | 3 | 4 | 5 |
| lIR text parsed, checked, lowered, IR passes (`add-source`) | 3 | 5 | 11 | 30 | 440 |
| machine code (`c-entry`: trampoline and lookup) | 5 | 20 | 56 | 172 | 297 |
| `main` called and returned (`main returned`) | 14 | 14 | 0 | 3 | 1 |
| process start (`F --version`, not a tick) | 9 | 9 | 9 | 9 | 9 |
| sum, against the wall time of section 2.1 | 106 vs 109 | 146 vs 152 | 247 | 438+9 vs 447 | |

Findings:

- **The library is a fixed cost of about 70 ms**: `empty` spends 7 + 33 + 31 = 71 ms reading, expanding and checking library units
  before it has any code of its own (the demand-driven check of PERFB-B already cut this from 0.356 s to 0.185 s for `emit`; ROADMAP).
  `hello` adds 26 ms to check what `println` reaches (`Show`, strings, I/O).
- **User code costs about 0.7 ms per line in the front end.** `pu` is 133 lines of code that touches almost no library: its `typed+owned`
  is 92 ms against 31 for `empty` (0.46 ms per line) and its lowering 29 against 2 (0.20 ms per line).
- **Library instantiations are most of what `p200` adds to `pu`**: 43 lines more, but `typed+owned` +42 ms, lowering +17 ms,
  machine code +116 ms (the maps, vectors, `split`, `join`, `reduce` and closures it instantiates).
- **At `-O 0` the JIT's code generator is the biggest single phase of a mid-size program** (172 of 447 ms), and at `-O 2` the IR
  optimiser is (440 ms). The text round trip (lower to text, parse, check) is 30 to 80 ms for `p200`: `add-source` 30 ms plus part of
  `emitted` 46 ms. `native.api` already has `jit-add-module` that takes the AST and skips the text; `fibc run` does not use it.
- **A constant of about 14 ms** appears under `main` for the two tiny programs and not for `pu` and `p200` (where it seems to move into
  `c-entry`). It is not attributed (ORC materialising the entry, or the call thunk of `native.call`); it is the first thing the
  `--time` report of DV0 should explain.
- The run itself is negligible in all of these (the loops are small): the cycle is compile-bound until a program computes for
  seconds, where `-O 1` pays (section 2.3).

### 2.3 Code generation: the fast path that exists

`compiler/native/jit.fib` has two sessions: `session-new` (LLJIT's default target machine, whose instruction selection is the full
SelectionDAG) and `session-new-fast-codegen` ("the code generator at level None whatever `opt` is (FastISel): for the compiler's own
sessions"). The macro runner (`macros/runner.fib:78`) and `def` evaluation (`emit/defs/jit.fib:241`) use the fast one; `fibc run`
(`driver/native.fib:115`, `run-lir`) uses the other. Experiment: the one-line change `jit/jit-new` to `jit/jit-new-fast-codegen` in a
scratch copy, built as `Ffast` (57 s).

| Command (`m3.out`, `m6.out`, `m9.out`) | stock `F` | `Ffast` |
|---|---|---|
| `run -O 0 p/empty.fib` | 120 ms | 111 ms |
| `run -O 0 p/hello.fib` | 149 (157 in a second batch) | 149 (134 in the second batch) |
| `run -O 0 p/p200.fib` | 501 | **330** |
| `run -O 0 p/pold.fib` | 375 | 265 |
| `run -O 2 p/p200.fib` | 1047 | 831 |
| `run -O 0 n-body.fib -- 100000` (about 0.1 s of compute) | 337 | 284 |
| `run -O 0 n-body.fib -- 1000000` (about 0.5 s of compute) | 745 | 876 |
| `run -O 1 n-body.fib -- 1000000` | 471 | 564 |

The fast path is correct on the corpus: `Ffast cases cases/ownership -j 12`: 305 pass (14.5 s, stock 14.6 s); `Ffast cases cases/modules
-j 12`: 27 pass (0.75 s, stock 0.82 s). The generated code is about 1.5 times slower (n-body at one million steps: 876 against 745
ms; at `-O 1` 564 against 471). The crossover is therefore about a second of compute: below it FastISel wins, above it `-O 1` wins
whatever the front end costs. So `run` should default to fast, and `-O 1` should be what a long computation asks for; a tiering
policy (run fast, re-JIT hot functions) is not needed until a measurement shows a case that wants it.

Lazy per-function JIT (compile a function when first called) is available through `LLVMOrcLazyReexports` and
`LLVMOrcCreateLocalLazyCallThroughManager` in `llvm-c/Orc.h` (LLVM 21), which `compiler/llvm/orc.fib` does not bind. It needs the module
split into one module per function. **Estimate of its value after FastISel: at most 55 ms of 330 ms** for `p200` (330 less the 9 ms start,
the 263 ms of phases before code generation and `main`'s 3 ms leaves 55), and only the fraction of functions that never run. Not worth
doing before the front end is cheaper.

### 2.4 The cost of the library and of the user's code, summarised

Per run, from section 2.2: library fixed floor 71 ms (empty); what `hello` reaches +26 ms front end and +15 ms code generation;
user code 0.46 + 0.20 ms per line in the front end and about 0.44 ms per line to generate code at `-O 0` (`pu`: (11 + 56) - (3 + 5)
ms over 133 lines; with FastISel roughly a third of that, about 0.15). These give the estimates in section 3.

### 2.5 The Rust interpreter, as a reference point

The frozen `fibref` in the seed tarball (`seed/fibc-0.1.5-linux-x86_64/bin/fibref`) is a tree-walking interpreter with a reference
front end. It carries the seed's library, which lacks `fib.math` and `fib.fmt`, so `pu` and `p200` do not run on it
(`rejected: module fib.math is not at p/fib/math.fib`) and `pold` was cut for it (`m6.out`):

| Command | Time |
|---|---|
| `fibref run p/hello.fib` | 66 ms (`F run -O 0` 157) |
| `fibref run p/pold.fib` | **547 ms** (`F run -O 0` 375, `Ffast` 265) |
| `fibref explain p/hello.fib` | 65 ms (`F explain` 317) |
| `fibref diagnostics p/pold.fib`, `fibref complete p/pold.fib 40 5` | 78 ms, 71 ms |
| `fibref cases cases/ownership --only 01-` | 4 ms |

Two readings. The interpreter's cold start is 2.4 times better than the JIT's on `hello`, because the Rust front end checks a smaller
library faster; and a program whose loops run a few thousand iterations already costs more to interpret than to compile and run. The
old design's own spike (section 5.1 of `fibber-interpreter.md`: 34 to 170 million tree nodes a second for a toy walker, 2 to 8
million planned for the real one, "100 to 300 times slower than native") agrees. And the Rust language server (`fibref lsp`,
`spec/bootstrap.md` section 9) cannot read the current library: `fibref diagnostics p/p200.fib` with `FIB_LIB=<tree>/lib` returns
`unbound name fib.prelude/cell-update!` for `lib/fib/core/cells.fib:23:4`, and without it rejects `fib.math`. The editor pack is frozen
at the seed's language.

### 2.6 Test running

`fibc cases` runs each case as a child `fibc run --trace` (`compiler/driver/child.fib`), so a case is a whole `fibc run`:

| Command (`m7.sh`, `m8.sh`) | Wall |
|---|---|
| `F cases cases/ownership --only 01-` (1 case) | 150 ms |
| `F cases cases/ownership --only 0 -j 1` (9 cases) | 1505 ms (167 ms per case) |
| `F cases cases/ownership --only 0 -j 12` | 369 ms |
| `F cases cases/ownership -j 12` (305 cases) | 14,712 ms (47 ms per case) |
| `F cases cases/modules -j 12` (27 cases) | 821 ms |

At `-j 12` the suite is 3.4 times faster than sequential (305 times 160 ms is about 49 s), not 12 times; the reason was not looked
for (memory bandwidth, the shared host, process start). FastISel does not help here (14.5 s either way): the cases are tiny and the
front end is the cost, which is the 70 ms library floor and the 15 to 60 ms of the case's own code, repeated 305 times.

### 2.7 Errors today

Programs `p/e1.fib` to `p/e5.fib` (`err.sh`): a type error, an unbound name, a use after move, a trap, an unclosed paren.

| Program | `F run` output | Exit |
|---|---|---|
| `(defun f (x: i64) -> str x)` | `rejected:` then `p/e1.fib:2:26: cannot unify i64 with str` | 3 |
| call of an unbound `g` | `rejected:` then `p/e2.fib:2:37: unbound name g` | 3 |
| `(take p)` then `(. p a)` | **runs and prints `x`**, exit 0 (the ownership checker accepts it: `take` borrows `p`, see `F explain` below) | 0 |
| `(nth [1 2] 5)` | `trap: nth: index out of range`, no function, line or stack | 134 |
| `(println "a" ` | `rejected:` then `p/e5.fib:2:27: unclosed (` | 3 |

(e3 is a bad test of mine, not a finding: a borrowed parameter is correct. The point is `explain`: `F explain p/e3.fib` prints
`params: p borrowed escapes=no` and per call site `@4:44 (take ..) call arg 1 p: borrow`, which is what an agent needs to see why.)
What is missing for an agent: a machine-readable form (no `--json`), a span end, the expected and actual types as fields, a
code per error kind, a trap location and call stack (`--trace` is the allocation trace, `A n k` and `F n` lines, not a stack),
and more than the first error.

## 3. The four options, with the numbers

Estimates are derived from sections 2.2 to 2.4. "Warm" means a server that already holds the library's expanded, checked and
lowered state and its generated code.

| | A: fast path (fast codegen, cached library image, `check`) | B: REPL on `native.*` | C: tree-walking interpreter | D: compile server |
|---|---|---|---|---|
| `hello`, cold | 152 to 134 ms (measured); about 100 with a cached expanded library (estimate: minus the 36 ms of `expanded`) | the REPL starts once: 150 ms, then per input | about 90 ms (estimate: the front end's 71 ms floor plus a start) | client to server and back: about 10 to 20 ms (estimate), warm |
| `p200` fresh file | 501 to 330 in one batch (measured; 447 stock in another), about 290 with the library image (estimate) | the same engine as D | 447 minus the 205 ms of JIT = about 240, then compute at 100 to 300 times slower | about 190 ms (estimate: below) |
| edit one function, run | same as fresh file | per input: only the new definition (estimate 20 to 50 ms) | same as fresh file | under 30 ms with DV6 (estimate: below); about 190 ms without |
| compute-bound program (seconds) | native speed at `-O 1` | native | 100 to 300 times slower: unusable for the scientific goal | native |
| state across runs | none | yes: session | none | the library only (DV3) or every definition (DV6) |
| new code | about 300 lines | about 700 lines on D's engine; about 2,500 without it | about 4,000 lines (estimate: the old plan's 7,350 plus 1,050 tests, less the audit, compare, tasks and heap fidelity) | about 500 lines, plus the refactor DV3/DV6 |
| risk | low | medium: redefinition, ownership across inputs | high: two evaluators to keep equal to the language | medium: stale state, crashes |

**A, per piece, measured and estimated.** Fast codegen: measured, 501 to 330 ms (`p200`), 337 to 284 ms (n-body, 100k steps). A cached
expanded library: it removes `prelude-forms` and the library part of `expanded`, 38 to 49 ms per run (all programs; `pu` and `p200`
spend 5 to 11 ms of their 42 to 44 ms on their own code, the rest is library). A cache of checked library units, which would take the
31 ms of `empty` and the 26 ms of what `hello` reaches, is a serialisation of the checker's tables and of the ownership facts: much larger,
and the server (D) gets it for free by not exiting. So A's own ceiling is about two times (`hello` about 100 ms, `p200` about 290 ms).

**D, derived.** A fresh 176-line file on a warm server: client and dispatch about 10 ms, the file's own front end (0.7 ms per line: 120
ms), its own code generation (about 0.15 ms per line with FastISel: 26 ms), and the lIR text and
`add-source` for the user's functions only (about 10 ms): **about 150 to 190 ms against 447 ms now**, a factor 2.5 to 3. That is a
real but not a transformative gain, because the user's own 176 lines are 60% of the cost. **The transformative gain is
incremental**: a changed function (about 15 lines) costs 15 times 0.7 ms = 10 ms of front end, a few ms to generate, a few to
dispatch, plus the cost of finding what depends on it: **under 30 ms (estimate)**, a factor of 15 against 447 ms, and the same figure
for `hello`'s class of program. Unmeasured assumption: the checker and the ownership pass can be run per strongly connected component
of definitions with the library's results held fixed. The types dump shows that the checker already works in components (`unit scc
main`) and `types.infer.demand` already selects units, which is why DV3 starts with a spike and a go/no-go (section 5).

**B**: a REPL needs the same engine to be fast. Without per-definition incrementality each input re-runs the whole front end over
the session's definitions: at least the 70 ms floor plus 0.7 ms per line of the session, so 100 to 250 ms per input once a session has a
screenful of definitions: usable, not fast. With the engine it is the cost of the new input. What the REPL adds is session state
(section 4.4).

**C**: reuses the front end (so the 71 ms floor stays), saves the roughly 40 ms of JIT for a tiny program and about 205 ms for `p200`,
then loses at run time by two orders of magnitude. For the goal in the decisions document ("a language that destroys Python at AI and
scientific work") an interpreter is the wrong tool for the loop that has tensors in it. **Not recommended.** What would change this: a
measurement showing that the JIT's materialisation, not the front end, was the cold-start floor; section 2.2 shows the opposite.

## 4. Cross-cutting

### 4.1 Errors and traces for agents

- **`fibc check [--json] FILE`**: the demand-driven front end (`front/check-file` with `demand` true) and no lowering, so about
  `F emit` minus the emit phase (`hello` about 80 ms today, less with the library cached). Exit codes as `run`. With `--json`, one
  object per line on stdout:
  `{"severity","code","file","line","col","endLine","endCol","message","expected","found","notes":[{"file","line","col","message"}],"hint"}`;
  `expected` and `found` are present for a unification failure, `notes` carry "borrowed here" and "moved here" for an ownership
  error, `code` is a stable name per error kind (`E-UNIFY`, `E-UNBOUND`, `E-MOVED`, ...). The checker stops at the first error today
  (section 2.7); the schema is a list so later work can report more. This is the same shape as `fibref diagnostics` (spec section 9:
  `{"diagnostics":[{"line","col","endLine","endCol","message","severity"}]}`) plus the fields an agent uses, so the editor and the agent
  consume one thing.
- **`fibc explain --json`**: the ownership decisions as JSON (the text form of `explain` is already complete: params, bindings, calls
  with `moved`/`borrow`); a stable record per call site.
- **Traps with a stack.** `trap: nth: index out of range` names no place. The cheap form: frame pointers kept at `-O 0` and a walk of
  the frames at the trap, each return address mapped to a function name through the symbols the JIT knows (ORC can resolve an address
  to its symbol); output `trap: nth: index out of range` then `  in f  (p/e4.fib)` per frame. No line numbers in v1 (call-site tables
  later). About 200 lines in the runtime (`rt/`) and the driver. The `--trace` allocation trace stays what it is.
- **`--time`**: the phase report of section 2.2 as a permanent option (`fibc run --time f.fib` prints the ticks on stderr). It is how
  every later package states its win and how this document's numbers are reproduced.

### 4.2 Test running: latency and a watch mode

Today (section 2.6): 150 ms for one case, 14.7 s for the 305 ownership cases at `-j 12`. Two changes:

- **In-process cases on a warm server.** The harness would send each case to the server, which forks a child that compiles the case
  against the warm library state and runs it. **Estimate**: a case drops from about 160 ms to about 90 ms of CPU (the 71 ms floor
  largely gone, the rest the case's own code and the JIT), so the 305 cases take about 8 s at the present parallel efficiency; at 12
  times parallel efficiency, about 2.4 s. The harness's outcomes (verdicts, tables) are unchanged because the child runs the same
  `run --trace`.
- **`fibc watch [DIR..]`** (and `fibc cases --watch DIR`): poll `stat` once a second (vanilla, no inotify binding needed), and when a
  file changes re-run the cases that load it. The front end already knows which modules a case loads (`mods/try-load-in`); the server
  keeps that map. A change to one library file reruns the cases that load it, which for a leaf module of the library is tens of cases:
  **estimate 0.3 to 1 s** to see the table, against 14.7 s for everything. The first line of output is the table row of the case that
  was just changed, so the agent that edited a file sees its own result first.

### 4.3 The editor: completion, hover, go to definition

`editors/vscode` is the language pack, and since 0.2.0 it starts `fibref lsp` (Rust, frozen at the seed's language, section 2.5);
it cannot read the current library. A stage 2 `fibc lsp` over the same engine as D gives: diagnostics on each change (the user
module's front end, 0.7 ms per line: 120 ms for a 176-line file, then less with DV6), completion (the spec's candidates:
locals, module names with their checker schemes, fields after `(. x`, types after `:` and `->`, from the checked tables with a degrading
fallback when the buffer does not check), hover (the scheme and the declaration, which the checker's tables hold: the types dump gives
`fun main : (fn :send () i64) params 2:1 10..54`), and go to definition (the span of the definition, already in those tables; for
library functions the file and the position). **The oracle exists**: `spec/bootstrap.md` section 9 defines the protocol and
`fibref complete` and `fibref diagnostics` print the JSON, so the new server is tested against the Rust one on every program the
older language can express (golden JSON), and gets new cases for what the Rust cannot read. Latency target: a diagnostic in under 150
ms for a file of 200 lines, hover and definition in under 20 ms (they read the held tables).

### 4.4 The REPL (B) and notebooks

What a REPL needs on `native.*`, with what exists:

- **A persistent session.** `native.jit` keeps one LLJIT with `names` (every exported name so far). `session-add-module` adds modules
  to it, and `emit.defs.jit` and `macros.runner` already hold sessions across calls. New: redefinition. `names-check` refuses a name
  a session already defines, so the REPL gives each input its own resource tracker (`LLVMOrcJITDylibCreateResourceTracker`,
  `LLVMOrcResourceTrackerRemove`, in the same header, to be bound) and versions names (`f` is `f#2` for later inputs, and the inputs that
  call it are recompiled when it changes: the cost is the incremental engine of DV6, not the JIT).
- **Ownership across inputs.** A top-level `def` in the REPL is a session-owned value; a later input may borrow it and may not move it
  away (the same rule `def` constants have in a program). `(take! x)` (or `(clone x)`) is how an input consumes it; the rule needs a
  decision (section 6) and an error text, and the checker needs one new fact (the borrow root of a session value is immortal for the
  input's duration).
- **Printing.** An input that is an expression is wrapped as `(show e)` with the type from the checker; the printed value is a
  temporary that drops after the print, so nothing leaks. `Show` already exists for the library's types; tensors need a numpy-shaped
  `Show` with elision (`[[1. 2. 3.] ... [7. 8. 9.]]` past a size). That is library work with its own cases and helps `cases` too.
- **A `--json` stdio protocol** (one request per line, one response per line: `{"id","out","value","type","error"}`), so that an
  editor, an agent or a notebook speaks to it without a terminal.

Notebooks, scientific use: no Jupyter kernel needs to exist in v1. The extension in `editors/vscode` (vanilla JavaScript, no build step)
can register a notebook controller that spawns `fibc repl --json` and shows `value` as text, and `{"mime":"image/svg+xml"}` as an
inline image when the library gets `fib.plot` that returns an SVG string (plots later, as asked). A ZeroMQ Jupyter kernel is a thin
shim over the same protocol if a Jupyter user ever asks. A notebook file is the REPL's inputs plus outputs in a JSON file; "run all" is
`fibc repl --json` fed the inputs, which is also an executable test.

## 5. Recommendation and package plan

Recommend **A, then D with the incremental engine, then B and the editor on it**. Not C. Reasons, in the numbers: the JIT is half of
a medium run and cheap to halve (A); the other half is the front end whose library part a long-lived process removes and whose own part
only incrementality removes (D); a REPL, a language server and a watch mode are three clients of that one engine (B, 4.2, 4.3); an
interpreter is a second evaluator that shares the front end's cost and loses on compute (C).

All of it is fibber in `compiler/` and `lib/`; the Rust stays frozen (CLAUDE.md). Sizes are fibber lines, files under 500 lines,
functions under 50. Each package is independent files plus its own tests, and each test is one that can fail (the mutant is named).

| Pkg | What | Size | Tests that can fail | Depends on |
|---|---|---|---|---|
| **DV1** | **`fibc run` uses the fast code generator at `-O 0`**; `scripts/bench/dev-loop.sh`: the table of 2.1 (`empty`, `hello`, a 176-line program, `cases/ownership -j 12`) against a recorded baseline, flagging a delta over 10% as `quick.sh` does. | 10 lines, 80 lines of shell, the 176-line program | differential: `compiler/tests/driver/fastrun.sh` runs the 100-case stdlib sample of `scripts/gate.sh --quick` and the n-body shootout at `-O 0` and at `-O 2` and diffs stdout and exit status (a miscompile by FastISel fails it); the bench row fails if the change is reverted (mutant: put back `jit-new`, the `p200` row goes from 330 to 500 ms, over 10%) | none |
| DV0 | `--time` phase report: `driver/timing.fib`, ticks at the eight places of 2.2; explains the 14 ms of `main` | 80 lines | stderr has the eight lines in order, each an integer, their sum within 5% of the wall time (a dropped tick fails it); with `--time` off nothing is printed (a golden of stderr on `hello`) | none |
| DV2 | `fibc check [--json]` and `explain --json`; error records get a span end and a code (`check-error-text`'s inputs); the schema of 4.1 | 350 lines | golden JSON for `p/e1`, `e2`, `e5` and one case of each rejection class of `cases/ownership` (the case headers fix the verdict, so the JSON's line and column must equal the header's); `check` and `run` agree on accept and reject over the 1,233 programs of the PERFB-B comparison (a `check` that is laxer fails it) | none |
| DV3 | Spike, then refactor: `front-check` split into library state (prelude, expanded library modules, library unit results) and user module, with no behaviour change; the spike keeps the state in one process and compiles `hello`, `pu`, `p200` twice, printing both times | spike 100 lines, refactor 500 | spike go/no-go: **go if the second compile of `hello` is at least 50 ms faster in-process**; after the refactor `F emit` byte-identical to the unrefactored `F` over the 1,233 programs, and the fixed point holds (`scripts/gate.sh --full`) | DV0 |
| DV4 | `fibc serve` and the client: a unix socket in `$XDG_RUNTIME_DIR`, line JSON requests (`check`, `run`, `cases-one`, `lsp`), a **forked child per run** (isolation, copy-on-write warm state, the client's stdin, stdout and stderr passed with `SCM_RIGHTS`), `fibc` falling back to in-process if no server answers | 500 lines | `fibc cases` through the server gives the same table as in-process (305 of 305); the client falls back when the server is killed mid-request (kill it and run); an invalid request returns an error and the server survives; a program that traps leaves the server up | DV3 |
| DV5 | `fibc watch` and `cases --watch`: a poll of file times, a case to modules map from the loader, the rerun of the cases that load a changed file | 250 lines | touch a leaf module: only the cases that load it rerun and the table rows equal a full run's rows for them; touch an unrelated file: nothing reruns (both fail on a wrong map) | DV4 |
| DV6 | **Per-definition incrementality**: the checker, the ownership pass and the emitter memoised by component of definitions, the library's results fixed, the dependents of a changed definition recomputed; first on the type checker, then ownership, then emit and a per-function JIT module | 1,500 to 2,500 lines, in three steps | the edited program's `F emit` equals a from-scratch `F emit` byte for byte after any sequence of edits (a randomised edit script over the corpus, `fibgen` programs, the case headers' verdicts); a deliberately stale memo (mutant: skip invalidating dependents) is caught by that script | DV3 |
| DV7 | `fibc repl [--json]` on `native.*`: session, redefinition by resource trackers, the ownership rule for session values, `show` of an expression, the stdio protocol | 700 lines | a transcript of inputs and outputs per feature (redefinition, a consumed value, a type error that leaves the session usable, a trap that does too); every transcript is also run as a program and must print the same | DV6 (fast), or DV4 alone (slow, 100 to 250 ms per input) |
| DV8 | `fibc lsp`: diagnostics, completion, hover, definition on the held tables; then the pack points at it instead of `fibref lsp` | 900 lines | golden JSON from the Rust `fibref complete` and `fibref diagnostics` on the programs the older language can express (the `spec` section 9 oracle); a latency bound of 150 ms diagnostics on a 200-line file (flaky tests are not used: the bound runs in `bench`) | DV2, DV4 |
| DV9 | Trap stacks: frame pointers at `-O 0`, the symbol walk, the driver prints frames | 200 lines | a trap three calls deep prints the three names in order (a golden on `p/e4`-like programs); exit status 134 unchanged for every trap case in `cases/` (the gate) | none |
| DV10 | A numpy-shaped `Show` for tensors (elision, shape header); `fib.plot` to an SVG string; the notebook controller in `editors/vscode` | 150 lines of library, 200 of JavaScript | `cases/stdlib` cases whose expected output is the exact text (a one-element difference fails it); the extension's `npm test` spawns `fibc repl --json` and runs a three-cell notebook | DV7, tensors (`simd-and-tensors.md`) |

**The first package is DV1.** It is one line plus a benchmark row, it removes 120 to 170 ms from a 176-line run (27% to 34%), it costs
nothing in the compile and it comes with a differential test that fails on a miscompile. DV0 and DV9 are independent and small and
can run alongside it; DV2 is the next most useful for agents and independent of the server. **The biggest win is DV3 to DV6**: DV3's
spike decides within a day whether the server is worth building (go if a second in-process compile of `hello` saves 50 ms or more,
which section 2.2 predicts at about 70).

Order of value for an agent's edit loop: DV1 (about 450 to 330 ms), DV3 to DV4 (to about 190 ms), DV6 (to about 30 ms), DV5 (the suite in
under a second for a leaf change). For a human in an editor: DV2, DV8. For science: DV7, DV10.

## 6. Decisions for the owner

1. **`-O 0` means fast to compile.** `fibc run` at `-O 0` takes FastISel (code about 1.5 times slower than now at `-O 0`); `-O 1` is
   what a long computation asks for. `fibc build` is unchanged (it defaults to `-O 2`). Yes or no.
2. **A server is acceptable**: a long-lived `fibc serve` process (opt-in at first; the client falls back in-process), forking per run.
3. **The JSON diagnostics schema of 4.1**, including that errors get a stable `code` per kind.
4. **The REPL's ownership rule for session values** (4.4): borrowed by later inputs, consumed only by an explicit `take!` or `clone`.
5. **`docs/design/fibber-interpreter.md` is superseded** by this document; the interpreter decisions D1 to D9 in
   `decisions-2026-10-04.md` are void for option C (the amendment already drops the oracle role). No `fibi`, no `compare`.

## 7. What was not measured

- The 14 ms under `main` for tiny programs (section 2.2): not attributed.
- The `-j 12` efficiency of `cases` (3.4 times, section 2.6).
- Memory use of a warm server: the library's expanded forms and checked units were not sized; the server's resident set is the next number
  the DV3 spike should print.
- Every "estimate" row of section 3 and 4.2 is arithmetic on the measurements above, not a run. The DV3 spike and the DV4 server are what
  turn them into measurements.
- Only one machine, shared with other jobs, 7 runs per number, 10% noise. The `p200` figures between batches (447, 501) show it.
- The Rust interpreter could not run the library-heavy programs (older library), so its compute comparison is on `pold` only.


## Implementation record (sections 8–11)

## 8. The compile server, as built (DV2 with the engine of DV3; 2026-10-07)

`fibc serve [--socket PATH] [-I dir]..` keeps the front end's state in a process and answers requests over a unix-domain socket. `fibc --server VERB ..` is its client. `fibc repl` (section 9) is
the same engine in a terminal. Code: `compiler/serve/` (13 files, 1,700 lines of fibber, none over 237 lines); two edits outside it: the command words in `compiler/driver/args.fib` and
`compiler/fibc.fib`, and the macro runner's cache key (`compiler/macros/runner.fib`, 23 lines). No new dependency: sockets, `fork`, `waitpid`, `poll`, `termios` through externs in
`compiler/serve/sock.fib` and `line.fib` (the compiler is a host tool, ADR 0011 does not apply; the library has no unix-domain sockets, `fib.os.net` is TCP).

### 8.1 What is kept, and what is not

- **The expansion of each module, under a key of its content and of everything before it.** A module's key is the hash of its text and name combined with the key of the module before it in the
  load order (the order is the dependency order, main last, the implicit library first). The engine stores, under that key, the module's expanded forms and a *snapshot of the expander's context*
  after it (`serve.snap`: every part of an `ExpandCtx` is a `Cell` of persistent values, so a copy of the cells continues exactly where the first stopped; the constructor is positional, a
  new field makes the file fail to compile). A request loads the program (the modules are read and parsed again: 17 to 40 ms for 60 to 110 modules), computes the keys, restores the snapshot
  after the longest prefix it already has and expands the rest. A change to a module changes its key and every later key: **the modules after the changed one are expanded again even when
  they do not use it** (the context after a module is a function of everything before it). Expanding from scratch and from a snapshot give the same lIR: the tests compare `emit` through the
  server, after an edit, with `fibc emit` byte for byte.
- **The macros.** `macros.runner` builds each user macro's module once and keeps it in its session. The server keeps one session across requests; the key of a built macro is now
  `ns/name@KEY`, the key of the module that defines it, so an edit rebuilds the macros of the modules from the edit on and no others (before: `ns/name`, which is only right inside one run).
- **The verdict of a check, by key** (the key of the last module, the file name, whether `main` is required): a check of text already seen costs the load (9 to 40 ms).
- **The prelude** (`lib/prelude.fib`, expanded once, re-read when its content changes: the key is its hash) and the context after it.
- **Not kept: the type checker's and the ownership pass's state.** Lowering, inference and ownership run on the whole expanded program at every request. This is the floor of the loop
  (section 8.5): for a one-function edit of `fib.json.codec` the check is about 70% of the time. A second version (`serve.units`, since removed) checked only the functions an edit can have
  affected and left the module's other functions unchecked; the measurement was a gain of about 10 ms, because the units that dominate are the library's reached units, and leaving
  those out is not possible: a function has no scheme until its unit is inferred (`internal: NAME has no scheme`, tried). What would cut it is a persistent checker: a snapshot of
  `Globals` and the checker's environment after the unchanged prefix, as the expander's context is kept. That is DV6 proper and is not built.
- **Not kept: generated code.** `run` and `test` make the lIR in the worker and JIT it in a new process; the JIT of the program and the part of the library it reaches is the other half of
  the time of a spec run (section 8.5).

### 8.2 The protocol

One JSON object on one line in, one on one line out, **one request for each connection** (the client connects, writes the line, reads one line, closes). UTF-8; a line over 8 MiB, a line that is
not UTF-8 and a connection that says nothing for 3 s are answered with a `bad-request` and closed. The server never traps on a request: it is the property that the protocol fuzz of the tests
checks (10,000 malformed requests, same worker afterwards). The socket is `$FIBC_SOCKET`, else `$XDG_RUNTIME_DIR/fibc-serve.sock`, else `/tmp/fibc-serve-$USER.sock`, mode 0600; `serve` refuses
a socket that something accepts at and replaces a stale file. Paths in requests are absolute (the client makes them so): the server has its own working directory.

Request: `{"op": OP, "id": ID?, ...}`; `id` (a string or a whole number) is echoed in the answer. Every answer has `id` and `ok`; an error is
`{"id":..,"ok":false,"error":{"code":"bad-request"|"unknown-op"|"cannot-read"|"unsupported"|"fork-failed","message":..}}`.

| op | request fields | answer fields |
|---|---|---|
| `ping` | | `pong` |
| `check` | `file`; `text` (the buffer instead of the file); `roots` (module roots, after the server's `-I`); `main` ("auto" default: required when the file defines it, "yes", "no") | `exit` (0, or 3 when rejected), `diagnostics`, `stats` |
| `emit` | as `check` | `lir` (the lIR text, as `fibc emit`), or `diagnostics` |
| `run` | `file`, `args` (strings), `opt` (0..3), `timeout` (seconds, default 60), `roots` | `exit` (the status of the process), `stdout`, `stderr`, `trapped` (status 134), `timed_out` (status 142), `stats`; or `diagnostics` and `exit` 3 when the program does not check |
| `test` | as `run`, and `only` (a substring of a scenario id, fib.test's `--only`), `seed` | as `run`: a scenario selection that matches nothing is exit 2 |
| `eval` | `session` (default "default"), `text` (what `fibc repl` takes), `timeout`, `roots` | `defined` (names), or `value` (the printed value, `stdout`, `stderr`, `exit`, `trapped`), or `type` (for `:type EXPR`); a refusal: `shown` (the lines the REPL prints) and `diagnostics` |
| `stats` | | `requests`, `entries`, `verdicts`, `pid` of the worker |
| `reset` | | forget the cache |
| `shutdown` | | the server answers, exits 0 and removes its socket |

`stats` of a check: `{"modules":67,"reused":65,"expanded":2,"verdict":false,"load_ms":18,"expand_ms":8,"lower_ms":80,"check_ms":290,"total_ms":400}`: the modules of the program, how many were
restored and how many expanded, whether the answer came from the verdict kept, and the milliseconds of loading, expanding, lowering and type and ownership checking.

Example (a program that does not check, then the same text edited):

```
> {"op":"check","file":"/p/bad.fib","id":3}
< {"id":3,"ok":false,"exit":3,"diagnostics":[{"severity":"error","code":"type/Unify","file":"/p/bad.fib","line":2,"col":26,"endLine":2,"endCol":27,"start":35,"end":36,"message":"cannot unify i64 with str"}],"stats":{...}}
```

### 8.3 Diagnostics

One object for each error: `severity` (always "error"), `code`, `file`, `line` and `col` (1-based, of the start), `endLine` and `endCol` (of the end, when the file can be read), `start` and `end`
(byte offsets), `message` (the text the compiler prints). The **code** is `STAGE/KIND`, KIND the variant name the passes' dumps already print (`fibc emit-dump`, the types and own tools):
`type/Unify`, `type/Resolve`, `type/NoField`, `type/NoInstance`, `own/BorrowEscapes`, `own/AmpTwice`, `read/UnterminatedString`, `expand/MacroFailed`, `load/Missing`, `load/Cycle`,
`internal/Checker`. (`fibc explain` prints the ownership checker's decisions and has no codes: these are the codes the checker itself uses.) The checker stops at its first error in a unit,
so a program has as many diagnostics as units that failed.

### 8.4 Runs, tests, concurrency, crashes

- `run`, `test` and `eval` make the program in the worker (so the cache is warm) and **fork a reporter, which forks the program**: the program's output and error go to files, its input is
  /dev/null, an alarm ends it after `timeout` seconds, and a trap (status 134), a signal or a timeout ends only that process. The reporter writes the answer on the client's connection and
  exits; the worker does not wait for it, so runs overlap. Neither child keeps the listening socket. No core files (`RLIMIT_CORE` 0: a core of this process takes a second to write).
  The exit goes through the C library's `exit`, as `fibc run`'s does: a program's buffered output is written.
- **Supervision.** The process you start holds the socket and runs the worker in a child; a worker that dies (a trap in a pass: a bug, or the debug request `crash` with
  `FIBC_SERVE_DEBUG=1`) is started again with an empty cache and the supervisor says so on its standard error. The request that was in flight gets no answer (the connection closes): the client
  takes that as "no server" and runs the ordinary command in its own process. At most 50 restarts.
- **Clients.** The worker serves one connection at a time; a check holds the others for its duration (they wait in the listen queue, 64 deep). 24 concurrent clients, mixed checks and runs,
  all get their own answer (tests). A client that connects and says nothing holds the worker for the read timeout (3 s) and no longer.
- **The client** `fibc --server check|emit|run|test|eval|ping|stats|reset|shutdown ARGS [--socket PATH] [--json] [-q] [--timeout N]` prints what the ordinary command prints (`check`:
  `rejected:` and one `file:line:col: message [code]` line for each diagnostic, on standard error, exit 3; `run`: the program's output, its exit status) and with `--json` the answer line.
  When no server answers it prints `fibc: no server (why); running here` on standard error (not with `-q`) and runs the ordinary path in this process: `run` and `emit` are `fibc run` and
  `fibc emit`, `test` is `fibc test`, `check` is the same engine with nothing cached. The tests compare the warm server's diagnostics with those of the cold process for the same edit.
  `check` has no ordinary command in the tree to fall back on (another package is adding `fibc check`; when it lands, `--server check` should call it for the fallback and this one stays the
  server's).

### 8.5 Measurements (what the loop costs now)

Command: `python3 scripts/bench/dev-loop-serve.py F9 F0` (edits are made to a scratch copy of `lib/` and passed as the first module root; `F0` is stage 2 of main 68e4cf6, `F9` this tree). The 28-core
host was **shared and loaded** (load average 11 to 13 from other agents): every number is high by an unknown amount, the min-max column shows the spread, and the numbers quoted in the text as "a
quiet moment" (first two runs of the same script, load 5) are lower. Seconds, median of 3 to 6 runs.

| row | before (no server) | with the server | notes |
|---|---|---|---|
| cold `run hello` | 0.21 (0.18-0.22) | 0.22 (0.20-0.27) through `serve` | the JIT and the process dominate; the front end saved is about 60 ms and a fork costs it back |
| `emit hello` (the front end alone; the nearest to a check there is) | 0.15 | `check`: 0.03 when the text was seen; first time 0.14 | |
| `test` one scenario of `specs/json-api-spec.fib` | 3.7 (3.4-3.7) | 3.8 (front end 2.0 of it, after an edit) | the code generation of the spec, not the front end, is what remains |
| `cases` one case | 0.20 | not changed | each case is a `fibc run` child |
| after an edit of one function of `fib.json.codec`: **check** | none; the client's fallback (a cold engine) 0.69 | **0.47-0.68** (expanded 2 of 67 modules; lower 135 ms, type and ownership check 431 ms); first time 0.73 | quiet moment (load 5): 0.35-0.42; the target of 0.30 s **was not reached** |
| after an edit of one function of `fib.tls.chain`: check | | **0.40-0.53** (expanded 2 of 60; lower 85, check 250); first time 0.75 | |
| the same text checked again | | **0.03-0.04** (the verdict) | |
| after an edit of `fib.json.access`: `run specs/json-api-spec.fib` | 3.4 | | |
| the same edit: **check** of the spec | 1.8 (`emit`-like) | **1.9** (expanded 27 of 107 modules: everything after the edit in the load order; check 719 ms) | the engine does not help here: the modules after the edit are all expanded again |
| `eval` of an expression in a session (server) | | 0.27 | check of the session's program + JIT + fork |
| `eval` of a definition or redefinition (server) | | **0.08** | the check only |
| REPL input that runs (`:time`) | | front end 60-120 ms, run 40-60 ms | |
| worker resident set after the run above (375 cached modules) | | 255 MB | |

What this says. (1) The server removes the cost of the library and of unchanged modules **where the edit is in the last modules of the load order**: a check of a library module drops from 2.3 s (the
spec that uses it, before) to 0.35-0.6 s, and to 0.03 s for text seen. (2) It does not remove the type and ownership checking of the whole program, which is 55 to 70% of what is left (lower 20%,
load and expansion 10%); that is the floor until the checker's state is kept (section 11). (3) An edit early in the load order (a module the spec reaches through ten others) expands everything
after it again: 27 modules, 1.9 s. (4) `run` and `test` are limited by code generation, which nothing here caches: no gain over the 3.4-3.7 s of today for a json spec, a gain of the front end only
(about 0.1 s for hello). **So the loop for a developer editing a library module is: `check` in 0.4-0.6 s instead of a 3.5 s run; for a spec run it is unchanged.**

### 8.6 The editor

Not done. The language server (`compiler/lsp`) checks the whole library, with no macro runner, for every analysis (it needs the scheme of every library name for completion and hover, which
the engine's on-demand check does not make), and keeps its own memo of whole runs. Using the server for the diagnostics only is cheap (the `check` answer has the byte offsets `LsDiag`
needs and `text` takes the buffer): send `check` with the buffer when a socket answers, and keep the language server's own analysis for completion and hover. That is about 40 lines in
`lsp.analysis` and a test against the recorded transcripts of `compiler/tests/lsp`; it was left because the lsp files are another package's golden-checked surface and the gain, for a file the
engine has seen, is a check of the buffer in 100 to 400 ms against the language server's whole-library analysis.

## 9. The REPL, as built (DV7; 2026-10-07)

`fibc repl [--echo] [-I dir]..`: read, evaluate, print. It is a development tool, not a specification oracle (ADR 0005). It uses the server's engine in its own process (`serve.incr`: the
library's expansion is done once per session; it does not talk to a running `fibc serve`; the server's `eval` request (section 8.2) is the same session code behind a socket, with the sessions in
the worker).

**An input** is definitions, `require` and `use`, or one expression; several forms in one input run in order (definitions first, then the expressions). Definitions: `def defun defn defrecord
defstruct defenum defprotocol defmacro extern impl derive`. `(require [a.b :as x])` and `(use a.b)` become clauses of the session's `ns`; `(ns ..)` in a loaded file contributes its clauses.
**An expression** is the body of a `main` that prints its value with `prn` (the Debug text: strings quoted, as Clojure's `pr`); an expression of type unit is run for its effect (a program that
does not print has no `Debug` for unit, the check says so, and the program is made again without the print: one more check, 60 to 100 ms).

**State is source.** A session is the text of its definitions. Every input is a whole program: the session's text, then the input, checked by the engine (the library and the unchanged modules
cost the load, not the check) and, for an expression, JIT-compiled and run in a child process with the terminal as its streams. Consequences, which are the answers to the questions the brief asked:

- *Redefinition.* A new definition of a name replaces the earlier one (it moves to the end of the session's text; order does not matter to the checker). Types and protocols are replaced the same
  way (`defrecord Pt` again). It is **accepted only when the program of all the definitions still checks**: otherwise nothing changes and the output says which definition broke
  (`<session>:6:26: Pt has no field x [type/NoField] (in the definition of px)`, then `; refused; the session is unchanged`). So dependents are always re-checked, against the new definition.
- *Previously compiled dependents.* There are none: nothing is compiled between inputs, so no dependent keeps the old code. Each run is a fresh program built from the current text.
  The price is that **values do not persist**: a `def` is evaluated again by every input that runs (its side effects too), an `atom` or a cell does not keep its value from one input to the next.
  A REPL with resident values needs a JIT session that keeps modules and replaces them by resource tracker, and the per-definition checker of DV6; this is not that (section 11).
- *Cost.* An input that runs costs the check of the program (60 to 120 ms with a few definitions, more as the session grows) and the JIT of the program and the part of the library it
  reaches (about 60 ms): 170 to 250 ms measured in a quiet moment. A definition alone costs the check only (77 ms in the server's `eval`).
- *Traps.* The evaluated program runs in its own process (`fork`): a trap, an exit, a signal ends that process, the REPL says `; the program trapped (status 134); the session is unchanged` and
  goes on. SIGINT is ignored by the REPL and default in the child, so ctrl-c stops a running program and not the session. Out of memory is the child's too. **Not covered:** a `def` whose
  initialiser traps makes every later run trap (it is evaluated by each); `:reset` clears it.
- *Ownership across inputs.* There is nothing to decide, because a value of one input is not visible in the next: a name defined by `def` is a global of the program, as in any program.

**Commands** (`:help`): `:type EXPR` (the checker's type of the expression: a function made of it is checked and its type read from the tables: `(map inc [1 2 3]) : (LSeq i64)`), `:doc NAME`
(a definition of the session: its text; any other name: its type), `(doc NAME)` the same, `:time EXPR` (the front end's milliseconds with how many modules were expanded and reused, and the
run's), `:load FILE` (the definitions of the file added, its expressions run; the file is remembered), `:reload`, `:defs`, `:reset`, `:quit` (also `(quit)`, `(exit)`, ctrl-d).

**Reading input.** On a terminal (`isatty`) the descriptor is put in raw mode with `termios` (`tcgetattr`, `cfmakeraw`, `tcsetattr`: what the platform gives, no readline) and a line is edited:
left, right, home, end, ctrl-a/e/b/f, backspace, delete, ctrl-k/u/w, ctrl-l, up and down (ctrl-p, ctrl-n) through the history, ctrl-c drops the line, ctrl-d on an empty line ends. The history is
`~/.fibc_history`. While a program runs the terminal is in its normal mode. An input whose brackets or string are open continues on a line with the prompt `...  `. Off a terminal a line is read
as it is; with `--echo` the prompt and each line are printed, so the output of a session is its own transcript.

Transcript (`compiler/tests/repl/redefine.transcript`, exact):

```
fib> (defun sq (x: i64) -> i64 (* x x))
; defined sq
fib> (defun cube (x: i64) -> i64 (* x (sq x)))
; defined cube
fib> (cube 3)
27
fib> (defun sq (x: i64) -> i64 (+ x 1))
; redefined sq
fib> (cube 3)
12
fib> (defun sq (x: i64) -> str "no")
<session>:2:34: cannot unify str with i64 [type/Unify] (in the definition of cube)
; refused; the session is unchanged
fib> (cube 3)
12
```

and of errors and a trap (`errors.transcript`, `trap.transcript`):

```
fib> (defun leak (x: (Box i64) :borrow) -> (Box i64) x)
<input>:1:1: parameter x of leak is declared :borrow but escapes [own/BorrowEscapes]
; refused; the session is unchanged
fib> (first-of [])
trap: nth: index out of range
; the program trapped (status 134); the session is unchanged
fib> (first-of [4])
4
```

## 10. Tests and planted faults

`compiler/tests/serve/run.sh FIBC` (13 s, offline; python3 and a real socket): the protocol (ids, errors, one request for each connection), diagnostics (file, line, column, end, code),
the cache against changed files (a touched file with the same content is a verdict hit; an edited dependency makes the dependent fail and is not served stale; only the modules from the edit on
are expanded; reverting works), `emit` equal to `fibc emit` (before and after an edit), the warm server's diagnostics equal to a cold process's, runs (arguments, status, a trap, a timeout, a
program that does not check), eval sessions (definition, redefinition, a trap), the client's output and its fallback, 24 concurrent clients, a silent client, a worker crashed with the debug
request and restarted, **10,000 malformed requests** (random bytes, truncated and deeply nested JSON, wrong types, invalid UTF-8, a 3 KB path) with the same worker afterwards, a second
server at a live socket refused, shutdown.
`compiler/tests/repl/run.sh FIBC` (about 15 s): five scripted sessions (`*.in`, the transcripts `*.transcript` are the exact output of `fibc repl --echo`: session, errors, redefinition,
trap, load and reload) and `terminal.py`, the REPL on a pseudo-terminal (typing, editing, history, continuation, ctrl-c, ctrl-d, a trap). Both are in the gate's `tools` stage (`scripts/tools.sh`,
full gate).
`scripts/mutant-serve.sh MODE` plants one fault in a copy of `compiler/`, builds it and runs the test that pins the rule; the test must fail:

| mode | fault | killed by |
|---|---|---|
| `stale-cache` | a module's key ignores its text (an edited file is not seen) | serve: 5 checks fail (the edit is not seen, the stale verdict is served, the dependent still checks) |
| `no-dependents` | a module's key ignores the modules before it (a dependent is not expanded or checked again) | serve: the dependent of an edited module still checks, only-from-the-edit-on expansion, warm vs cold diagnostics |
| `no-supervisor` | the worker is not run under the supervisor | serve: the crash request ends the server, no new worker (new pid is None) |
| `repl-trap-kills` | the REPL runs an expression in its own process, not in a child | repl: the first expression ends the session; 5 transcripts and the terminal test differ |
| `redefine-keeps-old` | a redefinition does not replace the earlier definition | repl: the load and redefine transcripts differ |
| `redefine-unchecked` | a definition is accepted without checking the program of them all | repl: the errors and redefine transcripts differ (refusals are accepted) |

Run on 2026-10-07, each built from a copy with the seed v0.1.11: all six KILLED (the lines of `$MUT_OUT/test.log` quote the failing checks).

## 11. What remains

1. **The checker's state across requests** (DV6 proper). Lowering and the type and ownership checks of the whole program are 70% of the loop for a large module (section 8.5). Keep a snapshot of
   `Globals` and the checker's environment after the unchanged prefix, as `serve.snap` does for the expander, and check only the modules after the change; then the library's units are checked once.
   Needs the audit that every part of `Globals` and `Checker` is a `Cell` of persistent values, as `ExpandCtx` is.
2. **Generated code.** `run` and `test` JIT the program each time. Per-function modules, or a module for each source module with the ORC resource trackers
   (`LLVMOrcResourceTrackerRemove`), would let a changed function be replaced. Section 3 estimated at most 55 ms of 330 for lazy compilation; the spec runs here are 1.5 to 2 s of code generation.
3. **A REPL with resident values** (needs 2).
4. **`fibc check`** is another package's: the client's fallback for `check` should call it.
5. **The editor** (section 8.6), `cases --watch` and the fork-per-case harness (section 4.2): not built.
6. The type checker is whole-program, so a check of a library module needs the program that requires it: a library module (`fib.*`) is checked as the module a one-line program requires, which
   sees the implicit modules; its text is the file on disk (`text` is for programs).
7. Linux and macOS have `sockaddr_un` handled; only Linux was run.
