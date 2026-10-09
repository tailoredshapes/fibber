# Executable architecture decision records: standalone `make adr`

Current status: `make adr` builds and runs the standalone `build/adr --strict`, in the full gate. `fibc adr` is still a proposed driver command. Original prototype plans below are dated history.

Original record (dated statements and unmarked code fences below are historical sketches):

Status: design with a working prototype, 2026-10-05. **Measured** means a command was run in this session and its output is quoted
(section 9). **Prototype** means it exists in `lib/fib/test/arch*`, `compiler/adr*` and `docs/adr/` and `compiler/tests/adr/run.sh`
passes. Anything else is design and is marked "not built". Built and run with the tree's stage 2 (`F`, built by the v0.1.6 seed from
this tree) and `FIB_LIB` = this tree's `lib`. `scripts/gate.sh --full` has an `adr` stage (section 7.2); `compiler/driver` and
`compiler/fibc.fib` are **not** edited (the tool is standalone, section 3.1).

The owner's request: architecture decision records that **run**, because "bdd doesn't look like an adr". The ADR is the presentation (context,
decision, consequences, in prose a person reads); the scenarios are the engine (checks that can fail). A decision whose checks fail is
**violated**; one with no checks is **unverified**. Nothing here replaces the project's rule that claims in docs carry no weight: it
extends it to the decision records, which until now were prose with `**Decided**` tags.

Contents: 0 short answer; 1 what exists and what an ADR adds; 2 the format; 3 the tool; 4 the fitness API; 5 measures; 6 the pilot ADRs;
7 how this changes the workflow; 8 limits and risks; 9 the prototype and its output; 10 not done and decisions for the owner.

## 0. The short answer

1. **An ADR is a Markdown file `docs/adr/NNNN-title.md`** in Michael Nygard's shape as extended in *Software Architecture: The Hard Parts*
   (Ford, Richards, Sadalage, Dehghani): Title, Status, Context, Decision, Consequences, and a **Governance** section where the automated
   checks live. **The wording of that template is from memory (I cannot read the book here): check it against the book before the format
   is frozen** (section 2.1).
2. **Status is derived from evidence for an accepted ADR**: `accepted` only while every executable block passes, `violated` when one
   fails, `unverified` when it has none; `superseded by N` stops running its blocks; `proposed` and `deprecated` are manual. The file
   says `Status: accepted` (the claim); the tool says what the evidence supports. A third word, `invalid`, is a file that breaks the format.
3. **Executable blocks are fenced code with an info string**: ```` ```fibber spec ```` (scenarios in `fib.test`'s forms, unchanged),
   ```` ```fibber fitness ```` (rules over the repository), ```` ```fibber measure ```` (a recorded number against a threshold).
   A shell `check` block is **not** supported (section 2.4).
4. **`compiler/adr.fib` is the tool** (`adr [PATH..] [--only N] [--format plain|json] [--status] [--stamp] [--root DIR] [--rerun] [--strict]`):
   it reads the Markdown, writes one fibber program from the blocks (a module each, the author's text kept line for line), runs it
   with `fibc test` (FT1's command, on a generated `adr-spec.fib`), and aggregates per ADR. A module that does not compile is attributed to its block (with the Markdown
   line) and the others still run. Exit 0 / 1 / 2.
5. **`fib.test.arch` is the fitness API** (library, `lib/fib/test/arch*.fib`): the tree with line counts, the top-level forms of every
   `.fib` file (read once), function lengths, definitions, the module graph, grep that skips comments, plant helpers, the combinators
   `rule` `forbid` `limit` `measure`, and a **planted violation per rule**: a rule must ship a change to the repository, applied in
   memory, under which it reports something; a rule without one, or whose plant it does not notice, fails. That is the answer to "a check
   that cannot fail is worse than none", enforced rather than hoped for.
6. **Seven pilot ADRs** were written from decisions already made (0001 task traps, 0002 the Rust is retired, 0003 core-form names, 0004
   FMA by `has-fma`, 0005 the interpreter is a tool, 0006 file and function limits, 0007 windows within 1.3x). All hold today; each
   was made to fail by a real change in a scratch copy of the tree and to hold again (section 9.3).
7. **Speed**: the whole run (seven ADRs, 40 scenarios, 4,200 source files read and parsed) takes about 3.5 seconds under the JIT, 3.6 s
   measured (section 9.1). It is one program, compiled once. The gate stage is wired into `--full` only.

## 1. What exists and what an ADR adds

| Mechanism | Judges | What it cannot do |
|---|---|---|
| `cases/` (`fibc cases`) | one program: compile verdict, `main`'s result, memory audit | say why a decision was made; check the repository's shape |
| `fib.test` specs (`docs/design/test-harness.md`) | behaviour of a library, tool or protocol across implementations | judge the tree (file sizes, who requires whom, what a script calls) |
| `scripts/mutant-*.sh` | whether the cases notice a planted fault | live next to the decision they protect |
| `**Decided**` tags in `spec/*.md` and `docs/design/*.md` | nothing: prose with a sign-off | fail |
| goldens | recorded output equality | say which decision they pin |
| **executable ADR** (this design) | a decision's *claims*: behaviour (spec), structure (fitness), cost (measure), stated beside the reason and the consequences | judgement, taste, intent (section 8) |

An ADR does not replace a case, a spec or a mutant script. It **points at the ones that guard a decision** (its Governance section lists
them, or runs them) and adds the one kind of check the others lack: rules about the repository itself.

Existing decision records, read for this design (a sample): `docs/design/decisions-2026-10-04.md` (numbered decisions with the owner's
words), `docs/design/exceptions.md` (§7 decisions with recommendations, **Decided** quotes), `docs/design/test-harness.md` §10,
`spec/types.md` §2.11 (**Decided**, owner, 2026-09-28: a trap aborts the program), `spec/method.md` ("Decision records": each marked
**Decided** with the owner's sign-off, or **Proposed**). They are prose with tags, which is exactly why they drift: spec/method.md rule 6
still says the interpreter is the oracle, and the owner's amendment of 2026-10-04 ("Rule 6 can suck it") is in another file. ADR 0005 makes
that disagreement visible instead of fixing it (section 6).

## 2. The format

### 2.1 The template (wording from memory: check against the book)

```markdown
# NNNN. Title in the present tense, a decision (not a topic)

Status: accepted | proposed | deprecated | superseded by NNNN
Date: YYYY-MM-DD
Source: where the decision was made (a document, a commit, the owner's message)

## Context
The forces: what is true, what hurts, what was tried. Facts, not the decision.

## Decision
What we do, in full sentences, in the active voice.

## Consequences
What becomes easier, what becomes harder, what we now owe. Include the bad ones.

## Governance
How the decision is kept true: the executable blocks, then prose ("### What this does not check").
```

What I recall of the sources: Nygard's 2011 post "Documenting Architecture Decisions" has Title, Status, Context, Decision, Consequences,
and says to keep each record short and to number it; *The Hard Parts* (chapter 14, "Managing Architectural Data" is not it; the ADR
discussion is in chapter 14's neighbours on architectural decisions) adds that an ADR's Status can be RFC, Proposed, Accepted or
Superseded, adds a **Governance** section describing how the decision is measured and enforced (in the book, as a fitness function
or a manual review), and cautions that a record should state the *why*, since the *what* is in the code. I may have the chapter and the
list of statuses wrong; **verify before freezing**. Nothing in the tool depends on the extra statuses: it understands `accepted`,
`proposed`, `deprecated`, `superseded by NNNN`, and takes `RFC` as an unknown word (reported `invalid`; add it to the list if the book's
vocabulary is wanted).

### 2.2 Files and numbering

`docs/adr/NNNN-slug.md`, `NNNN` at least four digits; the tool reads files whose name starts with four or more digits and ends in `.md`
(a `README.md` or `template.md` beside them is ignored). **The number in the file name is the identity**; the `# NNNN.` in the title is
for readers. The pilot numbers `0001` to `0007` are arbitrary and meant to be reordered (nothing refers to another ADR's number except
`superseded by`, and the scenario ids carry the number: reorder with `git mv` and a search for `b000N-`).

### 2.3 Status is derived

| Declared in the file | Evidence | Derived (printed) | Blocks run? |
|---|---|---|---|
| `accepted` | at least one executable block, a scenario in it ran, all passed | **accepted** | yes |
| `accepted` | a scenario failed, trapped or timed out; a block does not compile; a rule cannot fail (no plant, or a plant it misses) | **violated** | yes |
| `accepted` | no executable block, or none ran a scenario | **unverified** | yes |
| `accepted` | the file breaks the format (no `Status:`, an executable block outside `## Governance`, an unknown block kind, `superseded by` naming no ADR) | **invalid** | no |
| `proposed`, `deprecated` | none asked for | as declared | no |
| `superseded by NNNN` | none: it stops running | `superseded by NNNN` | no |

Exit status: 0 when no ADR is violated or invalid (`--strict`: and none unverified); 1 otherwise; 2 for a usage error (unknown option, a path
that is no ADR, `--only` matching nothing, a bad `--root`). `unverified` passes by default so that writing a decision first and its checks
second does not break a build; the gate uses `--strict`.

`--stamp` writes the derived status into the file, on the line after `Status:`, as `<!-- derived: accepted -->`, **only when the line
differs** (no dates, no counts, nothing that changes between runs). A diff therefore shows only a flip of status, which is what a reviewer
wants to see. It is optional: nothing reads the stamp (it is for a reader on a web view of the file who cannot run the tool).

### 2.4 Executable blocks

A block is a fenced code block whose info string is `fibber KIND`; it counts only under the heading `## Governance` (`###` sub-headings
stay inside it). Other fences (```` ```shell ````, ```` ```fibber ```` alone) are illustration and are never run.

| Block | Body | Runs as |
|---|---|---|
| `fibber spec` | top-level forms: `(feature ..)`, `(implements ..)`, `(outline ..)` or any expression of type `Suite`, and definitions (`defun`, `defstruct`, `defenum`, `def`, `impl`, `defmacro`, `defcontract`, `extern`, `derive`) used by them | the suites of `fib.test`, unchanged (`docs/design/test-harness.md` §2) |
| `fibber fitness` | rules: `(rule ..)`, `(forbid ..)`, `(limit ..)`; `repo` is the loaded repository | one scenario per rule (section 4) |
| `fibber measure` | `(measure ..)` forms | one scenario per measure (section 5) |

Comment directives at the head of a block add modules: `;; :use fib.async` and `;; :require [fib.os :as os]` are copied into the module's `ns`
(the default `:use` list is `fib.core fib.seq fib.coll fib.print fib.string fib.test.core fib.test.gen fib.test.arch`). A name that two of
these export (`includes?` is exported by `fib.coll` and `fib.string`) must be written qualified.

**Why no ```` ```shell check ```` block.** A shell check runs a command and reads its exit status. It has no scenario name, no steps, no
expected and actual to print, no isolation, no planted-violation discipline, and it brings the shell's dialect and the machine's tools into
a decision record. Everything the pilots needed was either a text search over the tree (a `grep`, in fibber, with comment skipping and a
"matches nothing" guard) or a fibber scenario. A shell check is **not supported**; a ```` ```shell check ```` block is reported `invalid`.
The one place a command belongs is a measure's `--rerun`, which is declared data (section 5).

## 3. The tool

### 3.1 Standalone first, one line to integrate

`compiler/adr.fib` (with `compiler/adr/{md,gen,run,json,report}.fib`, each under 500 lines, each function under 50, as CLAUDE.md asks)
is a program like `compiler/lairf.fib`:

```
fibc build compiler/adr.fib -I compiler -I lib -o adr
FIBC=/path/to/fibc adr [PATH..] [--only N[,N..]] [--format plain|json] [--status] [--stamp] [--root DIR] [--rerun] [--strict] [--keep] [-j N]
```

Integration into the driver is a later one-line change in `compiler/driver` (`commands.fib` dispatches `adr` to `adr.report-all`, or the
driver `:require`s `adr` and calls its `main` with the arguments after `adr`); this prototype does not touch `compiler/driver` or
`compiler/fibc.fib`, which another agent is editing for `fibc test`. The tool's only dependency on a compiler is the executable named by
`$FIBC` (default `fibc` on the `PATH`) that it runs blocks with; **inside `fibc adr` that becomes the running executable**
(`fib.os/executable-path`, as `driver.proc` already does) and `$FIBC` goes.

### 3.2 How it composes with `fib.test` and `fibc test`

```
docs/adr/*.md ──parse──▶ blocks ──gen──▶ DIR/adrgen/bNNNN-KK.fib (one module per block) + DIR/adr-spec.fib
                                         │
      fibc test DIR/adr-spec.fib -I compiler -I lib -I DIR --jobs 4 --format json    (the JIT: one compile for every block)
                                         │
   fib.test.run/run-main (isolated per scenario) prints the JSON report ──▶ adr.json ──▶ per-ADR status
```

- The generated `main` is `(run-main (reduce into [] [(retag "b0001-01" (m/suites repo)) ..]))`: **the runner is `fib.test.run`, unchanged**,
  with its isolation (a trap in a rule is a `trap` row, not a crash), its `-j`, its JSON schema 1. A scenario's id carries the block tag
  (`b0001-02/gate-never-calls-cargo`); the tool groups by it.
- Each executable block becomes a module `adrgen.bNNNN-KK` with one function `suites` of the repository. The block's own text stays where
  it was (line for line, after a two-line header), with each expression form wrapped in place as `(defun eN (repo: Repo) -> T <form>)`; so
  a compiler error at line L of the module is line `L + (fence line - 2)` of the Markdown, which the tool prints (`docs/adr/0099-x.md:40:114: cannot
  unify str with i64`). Limit: a form must begin its line and end it without a trailing comment.
- **Failure isolation between ADRs.** If the program does not compile, the compiler's message names a module; that block is reported
  `does not compile: LINE: message`, left out, and the rest run again (at most once per block). One broken ADR cannot hide the others' verdicts.
- **Composition with `fibc test`** (FT1, on main at `dab8213`): `fibc adr` *is* `fibc test` over a generated spec file (`adr-spec.fib`, the
  `*-spec.fib` convention) plus the aggregation by ADR; it uses `--jobs`, `--format json` (schema 1 with the `files` array and the per-step `at`)
  and `-I`, and nothing private. Two facts found on the way: (a) a spec that does not compile is `"outcome":"error"` in the JSON with **no
  message** (only the plain format prints the compiler's text), so the tool reruns with `--format plain` to get it; a `"message"` in the `files`
  entry would remove the second run. (b) The runner isolates by *task* by default, so a scenario closure must be `:send`: the rule engine's
  `RuleDef` carries `(fn :send ..)` fields for that reason. The scenario's `at` ("FILE:LINE:COL" from `(call-pos)`) points into the generated
  module, not the Markdown; the tool's own line mapping (module line + fence line - 2) is used for compile errors, and `at` is left empty for rules
  (not built: mapping `at` back to the Markdown line for failed spec steps).
- **In process, not now.** Compiling the program inside the `fibc` process (`driver.child`'s machinery, as `fibc cases` does) would drop the
  child processes (about 1 s of the 3.6 s) and the `$FIBC` variable. Not built: it is a driver change.

### 3.3 Output

```
ADR   status          blocks  pass  fail  title
0001  accepted        2       11    0     Task traps are isolated at the thread entry
...
7 ADRs: 7 accepted, 0 violated, 0 unverified, 0 invalid, 0 not run
```

Below the table, for each ADR that has notes (a violated one, an invalid one), indented lines `bNNNN-KK: fail: SCENARIO -> STEP: expected E, got A`
with the findings as `file:line: what`. `--status` prints only `NNNN status` lines (the blocks still run: the status is derived, not read).
`--format json`: `{"schema":1,"adrs":[{"adr","title","declared","status","blocks","pass","fail","notes":[..]}],"summary":{..}}`.

## 4. The fitness API: `fib.test.arch`

Library, `lib/fib/test/arch.fib` (the forms) over `arch/{scan,repo,rules}.fib`. `(:use fib.test.arch)` brings all of it.

### 4.1 The repository as data (`arch/repo.fib`)

```
(load-repo ROOT DIRS)          the tree under ROOT: every path (7,100 names today), text and parsed forms for the files of the root
                               and of DIRS (default compiler lib scripts docs spec rt .github editors): about 4,200 files, read once
(Repo root names files)        a value; a rule takes it and returns findings
(Finding file line text)       "file:line: what"
(select repo globs)            files matching a glob (`*` within a segment, `**` across, `?`), minus any glob written `!GLOB`
(names-matching repo globs) (present repo globs) (missing repo paths) (at-least repo globs n) (must-contain repo path text)
(grep repo globs text)  (grep-live ..)   lines containing text; -live skips lines that are only a comment (`#`, `;`, `//`);
                               a glob set that matches no file is itself a finding (a search of nothing finds nothing)
(file-lengths repo globs) (fn-lengths repo globs) (definitions repo globs heads)    Measures: key, file, line, value
(forms-mentioning repo globs names)    top-level forms that mention every name as an atom (not in a string or a comment)
(string-atoms-in repo path name) (row-strings repo path head)    the strings of one form; the second string of every (HEAD "x" ..) row
(defined-among repo globs names)       definitions with one of the names (the self-hosting trap, ADR 0003)
(edges repo globs) (requires-into repo from-globs to-prefix except)    the module graph from the `ns` forms
(over-limit measures max allow) (allowing findings allow)    shrink-only allow-lists (section 4.4)
(plant-append repo path text) (plant-file repo path text) (plant-remove repo path)    another Repo, one file changed (section 4.3)
```

**Cache.** The tree is loaded once per program (one `load-repo` in the generated `main`); each file's top-level forms are read once, at load,
and every rule of every ADR queries the same value. A `fork`ed scenario shares it copy-on-write. A planted Repo re-reads only the planted file.
Measured: loading and parsing 4,215 files takes about 1.3 s of the JIT's `-O0` code; the whole run is 3.6 s (section 9.1). A text of a file
outside the loaded directories (a case under `cases/`) is read on demand when a rule asks for it.

### 4.2 The light reader (`arch/scan.fib`) and why not the real one

The library cannot depend on `compiler/` (it ships in the release tarball and `fib.test.arch` must run in a user's project). So the
module graph, function lengths and definitions come from a **light reader**: nested `(` `[` `{`, atoms, strings, comments, prefix
characters, with the first and last line of each form. It is not the compiler's front end and does not claim to be. It is **checked
against the real reader** by `compiler/tests/adr/reader-agree.fib`, which reads every `.fib` file of `compiler/` and `lib/` with both and
compares each top-level form's head, name and first line and every module an `ns` names: 606 source files, 606 read by both, 0 disagree
(section 9.2). The check found two bugs of the first version on real code (a non-ASCII byte in a string read as the end of the text;
a backslash at the end of the file). The 3,269 test inputs are counted and not gated: 237 disagree, all of them malformed on purpose
(an `ns` whose name is a number), where the two readers are meant to differ.

### 4.3 Rules and planted violations

```clojure
(rule "the gate does not call cargo"                       ; text: the scenario's name
  (grep-live repo ["scripts/gate.sh"] "cargo")             ; the findings: pass when empty; `repo` is the loaded repository
  (plant "scripts/gate.sh" "\ncargo build\n"))             ; REQUIRED: a change under which the check must report something

(limit "files under 500 lines" (file-lengths repo ["compiler/**.fib"]) 499 ["compiler/expand/ctx.fib=525"]
  (plant-file "compiler/zz.fib" (join "\n" (mapv (fn (i: i64) ";;") (range 600)))))
```

| Form | Meaning |
|---|---|
| `(rule "text" check step..)` | a scenario "no violations" that passes when `check` (a `(Vec Finding)`) is empty; one more step per plant |
| `(forbid "text" findings step..)` | the same; the findings *are* the violations |
| `(limit "text" measures max allow step..)` | the measures above `max` are findings, except those in the shrink-only allow-list (4.4) |
| `(plant "path" "text")` `(plant-file "path" "text")` `(plant-remove "path")` | the planted change: append to a file, replace or create it, remove it |
| `(no-plant "why")` | admit that no change can show the rule failing, and say why (a reviewer reads the reason) |
| `(measure "text" :recorded F :key K :under N [:over M])` | section 5 |

The runner applies each plant **in memory**, to a copy of the Repo, runs the same check, and requires at least one finding. Under a plant that
produces none the scenario fails with "the rule cannot fail here (vacuous)"; a rule with neither a plant nor a `no-plant` fails with "the rule
ships a planted violation". So a fitness rule that matches nothing, that greps the wrong path or that was edited into uselessness turns the
ADR `violated` the next run. **The plant is part of the rule's text, in the Markdown, next to the decision** (an alternative, plants as
recipes run against a scratch copy on disk, was rejected: it needs a copy of 16 MB per rule and the in-memory change is exact and
fast; the cost is that a plant cannot show a rule that reads the *disk* behind the Repo's back, which none do).

The base check is also protected from the other side: a glob that matches no file is a finding (so `grep-live` over a renamed script cannot
pass on nothing), and a table-reading rule (ADR 0003) has a companion rule that the table is not empty.

### 4.4 Shrink-only allow-lists

`(limit .. max ["key=cap" ..])`: a measurement over `max` is allowed only if the list names it, and only up to its cap. The three outcomes
are findings: a new violation (not in the list); a listed one that **grew** (the list only shrinks); a listed one that **shrank** (lower the
cap: the list tracks reality), and a listed one that is now within the limit (remove it). ADR 0006 holds today's six exceptions this way.
`(allowing findings ["file:name" ..])` is the same for findings (ADR 0003's four old builtin names).

## 5. Measures

```clojure
(measure "matmul f64 1024 within 1.3x of numpy+OpenBLAS" :recorded "docs/shootout/tensor.md" :key "matmul-f64-1024/openblas" :under 1.3)
```

**A measure checks a recorded number, in the repository, against a threshold; the quoted command re-measures on request.**

- **Record.** One line in the recorded file (any file the Repo loads; HTML comments do not render):
  `<!-- measure KEY VALUE UNIT DATE | cmd: COMMAND -->`, for example
  `<!-- measure rmw-window/array 1.07 ratio 2026-10-05 | cmd: scripts/bench/windows.sh -->`. The latest record of a key (the last line in the
  file) is the one that counts: **records are appended, history stays in git blame**.
- **Fast path (every run, no benchmark).** The tool reads the record from the cached text and compares: `:under N` needs VALUE < N, `:over M`
  needs VALUE > M. A missing record is a finding. Cost: microseconds; the gate can afford it, and it cannot flake.
- **`--rerun`.** Sets `FIB_ADR_RERUN=1`; each measure then runs its record's `cmd` (through `sh`, output to a scratch file), looks for a line
  `measure KEY VALUE` in the output (**the protocol**: `scripts/bench/windows.sh` now prints one for each ratio it checks), and judges that
  value instead. The new value is shown (`re-measured KEY = V (recorded R)`); **nothing is rewritten**: a person pastes the new record.
  A command that prints no such line is a finding. Rerun measures hold `/tmp/fibsuite.lock` through the script, as every benchmark must.
- **Vacuity.** Every measure carries an automatic plant: a record far on the wrong side of the threshold is appended (in memory) and the
  comparison must then fail. A measure whose comparison cannot bite is a failed scenario.
- **What it does not know.** That the recorded number was ever measured, or on which machine: the record is trust until `--rerun`. The date
  in the record is for a reader; the tool does not age records (not built: `:max-age-days`).

## 6. The pilot ADRs

All seven are real decisions already made; none was invented to exercise the tool. Numbers are arbitrary. Each states what it does **not** check.

| ADR | Decision | Blocks | Scenarios | Source | Does not check |
|---|---|---|---|---|---|
| 0001 | Task traps are isolated at the thread entry | spec (4) + fitness (8 rules) | 12 | exceptions.md; cases 911-926 | every kind of trap; shared state a trapped task touched; the leak bound |
| 0002 | The Rust is retired | fitness (6) | 6 | ROADMAP stage 10, `847f785` | that goldens are right; prose that says "cargo" |
| 0003 | New core-form names do not collide with compiler functions | fitness (3) | 3 | CLAUDE.md, the self-hosting trap | names only the seed knows; collisions in `lib/` |
| 0004 | The library chooses FMA by `(has-fma)` | fitness (6) | 6 | CLAUDE.md "Vector fma"; aarch64.md | that `has-fma` is true where it should be; an `fma` behind an alias |
| 0005 | The interpreter is a development tool | fitness (4) | 4 | decisions-2026-10-04.md amendment | **that spec/method.md rule 6 was updated (it was not)** |
| 0006 | Files stay under 500 lines, functions under 50 | fitness (2, `limit`) | 2 | CLAUDE.md "Rust standards" | taste; tests; readability |
| 0007 | Windows run within 1.3x of the array loop | measure (5) + fitness (2) | 7 | exclusive-views.md, `scripts/bench/windows.sh` | other machines; cold caches; that a record was measured |
| 0008 | Supported ISAs have guaranteed tail calls and FMA | fitness (6) | 6 | the owner, 2026-10-06; targets.fib | that the rows are true of LLVM; the warning, trap and start-up check at run time (no-fma.sh, cpu-check.sh) |
| 0012 | The tree has no hand-written cryptography | fitness (5) | 5 | crypto.md; the owner's standing rule | primitives written without the well-known constants; that a provider passes the contract (it runs in the driver's repository) |
| 0013 | Library code has no global mutable state | fitness (3) | 3 | bootstrap.md; observability-and-databases.md 6.1 | state that is not a top-level `def` (runtime globals, `extern`); one allowed entry in `compiler/` |
| 0014 | Every builtin has a spec row and a case | fitness (3) | 3 | method.md; the builtin table | that a mention exercises the builtin; the mutant (prose); 7 builtins without a spec mention and 22 without a case are listed |
| 0015 | The in-place primitives are called from a short list of library files | fitness (3) | 3 | spec/types.md 2.13.1 | that a permitted call meets the null-slot obligations |
| 0016 | Every library facade is named by a spec or a case | fitness (2) | 2 | method.md | the depth of coverage; parts below a facade |
| 0017 | The release files agree and the seed is pinned | fitness (5) | 5 | check-version.sh; SEED | that a url is published (needs the network) |
| 0018 | An idle process gives its cached large blocks back | fitness (3) | 3 | allocator.md (the measuring case 7791 runs in the gate) | the RSS itself (it is the case's job) |
| 0019 | A changed benchmark checksum is a failure, not a speedup | fitness (3) | 3 | scripts/bench/quick.sh | that quick.sh behaves (minutes, a quiet machine) |
| 0020 | A script that downloads also checks a checksum | fitness (2) | 2 | the owner's tool-download rule | scratch only; two CI lines of LLVM's installer are listed |
| 0021 | Parallel reductions do not depend on the worker count | fitness (3) | 3 | parallelism.md 3.5 | the bits themselves (cases 7609 to 7611) |
| 0022 | Exclusive windows are over contiguous buffers, and the checker refuses closures over them | fitness (3) | 3 | decisions-2026-10-04.md, exclusive views | that the checker refuses every closure (the mutant script) |

Rows 0012 to 0022 are the backfill (package ADR-2): decisions that were prose, memory notes or design paragraphs, each with a rule that
ships a planted violation. What was left as a reviewer's judgement is in the package report: the mutant requirement of 0014, scratch-only
downloads (0020), a hash written without the standard constants (0012), the depth of a case (0016).

Things the pilots found (reported, not fixed):

- **0003 found real collisions.** No core or primitive form has a collision (the rule that matters most is clean). Four **builtin** names are
  defined as functions in `compiler/`: `trap` (`driver/header.fib`), `array-with` (`emit/lower/builtins.fib`), `spawn`
  (`native/cases/exec.fib`) and `struct-fields` (three files). They compile today (the seed knew those builtins before the definitions
  existed). They are in the ADR's allow-list; whether to rename them is a decision for the owner.
- **0005 exposes a disagreement.** `spec/method.md` rule 6 still says "The compiler is checked against the interpreter ... A feature is done
  when this passes in CI"; the owner's amendment of 2026-10-04 says rule 6 is dropped. By the project's rule (a spec rule and a decision disagree: stop
  and report) I did not edit either; ADR 0005 records the decision and says in its Consequences that the spec was not updated.
- **0006 counts the exceptions honestly:** three files (`runtime.fib` 2440 lines, generated; `lib/prelude.fib` 721; `expand/ctx.fib` 525) and
  three functions (`mailbox-source` 333, `special-kind` 52, the `scenario` macro 73: it was 77 until FT1 shortened it, and the ratchet noticed and asked for the cap to be lowered); everything else under `compiler/` and `lib/`, outside
  the tests, is under the limits. (The tests have longer files on purpose: 1001-line reader inputs.)

## 7. How this changes the workflow

### 7.1 What an agent writes when it makes a decision

1. Copy the template (section 2.1) to `docs/adr/NNNN-slug.md` (the next number); `Status: proposed` while it is being discussed.
2. Write Context, Decision and Consequences in prose; **write the Governance section before asking for acceptance**: at least one executable
   block that can fail, each rule with its plant. Run `adr --only NNNN`; it must say `accepted` (and you must have seen it say `violated` with
   the plant, which the tool enforces).
3. Under `### What this does not check`, say what the blocks leave to people. The pilots each do; a decision record that claims more than its
   checks is the failure this design exists to prevent.
4. Set `Status: accepted` only with the owner's sign-off for a decision that is the owner's (the existing rule: **Decided** means signed off).
   The tool derives whether the claim holds; it does not decide who may accept.
5. To supersede: write the new ADR, set the old one to `Status: superseded by NNNN`; the old one's blocks stop running (their checks may be
   moved into the new ADR where they still apply).

### 7.2 The gate

`scripts/gate.sh --full` has an **`adr` stage** (wired in this branch, **not** touching `compiler/driver`): after the golden checks it builds
`compiler/adr.fib` with the gate's `F` and runs it with `--strict`, one timing line (`adr 7 ADRs: 7 accepted, ...`), FAIL if any ADR is
violated, invalid or unverified. Measured: about 6 s (3 s to build the tool, 3 s to run it); the quick gate does not run it. Violations are **not**
compared with `scripts/ci-stage2.expected`: a violated decision is a regression, not a known gap. A decision known to be violated is
`proposed` or `deprecated`, or carries a `limit` allow-list, not an expected-failure row. (When `fibc adr` exists the stage becomes
`"$F" adr --strict`, and the build step goes.)

### 7.3 `**Decided**` tags and ADR numbers

Specs mark decisions `**Decided**` with the owner's sign-off and the date. Proposal (not built): a tag may carry an ADR number,
`**Decided** (ADR 0003)`, and the tool's `--links` (not built) lists the tags that name an ADR that does not exist or is `violated`, and the ADRs
no spec points to. The link is **from the spec to the ADR**: the spec stays the statement of the rule; the ADR holds the reason and
the checks. `fibc adr` would also be able to check that a tag naming an ADR exists (a `fitness` rule over `spec/*.md` does it today with
`grep`; ADR 0005's first rule is the same pattern: a sentence on record).

### 7.4 Migrating the existing design docs

Not a rewrite. In order of value, none required at once:

1. Decisions that already have a mechanism (a case, a mutant script, a table in a spec): write the ADR **as the index** of that mechanism
   (its Governance runs a fitness rule that the case exists and its header says what the ADR claims, as 0001 does for cases 911-926).
2. `docs/design/decisions-*.md` entries: one ADR per numbered decision that something in the tree can contradict; the rest stay prose.
3. `**Decided**` tags in specs: add the ADR number when one exists (7.3).
4. A design doc's "Decisions for the owner" sections are the source of future ADRs: each decision the owner signs becomes an ADR with its
   checks.
5. Keep the design docs: they hold the investigation. An ADR cites its source and does not copy it.

### 7.5 Where it fits with the rest

A change that needs an exception to an ADR (a seventh file over 500 lines) edits the allow-list in the ADR in the same commit: the exception is a
reviewed diff next to the reason. A change that violates an ADR **fails the full gate**, which is the point. Mutation reviews (`scripts/mutant-*.sh`)
stay what they are: they test the cases; the ADR's planted violations test the rules.

## 8. Limits and risks

**What cannot be executed.** Judgement and taste: "the code is readable", "the API is pleasant", "this is the right layering". An ADR may state
them in Context and Consequences; they have no block and the Governance section says so. Intent: a rule can see that no module requires
`fibref.*`, not that nobody *meant* to. Cost on other machines. That a recorded number was measured. Decisions about people.

**Risks.**

1. **A check that cannot fail is worse than none.** Mitigations built in: the planted violation per rule (section 4.3, enforced); a glob that
   matches nothing is a finding; a measure carries an automatic plant; tables read by a rule have a "not empty" companion (ADR 0003).
   **Review checklist for a fitness rule** (a human, at review): (a) the plant is the *real* bad change, not a string that happens to contain the
   search text; (b) the plant is in a file the rule's globs select; (c) the base check passes for the right reason (run it with the plant applied
   by hand on a scratch copy once); (d) the rule does not read a table that could be empty; (e) the allow-list entries are today's reality, no
   more; (f) the "does not check" paragraph says what the rule leaves out.
2. **Plants test the rule, not the decision.** A rule can pass its plant and still miss the interesting violation (0004's rule reads one form
   at a time and would not see a helper). The "does not check" paragraph is where that is admitted; a reviewer must keep it honest.
3. **Textual rules over generated or moved code.** ADR 0001's runtime rules read `.lir` source as text: a rename defeats them (they fail
   loudly, since a glob or a `must-contain` then finds nothing, which is the safe direction).
4. **The light reader is not the compiler's.** Section 4.2: checked on every source file, but a syntax the compiler adds that the light reader
   cannot skip would make forms vanish; `reader-agree.fib` then fails (put it in the gate with the `adr` stage: not done, a recommendation).
5. **Compile time scales with blocks.** One program, one compile (1 s of 3.6); 40 scenarios is small. Spec blocks with many scenarios pay
   macro expansion (`test-harness.md` §10 risk 3).
6. **Status theatre.** `accepted` printed by the tool means the *blocks* pass. A decision with weak blocks reads as healthy. The table shows the
   scenario count; a reviewer should distrust an ADR whose count is 1.
7. **The tool's trust root.** `adr` runs code written in a Markdown file. The ADRs are in the repository and reviewed like source; `--rerun`
   runs shell commands from the same files. Run it only on trees you trust, as with `scripts/*.sh`.

## 9. The prototype and its output

Files: `lib/fib/test/arch.fib`, `lib/fib/test/arch/{scan,repo,rules}.fib`, `compiler/adr.fib`, `compiler/adr/{md,gen,run,json,report}.fib`,
`compiler/tests/adr/{run.sh,reader-agree.fib}`, `docs/adr/0001..0007-*.md`, this file, the `adr` stage of `scripts/gate.sh`, and one line
in `scripts/bench/windows.sh` (it prints `measure KEY VALUE`, the `--rerun` protocol), plus the records in `docs/design/exclusive-views.md`.

### 9.1 The tree's ADRs (`adr`, built by `fibc build compiler/adr.fib -I compiler -I lib`, blocks run by `fibc test`)

```
$ FIBC=$F adr
ADR   status              blocks  pass  fail  title
0001  accepted            2       12    0     Task traps are isolated at the thread entry
0002  accepted            1       6     0     The Rust is retired
0003  accepted            1       3     0     New core-form names must not collide with compiler functions
0004  accepted            1       6     0     The library chooses FMA by `(has-fma)`, not by lane count
0005  accepted            1       4     0     The interpreter is a development tool, not an oracle
0006  accepted            1       2     0     The compiler's files stay under 500 lines and its functions under 50
0007  accepted            2       7     0     Windows run within 1.3x of the array loop

7 ADRs: 7 accepted, 0 violated, 0 unverified, 0 invalid, 0 not run        [exit 0]
```

Wall time 3.6 s (`compiler/tests/adr/run.sh` check 1), 40 scenarios, one compile, 4,200 files read. Before the rebase onto FT1's work the same run took 2.7 s
with the private runner; `fibc test` adds a child process and the JSON `files` array, and costs about 0.9 s more.

### 9.2 The light reader against the real reader

```
607 source files, 607 read by both, 0 disagree
3271 test inputs, 2644 read by both, 237 disagree (informational)
```

### 9.3 Each pilot made to fail by a real change, then not (`compiler/tests/adr/run.sh`, check 3)

A scratch copy of the tree (never the tree) with one change for each ADR: `cargo build --release` appended to `scripts/gate.sh` (0002), a
`(call @fib.fail-task t b n)` line in `rt/str.lir` (0001), a new `compiler/emit/zz.fib` defining `splat` (0003), a function using `native-lanes` and
`simd/fma` in `lib/fib/tensor/linalg.fib` (0004), an empty `scripts/compare-interp.sh` (0005), a 601-line `compiler/emit/zz2.fib` (0006), a
newer record `rmw-window/array 2.5` (0007):

```
ADR   status              blocks  pass  fail  title
0001  violated            2       11    1     Task traps are isolated at the thread entry
0002  violated            1       5     1     The Rust is retired
0003  violated            1       2     1     New core-form names must not collide with compiler functions
0004  violated            1       5     1     The library chooses FMA by `(has-fma)`, not by lane count
0005  violated            1       3     1     The interpreter is a development tool, not an oracle
0006  violated            1       1     1     The compiler's files stay under 500 lines and its functions under 50
0007  violated            2       6     1     Windows run within 1.3x of the array loop

0001 violated:
  b0001-02: fail: fib.fail-task is called by the trap path in rt/core.lir and by nothing else -> no violations: ... got rt/str.lir:608: (call @fib.fail-task t b n)
0002 violated:
  b0002-01: fail: the gate does not call cargo or name crates/ -> no violations: ... got scripts/gate.sh:120: cargo build --release
0003 violated:
  b0003-01: fail: no definition in compiler/ has the name of a core, primitive or conversion form -> ... got compiler/emit/zz.fib:2: splat is defined here, and it is the name of a core form or builtin
0004 violated:
  b0004-01: ... got lib/fib/tensor/linalg.fib:77: defun zz mentions ["native-lanes" "simd/fma"]
0005 violated:
  b0005-01: ... got scripts/compare-interp.sh: is in the tree and must not be
0006 violated:
  b0006-01: ... got compiler/emit/zz2.fib:1: compiler/emit/zz2.fib is 601, the limit is 499
0007 violated:
  b0007-01: ... got docs/design/exclusive-views.md:459: rmw-window/array = 2.5, it must be under 1.3

7 ADRs: 0 accepted, 7 violated, 0 unverified, 0 invalid, 0 not run        [exit 1]
```

and the same copy rebuilt without the changes: `7 ADRs: 7 accepted` (exit 0). Every one of the 40 scenarios of the pilots also ships its own planted violation
(applied in memory by the runner), which is why a pilot rule that matched nothing would already be reported.

### 9.4 The tool's own rules (checks 4, 4b, 5 of `run.sh`; 53 checks, `adr test: 0 failed`)

A passing spec is accepted; a failing spec is violated; a rule that cannot fail is violated ("the rule cannot fail here (vacuous)"); a rule with
no plant is violated; a block that does not compile is violated with the Markdown line (`.../1005-synthetic.md:22:28: cannot unify str with i64`) while
the other ADRs still run; no blocks is unverified (`--strict` exits 1); `superseded by 1001` and `proposed` are not run even over a failing block; a missing
`Status:` and a block outside Governance are invalid; a measure reads the record, `--rerun` re-measures (one drifted to 1.9 is violated with the value, a
command that prints no `measure` line is violated); `--only`, `--status`, `--format json`, `--stamp` (writes one line, `<!-- derived: accepted -->`,
and nothing the second time); exit 2 for a bad `--format`, an `--only` that matches nothing, a bad `--root`.

### 9.5 A finding: the windows benchmark is noisy near its limit

Four runs of `scripts/bench/windows.sh` (3, 5, 7, 9 runs per median) gave `rmw window/array` 1.43, 1.01, 1.48, 1.15 and `fill window/array`
0.97, 1.31, 0.97, 1.01 against a limit of 1.3: on the shared machine, two of the four runs failed the script's own check. The records
are in `docs/design/exclusive-views.md` §12, newest last; the newest passes, so 0007 is `accepted`, and it was `violated` after the first record
(shown in the first drafts of this work: `rmw-window/array = 1.43, it must be under 1.3`). **ADR 0007 is accepted on a number that a rerun may not
reproduce**; that is the honest state of the claim, and exactly what `--rerun` is for.


## 10. Not done and decisions for the owner

**Not built.** `fibc adr` as a driver command (a one-line change, section 3.1); the in-process run; `--links` and spec tag checks (7.3); a
Markdown front-end for the `**Decided**` tags; a record writer (`--rerun --record`); record ageing; `RFC` as a status; a matrix of
ADR by scenario; an HTML or `docs` rendering of the table; `reader-agree.fib` in the gate; checking the template against the book; the Windows
and macOS runs (the tool needs `sh` and `fork`: POSIX only, like `fib.test.run`); checking `docs/adr` numbering for gaps and
duplicates (a duplicate number would be refused by the file name only if two files share the name).

**Decisions for the owner.** (a) Approve the format and the three block kinds (2.1, 2.4), after checking the template against *The Hard Parts*.
(b) Whether `unverified` fails the gate (`--strict`, as wired) or warns. (c) Whether the allow-listed builtin collisions of ADR 0003 are
renamed. (d) Whether spec/method.md rule 6 is brought into line with the 2026-10-04 amendment (ADR 0005). (e) Which of the existing `Decided`
decisions become ADRs next (7.4). (f) `fibc adr` as a driver command, once `fibc test` lands, sharing `fib.test.run`.
