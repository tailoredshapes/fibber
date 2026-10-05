# Porting the rest of `fibref`: plan, contracts and references

Status: design (FPORT0, 2026-10-05). The only code is contract stubs (section 8) and recorded references (section 9). Every claim about the Rust
names a file at tag `seed-1` (read with `git show seed-1:crates/fibref/src/...`); every number I ran this session is marked "ran" and is
reproducible from the commands in section 9. Nothing here is **Decided** by the owner except what section 1 quotes; section 11 lists the
decisions with recommendations.

## 0. Short answer

- The front end of `fibref` is already in `compiler/`. What is not: the evaluator (`eval/`, 6,515 lines), the audited heap (`heap/`, 2,122), the
  language server (`editor/`, 1,974 lines of sources and 322 of unit tests), and the JSON
  reader and writer that the server needs (`json.rs`, 362; fibber has none). `cases/` is not a port task (section 1.3).
- Build **two products that share nothing but the front end**, in this order: (1) the **language server**, `fibc lsp` and `fibref lsp`, because
  the VS Code pack starts `fibref lsp` and the only server that exists cannot read today's library (dev-loop.md 2.5), so the pack is broken for
  every current program; about 1,850 lines of fibber in six packages. (2) The **interpreter with the audit**, `fibref run|trace|audit|explain`,
  about 5,900 lines in five packages.
- Port the heap and the audit **faithfully** (it is the unique value: 249 firing tests exist in the Rust and each becomes a planted-fault test);
  port the evaluator **as a prepared-tree walker over the plan**; **drop** the independence rules of fibber-interpreter.md (the owner dropped
  rule 6), so builtins that have a host equivalent forward to the host instead of being rewritten (this removes about 600 lines of the Rust:
  float text, `strtod`, float bits).
- The compiled runtime already audits **leaks and double frees** by trace (`FIB_TRACE`, spec/compiler.md section 4). The interpreter still adds
  eleven things (section 4.3); the first is that use-after-free, over-release without a free, and illegal writes are **invisible** to the compiled
  run, and the compiled run cannot tell a leaked cycle from another leak.

## 1. Scope decision per part

### 1.1 What the Rust is (ran: `wc -l` over the tree at seed-1; total 74,873 lines including tests)

| Part | Sources | Tests | Already in `compiler/`? |
|---|---|---|---|
| `syntax expand types own modules` + dumps | ~38k | many | yes: reader, expander, checker, ownership checker, loader, `read/expand/types/own` dumps |
| `eval/` | 6,515 | 1,313 + `tests/eval_*` 295 | no |
| `heap/` | 2,122 | 1,150 + `tests/heap_*` 1,900 (98 + 123 + 12 + 16 = 249 `#[test]`) | no |
| `cases/` | 2,986 incl. tests | | `driver.header`, `driver.verdict`, `driver.harness`, `driver.child`: yes |
| `editor/` | 1,974 | 322 (31 + 5 `#[test]`) | no |
| `json.rs` | 362 | | no |
| `main.rs`, `cmdline.rs`, `dump.rs`, `roots.rs` | 1,100 | | `fibc.fib`, `driver.args`: yes |

### 1.2 Per part: faithful, drop, or do differently

| Part | Decision | Why |
|---|---|---|
| `heap/` | **Port faithfully**, plus the rules new since seed-1 (steal, move-in, peeks, ROOMY, `array-push!` `array-pop!` `array-take!`, `cell-update!`) | the audit is the one thing the compiled path cannot give (4.3); 249 existing tests are the spec of it |
| `eval/` forms (`expr pattern call closure alloc interp plan world ast fx value object option`) | **Port as a prepared tree**: `prepare` resolves slots, plan operations and callees once per body | a walker over `Expr` with map frames is 3 to 5 times slower than a prepared tree (fibber-interpreter.md 5.4) |
| `eval/` builtins | **Port the semantics, forward the host where the host is the spec**: `str-*`, array primitives, the nine conversions, float text and parse, float bits go to the host's own builtins after the interpreter checks overflow and bounds; the interpreter keeps its own raw-memory arena (so `raw` can be audited) | independence is no longer a goal (decisions-2026-10-04.md, amendment): 79 builtins, but `floattext.rs` 170, `strtod.rs` 298, `float_bits.rs` 150 vanish |
| `eval/{sched,threads,task}.rs` | **Do differently**: lazy run-to-completion (fibber-interpreter.md 3.3 plan A) | fibber cannot share one mutable `World` between host threads; the corpus has 33 `spawn` and 18 `async` cases |
| `eval/macros.rs`, `forms.rs`, `vecs.rs`, `vec_view.rs` (686) | **Drop**: reuse `macros.runner` | it runs macros through the JIT; the interpreter then links LLVM, which is acceptable now (below) |
| `eval/unsafe_ops.rs` `extern` | **Whitelist** `write sqrt strtod strtof strfromd`; anything else `IeUnsupported`, listed pending | as the Rust did |
| `eval/pipeline.rs` | **Port** as `fibref.cmd` (322 lines becomes about 150) | |
| `cases/` | **No port**; add the interpreter as a second outcome source to the existing harness (1.3) | |
| `editor/` | **Port**, as `lsp.*`, on the same front-end tables, plus go-to-definition and document symbols (section 6) | the pack needs it; dev-loop DV8 |
| `json.rs` | **Port** as `lsp.json` | fibber has no JSON module (`lib/fib/lacinia/json.fib` is a writer of GraphQL values only) |
| `dump.rs`, `expand_dump`, `types_dump`, `own_dump` | already ported (`fibc read|expand|types|own`) | |

**Does the interpreter need LLVM?** Today macros run through the JIT, so yes unless package F6 (macros on the interpreter, fibber-interpreter.md P9) is
built; with the independence rule gone I recommend **v1 links LLVM** like `fibc`, and F6 is optional. The "fast start, no LLVM" selling point is
therefore weaker than it sounds: dev-loop.md 2.5 measured the Rust interpreter's cold start at 66 ms against 157 ms for `F run -O 0` on `hello`; the
front-end floor is shared and the interpreter only saves the code generation and the JIT.

### 1.3 `cases/`: what `fibc cases` lacks (ran: read `compiler/driver/{header,verdict,harness}.fib` against `crates/fibref/src/cases/`)

Nothing is missing from the harness: the header parser, the verdict table, the `open` labels, `list-cases` and the selection are ported line for
line. The difference is the **outcome**, not the judge:

| | Rust `fibref cases` | stage 2 `fibc cases` |
|---|---|---|
| runs | the interpreter | the compiled program |
| result | the interpreter's | the compiled exit code |
| audit | `clean`, `leak-cycles=n`, `leaks=n`, `errors=n` from the heap | read from the free trace: every `A n` freed once by an `F n`; `leak-cycles=n/a` (`verdict.fib` header: "the trace cannot tell a leaked cycle from another leak") |
| `audit: leak-cycle` | holds when `leak_cycles > 0`, no other leak, no error | holds when something leaked and nothing erred (`audit-mismatch`) |
| `allocs: <= N` | the interpreter's `A` count | the compiled `A` count |

What to do: **`fibref cases DIR`** is the same harness with `OutCompiled` produced by the interpreter, and `CsAudit` gains `cycles: i64` and
`classified: bool` so `verdict.audit-mismatch` can require a real leak cycle when the interpreter ran. About 150 lines in `fibref.cmd`; no change to
the judge's table. In the corpus only **4 case files** say `audit: leak-cycle` (ran: `grep -rl "^;; audit: *leak-cycle" cases`: `ownership/15`, `ownership/80`,
`stdlib/017`, `stdlib/252`), so the weakness of the compiled judge costs little today; it matters for new work (cycles through atoms).

## 2. Command set

Name decision: the tool is **`fibref`** (the target name of fibber-interpreter.md 4; `fibi` was its working name). Source `compiler/fibref.fib`;
modules `fibref.*`. `fibref` and `fibc` are separate programs sharing modules (the Rust did the same); `lsp` is a subcommand of **both**
(`lsp.cli` is one function, so the release tarball that ships only `fibc` still has a server).

| Command | Does | Replaces |
|---|---|---|
| `fibref run [-I DIR].. FILE [-- ARG..]` | front end, evaluate `main`, print `result: N` and the audit line exactly as seed-1 did (`audit:  clean=true leak-cycles=0 leaks=0 errors=0`, golden `run-ownership.txt`); exit 0, or 3 rejected, 4 unsupported, 5 internal, 134 trap (`trapped:` then the message) | `fibref run` |
| `fibref audit [-I DIR].. FILE` | run and print the full report: every leak with its class and count, dangling refs, open scopes, live tally; exit 1 on any audit error | new (the Rust printed only the summary) |
| `fibref trace [--events] [-I DIR].. FILE` | the free trace of spec/compiler.md section 4 on stderr, the result on stdout; `--events` adds retain, release, read, write, weak, share with counts | `fibc itrace` |
| `fibref explain [--steps] [-I DIR].. FILE` | the plan (the text of `own.explain`, byte for byte, same as `fibc explain`); `--steps` runs the program and prints each plan operation as it is performed with the object ordinal and its count before and after; `--why N` the history of object N | `fibref explain`, new |
| `fibref cases DIR [--only PREFIX..] [-j N]` | the harness with the interpreter's outcome (1.3) | `fibref cases` |
| `fibref lsp [-I DIR]..` and `fibc lsp` | the language server (section 6) | `fibref lsp` |
| `fibref check` (diagnostics as JSON), `fibref complete FILE LINE COL` | the JSON of `lsp.features`, for scripts and the transcripts' replay | `fibref diagnostics`, `fibref complete` |

`read expand types own` stay in `fibc`'s tools (`read.fib expand.fib types.fib own.fib`). Options as `driver.args`: `-I`, `FIB_LIB`, `--no-runner`,
`--max-steps`, `--max-depth`. A **step budget** (default 2e9 calls and back edges) turns an endless spin into `unsupported`, so a hang is never a
pass.

## 3. The evaluator

### 3.1 Value, heap, plan (kept from fibber-interpreter.md section 2.2, with changes)

- **`IVal`** (`compiler/fibref/value.fib`): scalars unboxed in the model, `IObj id` into the heap, `INone`/`ISome` for an `Option`. An `Option` of a
  scalar **holds a value** (spec/types.md section 8.1; the seed-1 Rust boxed it, which is why a few cases are `;; stage: 2`).
- **Plan, not just types.** The evaluator performs the retains, releases and stack-ends the plan lists (`own.program`: per body `BodyOwn`, per call
  `CallOwn`), because the audit is an audit of the plan. A walker that counted by itself would audit nothing.
- **Dynamic dispatch** by type id: no monomorphisation, which is also the property that makes `explain --steps` readable.
- **The `Checked` input** is `own.driver`'s `Checked (typed owned)`; nothing under `fibref.*` requires `emit.*`, `native.*` (except through
  `macros.runner`), `llvm.*` or `lir.*` (kept as a cheap structural rule, `compiler/tests/fibref/skeleton.sh` lists the modules).
- **New since seed-1** each needing a heap rule and a firing test: `steal` (`PsSteal`), `PsMoveIn`, `peeks`, ROOMY and in-place arrays,
  `cell-update!`, all-owned bodies (docs/design/in-place-update.md).

### 3.2 Speed

Planned 2 to 8 million node evaluations a second; 100 to 300 times slower than native on compute (dev-loop.md 2.5, fibber-interpreter.md 5.2: both
unmeasured for the real tool; the spike measured 34 to 170 million for a toy). Hence the product is **not** for the stdlib's `ref-` cases or
tensors; it is for the ownership corpus and for programs where the answer to "why was this object not freed" matters. The package F2 acceptance
includes the first measurement: mean node rate over `cases/ownership`.

### 3.3 `prepare`

One pass per body, run on first call: locals become frame-array slots (no map frames), each node holds its plan operations inline, a call node
holds `CallOwn` and a resolved callee index, patterns become test sequences. About 600 lines. `ops-text` prints the prepared operations in
`own.explain`'s format so a test can assert prepared operations equal the plan for every case (a dropped release is then a failed test, not only a
missed audit).

### 3.4 Builtins and the host

The 79 rows of `types/builtins.fib` plus the methods of the built-in instances. Overflow at each width is checked explicitly and traps with the
message of spec/types.md section 8.12 (fibber has no wrapping arithmetic to lean on; this is the largest new care). Forwarded to the host after the
check: strings, array primitives, conversions, float text and parse, hashing (so `Map` iteration order is the compiled order). Kept in the
interpreter: object allocation and counts, `cell atom weak` (audited), the raw arena (`ptr+ load-* store-* alloc free raw raw-retained release-raw`
need to carry counts), `args`, and `spawn join`. `builtin-known?` is compared with the table's names by a test, so adding a row without an
implementation fails it.

### 3.5 Tasks, atoms and deterministic scheduling

Plan A: `async` creates a record, `await` and `join` run it on the caller's stack, a `spawn` runs when joined or at the end of `main`; a program
that spins on an atom for another thread's write exhausts the step budget and reports `unsupported: needs preemption`. This is deterministic
by construction: the same program has the same trace on every run, which is the one scheduling property the compiled run lacks. Switch to an
explicit-stack machine (plan B) only if more than about 5 of the 51 task cases need preemption (census in F2's first milestone). `T` is traced per
spawn.

## 4. What the compiled runtime audits, and what the interpreter adds

### 4.1 Compiled (ran: read spec/compiler.md section 4 and `driver/verdict.fib`)

With `FIB_TRACE=1` the runtime prints `A n k`, `F n`, `S n k`, `D n`, `T`. `fibc cases` derives: every `A` freed once by an `F` (leak = never freed;
double free = two `F n`; free of an unallocated ordinal), the `A` count for `allocs:`, equal results. Retains, releases, reads and writes are not
traced (the compiler elides pairs, types section 6.12).

### 4.2 Interpreter (ran: the golden `itrace-ownership.txt` is the interpreter's trace for 243 programs, `run-ownership.txt` its audit line)

On `cases/ownership` at seed-1: 190 exit 0, 53 exit 1 (traps and rejections); 188 audits `clean=true`, 16 `clean at the abort`, one
`leak-cycles=4`, one `leak-cycles=2`. The interpreter's `A F S D` lines match the compiled ones by construction of rule 6 when both ran (the
comparison is no longer a gate).

### 4.3 What only the interpreter gives

1. **Use after free**: a read or write of a freed object (`HeUseAfterFree`); compiled code reads freed memory and may print a plausible result.
2. **Over-release that does not reach zero** (a count that goes wrong but nothing is freed early) and a release of a freed object (`HeReleaseOfFreed`).
3. **Illegal writes**: write to an immutable, `NotUnique` (`write_unique` on a shared, immortal or stack place), bad field.
4. **Stack discipline**: `StackUseAfterScope`, `StackRefInHeap`, `StackRefIntoOuterScope`, `WeakToStack`, `SharedStack`.
5. **Static data discipline**: `ImmortalHoldsMortal`, `MutableImmortal`, `SharedCell`.
6. **Leak classes**: `LeakCycle` (count equals live ref fields, kept alive only by a cycle through a cell or atom), `Leak`, `ImmutableCycle` (a bug).
7. **Dangling refs and unended scopes** at exit.
8. **The full event trace**: every retain and release with counts, which the compiled run elides.
9. **Step-level explanation**: which plan operation moved which count (`explain --steps`, `--why N`): the answer to "why is this object still live".
10. **Deterministic tasks**: the same schedule every run.
11. **Fast start without code generation**: 66 ms against 157 ms on `hello` (dev-loop.md 2.5); nothing more is claimed.

Not given by the interpreter, because it shares the front end: type, expansion and ownership-decision bugs (the audit only shows a plan that is
unsafe, not one that is safe and unintended).

An alternative that covers items 1 to 5 at native speed is a compiled `FIB_AUDIT=1` mode (poison on free, per-object live flags, a debug check in
`fib.unique?`); I do **not** plan it here: it is a runtime change (`crates/fibc/rt/*.lir`), outside the brief. It is decision D4.

## 5. What the golden audit outputs are for

See section 9. They are the acceptance data of F1, F2, F5: byte-equal `run` output on the cases whose source is unchanged since seed-1, and
byte-equal `A/F/S/D` traces (modulo the 400-line cap) for those.

## 6. The language server

### 6.1 What the editor needs (ran: read `editors/vscode/extension.js`, `package.json`, `test/lsp.js`)

- Start: `fibber.fibrefPath` (default `fibref`) with args `lsp`, `-I DIR` per `fibber.includePaths`, env `FIB_LIB` from `fibber.libraryPath`.
  Over stdio. The extension uses `vscode-languageclient`, so it speaks plain LSP; **no custom messages**.
- Handshake: `initialize` answered with `textDocumentSync: 1` (full), `completionProvider.triggerCharacters ["(" "/" "." ":" " "]`,
  `hoverProvider: true`, `serverInfo.name`.
- Notifications: `initialized`, `didOpen`, `didChange` (the last full text), `didSave`, `didClose` (clears diagnostics), `exit`.
- `publishDiagnostics` after open, change and save: one error range per front-end failure (severity 1, source `fibref`, UTF-16 columns,
  an empty range widened to one character).
- `textDocument/completion`: items `{label, kind (LSP number), detail (the scheme), documentation}` from locals, parameters, top-level definitions,
  the library's exports, fields after `.`, exports after `alias/`.
- `textDocument/hover`: markdown ```` ```fibber ```` of `name : scheme` and `(kind, module)`.
- Unknown request: error `-32601`; a request after `shutdown`: `-32600`; bad params: `-32602`; a handler failure: `-32603` and the server goes on.
  Exit status 0 after `shutdown`, 1 without it.
- `editors/vscode/test/lsp.js` (`npm test`) runs exactly this against `$FIBREF`; **it skips (prints SKIPPED, exit 0) when no binary exists, which is a
  test that cannot fail**: package L5 changes it to fail when no server is found unless `FIBREF_SKIP=1`.

### 6.2 What changes

| Capability | Rust | Port |
|---|---|---|
| diagnostics | first error only, byte range | same, plus every error the front end returns when it returns a list (the type checker does) |
| completion, hover | present | present, from `TypedProgram` tables |
| go to definition | absent | new: locals and top-level definitions by the reader scope, module members by the `Globals` positions (`Pos` of each definition) |
| document symbols | absent | new, from `top-defs` |
| macros | `NoRunner`: a user macro call is pending | same (`--no-runner` ladder); a server must not JIT per keystroke |
| robustness | `catch_unwind` per handler | fibber has no catch (a trap aborts). Decision D5: either the front end is total on garbage input (the transcripts and a fuzz run say so) or each analysis runs in a child process (`driver.proc`); recommend child process only if a fuzz run finds a trap |

### 6.3 Reference transcripts

`compiler/tests/golden/fibref/lsp-transcripts.jsonl` (ran: 71 messages, 5 scenarios: `vscode-session` is exactly `test/lsp.js`'s, `completion-contexts`,
`diagnostics` (type error, UTF-16 with a non-BMP character, close), `protocol-errors`, `exit-without-shutdown`). The seed-1 server answered with its
own older library, so **item lists and `detail` texts will differ from the port's; the golden fixes the protocol shape, error codes, ranges and the
order of messages, not the library**. The replay compares the shape and the diagnostics ranges for programs that do not touch the library's newer parts.

## 7. `explain --steps`, new

The Rust `explain` printed the static plan. The interpreter can also print the dynamic story, one line per plan operation:

```
@14:14  retain   #3 (Cons)  2 -> 3     arg 1 l: borrow
@15:7   release  #3         3 -> 2     L15 release [l] (exit)
```

`--why N` prints the history of object N (alloc site, every retain and release with positions) and, if it is live at exit, who holds it. Built on
`events-text` and the plan's positions; F5.

## 8. Contract stubs (ran: `compiler/tests/fibref/skeleton.sh`)

Stubs live where the code will, in the style of docs/design/lair-interfaces.md: prefixed names (type and variant names are global when linked), files
under 500 lines, bodies `(trap "todo: MODULE FUNCTION")`, one owner per file, a signature change is reported to the lead first. Rules: no global
mutable state (a heap is a value of cells passed in); positions are `syntax.pos` `Pos` in the interpreter and byte offsets in the server; errors are
`(Result T E)`.

| Module | Package | Content |
|---|---|---|
| `fibref.value`, `fibref.error` | F0 | `IVal`, `IKind`, `IRunError`, `ITraceLine`, exit statuses |
| `fibref.heap`, `fibref.trace` | F1 | `Heap`, `HpErr` (17 audit errors), `HpEvent`, `HpLeakClass`, `HpReport`; the 14 operations; trace lines |
| `fibref.eval`, `fibref.prepare` | F2 | `run-program`, `IRun`, `IBody`, `prepare-program`, `ops-text` |
| `fibref.builtins` | F3 | `call-builtin`, `builtin-known?`, `IBuiltinCx` |
| `fibref.task` | F4 | `ITasks`, spawn, join, tick |
| `fibref.cmd`, `fibref.explain` | F5 | `cmd-run cmd-trace cmd-audit cmd-explain`, `explain-plan explain-steps explain-why-live` |
| `lsp.json` | L0 | `JVal`, `json-parse`, `json-text`, `json-get` |
| `lsp.types`, `lsp.text` | L0 | `LsDiag LsItem LsLocation LsPos LsRange`, offset conversions, `path-of-uri` |
| `lsp.analysis` | L1 | `LsAnalysis`, `analyse` |
| `lsp.scope` | L2 | `LsScope LsCtx LsAsk`, `scope-at`, `classify`, `top-defs` |
| `lsp.catalog` | L3 | the five catalog functions over `TypedProgram` |
| `lsp.features` | L3, L4 | `complete hover diagnostics definition document-symbols` |
| `lsp.server` | L5 | `LsServer LsReply`, `server-handle`, framing, `lsp-main` |

The program `compiler/tests/fibref/skeleton.fib` requires every one of these modules and six of the compiler's own (`driver.front`, `driver.header`,
`driver.verdict`, `own.driver`, `own.explain`, `native.error`), so a name collision with the compiler fails the build. It built and printed `ok`
with `F` of `~/.cache/fibber-scratch/B4-D` (a stage 2 built the day before) and this tree's `lib/`; with a planted `(defun bad () -> i64 "x")` in
`lsp/text.fib` it failed with `compiler/lsp/text.fib:14:22: cannot unify str with i64`.

## 9. Reference outputs

### 9.1 Recorded (ran), `compiler/tests/golden/fibref/`

Built from `git archive seed-1` in `~/.cache/fibber-scratch/FPORT/src` with `CARGO_TARGET_DIR=~/.cache/fibber-scratch/FPORT/target`
(`cargo build --release -p fibref` 15 s; `-p fibc` 16 s more with `LLVM_SYS_211_PREFIX=/usr/lib/llvm-21`); regenerate with `record.sh SRC BIN OUT`,
one program at a time, `timeout 60`, `ulimit -v 12000000`, `nice -n 10`.

| File | Content | Size |
|---|---|---|
| `run-ownership.txt` | `fibref run` on the 243 programs of `cases/ownership` at seed-1: result, audit line, trap or rejection text, exit status | 1,054 lines |
| `explain-ownership.txt` | `fibref explain` of the same 243 | 8,948 lines, 520 KB |
| `itrace-ownership.txt` | `fibc itrace` (the interpreter's free trace) of the same, first 400 lines of each | 14,735 lines, 200 KB |
| `run-modules.txt` | `fibref run` of the 27 module programs (`cases/modules/*/main.fib`) | 108 lines |
| `lsp-transcripts.jsonl` | the server's transcripts, 5 scenarios | 71 lines, 220 KB |
| `record.sh`, `lsp-record.js` | the recorders | |

Block format: `=== NAME`, the output, `--- exit N`. Not recorded: the stdlib corpus (it needs the newer library, which seed-1 cannot read), `fibref cases`
(the harness is already ported), `trace --events` (the Rust has no such command), and anything for the primitives added after seed-1.
Case sources: `git show seed-1:cases/ownership/NAME.fib` (243 files; main has 244 plus 112 newer; the one changed since is `05-closures-share-state`).
A first session recorded these without `nice`, a cap and a timeout and was killed by a memory spike elsewhere; `itrace` of `07-recursive-accumulator`
alone is 639,518 lines, hence the cap.

### 9.2 RR1's goldens

`compiler/tests/golden/audit/` did not exist on any branch or worktree I could read when I finished (only `golden/gen/record.sh` in another
worktree). If RR1's lands, the two are complementary: mine are `run`/`explain`/`itrace`/LSP under `golden/fibref/`; I did not duplicate the name.

### 9.3 Rust unit tests as the port's test inventory (ran: `grep -c '#\[test\]'`)

`heap/tests` 98, `tests/heap_adversarial` 123, `heap_unique_adversarial` 12, `heap_stack_adversarial` 16, `eval/tests` 80, `eval_adversarial` 9,
`eval_method_values` 5, `editor` 31, `editor_cli` 5, `allocs` 4. Each F1 audit error has a Rust test that shows it firing; F1 ports them as
fibber programs that drive the heap directly (`compiler/tests/fibref/heap-*.fib`).

## 10. Package plan

Sizes: Rust lines times the project's 0.7 port ratio (ROADMAP), adjusted. Files under 500 lines, functions under 50. Each package replaces the
`trap` bodies of its own files only and adds private helpers or files under its own directory.

### 10.1 Language server (first deliverable)

| Pkg | Files | Rust basis | Est. lines (+ tests) | Needs | Test that can fail (planted fault) |
|---|---|---|---|---|---|
| **L0** json, text | `lsp/{json,text,types}.fib` | `json.rs` 362, `text.rs` 59, `diagnostics.rs` range | 330 (+120) | stubs | round-trip over 200 generated documents and the transcripts' messages; planted: `\u` surrogate decode wrong, UTF-16 column of the non-BMP transcript case off by one |
| **L1** analysis | `lsp/analysis.fib` | `analysis.rs` 360 | 250 (+80) | L0 | the ladder: a broken buffer still returns a program (a test with a missing paren must complete `x`); planted: skip the second rung |
| **L2** scope, context | `lsp/scope.fib` | `scope.rs` 409, `context.rs` 107 | 360 (+100) | L0 | the Rust's `editor/*/tests` as cases; an unbalanced buffer; planted: drop closing delimiters and the local disappears |
| **L3** catalog, complete, hover | `lsp/{catalog,features}.fib` | `catalog.rs` 241, `complete.rs` 189 | 300 (+80) | L1, L2 | the `vscode-session` and `completion-contexts` transcripts: labels `x y map let` present, hover contains `inc : (fn`; planted: filter by module wrongly |
| **L4** definition, symbols | `lsp/features.fib` (rest) | new | 250 (+80) | L1, L2 | definition of a local, a top-level, a library function (lands in the library file), a field; planted: off-by-one position |
| **L5** server, framing, wiring | `lsp/server.fib`, `fibc.fib` + `fibref.fib` `lsp` command, `editors/vscode` default and `test/lsp.js` | `lsp.rs` 359, `transport.rs` 95, `cli.rs` 94 | 380 (+150 JS replay) | L0, L3 (L4 optional) | `lsp-replay.js` over the transcripts (shape equality, ranges, error codes, exit codes) and `npm test` made non-skippable; planted: wrong `-32601` |

Total about 1,870 lines + 610 tests. dev-loop.md DV8 said 900 lines; it counted the features over held tables but not the reader-scope completion
(`scope.rs` alone is 409 Rust lines) nor JSON.

Parallelism: L0 first (a day); **L1, L2 in parallel**; then L3, L4 in parallel; L5 last, though its framing can be written against `lsp.json` while L1 to L4 run.
First user value: after L5 with L3 (diagnostics, completion, hover); go-to-definition one step later.

### 10.2 Interpreter and audit

| Pkg | Files (`compiler/fibref/`) | Rust basis | Est. lines (+ tests) | Needs | Test that can fail |
|---|---|---|---|---|---|
| **F0** contract | the stubs above, `skeleton.fib/.sh` | | done | | skeleton builds and a planted type error fails it (ran) |
| **F1** heap, audit, trace | `heap.fib` split into `heap/{store,alloc,release,cascade,scope,unique,shared,immortal,audit,graph,scc}.fib`, `trace.fib` | `heap/` 2,122 | 1,500 (+500) | F0 | the 249 Rust tests become `heap-*.fib` programs; planted: a heap that skips the double-free check must fail `release-of-freed`; cascade order vs `itrace` golden for 20 object graphs |
| **F2** evaluator, `prepare` | `prepare.fib`, `eval.fib`, `eval/{expr,pattern,call,closure,alloc,option,plan,defs}.fib` | `eval/` forms 2,785 less macros | 2,300 (+400) | F0, F1 interface | `fibref run` byte-equal to `run-ownership.txt` on the unchanged cases; `ops-text` equals `fibc explain` operations; planted: drop one release in `prepare` and a case must report a leak |
| **F3** builtins | `builtins.fib`, `builtins/{arith,str,array,cell,sys,raw,extern}.fib` | `eval/` builtins 2,377 less 618 | 1,000 (+250) | F0 | table-driven at each width boundary (`i8` + overflow, `i64` min / -1, rem), against the spec section 8.12 messages; `builtin-known?` equals the 79 rows; planted: skip one overflow check |
| **F4** tasks | `task.fib` | `eval/{sched,threads,task}.rs` 793 | 450 (+100) | F1, F2 | the task cases (11, 24, 32, 38, 43, 44, 148, 174 to 176); a spin on an atom reports `needs preemption`, not a hang |
| **F5** commands, cases, explain | `cmd.fib`, `explain.fib`, `fibref.fib`, `driver/` additions (`CsAudit.cycles`) | `main.rs` 420 | 900 (+150) | F0; useful with F1 to F3 | `fibref cases cases/ownership` has no FAIL; `explain` equals `fibc explain` byte for byte on 243 programs; `--steps` totals per object equal the audit's final counts; planted: mutated stage 2 frees one object later and `trace` vs compiled differs |
| **F6** (optional) macros on the interpreter | `macros.fib`, `forms.fib` | 686 | 500 | F2, F3 | the macro cases |

Total F1 to F5: about 6,150 lines + 1,400 tests; with F6 +500. Compared with fibber-interpreter.md (7,350 + 1,050): smaller by the forwarded builtins and the
retired independence checks, larger by `explain --steps` and `audit`.

Parallelism: F1, F3 and the command half of F5 run in parallel from the stubs; F2 starts on the stub interface of F1 and finishes against the real one;
F4 after F2; the `cases` and `--steps` parts of F5 last. First user value of this track: F1 + F2 + F3 + `run` and `audit` of F5 on the ownership corpus.

### 10.3 Order

1. L0, then L1 and L2, F1 and F3 (four agents at once), 2. L3, L4, F2, 3. L5, F4, F5. LSP ships at the end of step 3 if the lead gives L-packages priority;
the two tracks share only the front end and the driver, so they never block each other.

## 11. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| D1 | Name: `fibref` (target) or `fibi` | `fibref`, source `compiler/fibref.fib` |
| D2 | LSP in `fibc` as well as `fibref` | both: one `lsp.server` function; the extension's default command stays configurable |
| D3 | Interpreter links LLVM in v1 (macros via the JIT) | yes; F6 only if a no-LLVM interpreter is wanted |
| D4 | A compiled `FIB_AUDIT=1` mode for use-after-free and stack discipline at native speed | consider after F1; it is a runtime change and gives items 1 to 5 of 4.3 only |
| D5 | LSP handler failure: no catch in fibber | fuzz the front end on garbage buffers first; a child process per analysis only if a trap is found |
| D6 | Make `editors/vscode/test/lsp.js` fail rather than skip when no server exists | yes (6.1) |
| D7 | Keep `;; stage: 2` labels | unchanged; a case labelled only for a seed-1 gap loses its label in a separate reviewed change |
| R1 | The interpreter's speed (planned, not measured) | measure at F2's first milestone; if under 1 million nodes per second reconsider plan B |
| R2 | Host stack depth for recursion | the Rust used a 1 GiB stack; fibber has the process stack: re-exec under `setrlimit` (as fibber-interpreter.md 5.3) |

## 12. What this document does not claim

No stub has a body. No fibber port of any part exists yet. The speeds are plans. The golden outputs are seed-1's: they fix behaviour for the language
as it was then, not for primitives added since. The 0.7 ratio is the project's, not measured for an interpreter.
