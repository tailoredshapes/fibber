# The build: GNU Make coordinates, targets are files

Status: adopted (package MAKE-1, 2026-10-07). The owner: "a menagerie of shell scripts; make Make the build coordinator of
choice; classic make files that target file creation".

## 1. What this is

`Makefile` at the root and `mk/*.mk` (one section each, under 300 lines) are the coordinator. Every stage of the gate, the
release and the tool fetches is a **file**: a built program (`build/F`), a generated source (`compiler/emit/runtime.fib`), a
table of results (`build/cases/stdlib.3.txt`), or a stamp (`build/golden/own-amp.ok`) that holds the seconds the stage took
and exists only when the stage passed on the inputs it names. `make -j8 gate` is the full gate; it is incremental by
construction, because Make compares the stamps with their inputs: a change to one mutant script reruns one stamp, a change to
`lib/fib/json/parse.fib` rebuilds `build/F` and reruns everything downstream of F (which is everything: that is correct), a
change to nothing is a no-op.

GNU Make 4.x (4.4.1 on the box). macOS has BSD make: install `gmake` from Homebrew and run `gmake`; the Makefile refuses other
makes (`mk/config.mk` checks `$(MAKE_VERSION)`).

## 2. The graph

```
SEED ─────────────────────► build/seed/<sha256>/bin/fibc        fetch (scripts/fetch-seed.sh), sha256 verified, unpacked
rt/*.lir + gen-runtime ───► compiler/emit/runtime.fib           generated (a real target; build/runtime-drift.ok compares)
compiler/**/*.fib lib/**  ► build/F                             stage 2, built by the seed (BUILDER=… names another fibc)
build/F ──────────────────► build/F.lir, build/F3, build/F3.lir ► build/fixed-point.ok   (the stage check: cmp of the two emits)
build/F ──────────────────► build/golden/tools/<tool> ─────────► build/golden/<suite>.ok  (one per suite of compiler/tests/golden/suites.sh)
build/F + cases/<dir>/** ─► build/cases/<dir>.<k>.txt ─────────► build/cases/full.ok      (the comparison with scripts/ci-stage2.expected and the floor)
                                                          └────► build/cases/quick.ok     (ownership, modules, the stdlib sample)
build/F + specs/** ───────► build/specs.ok
build/F + a tool script ──► build/tools/<name>.ok               (one per entry of the old scripts/tools.sh queue)
build/F + the tree ───────► build/adr.ok                        (the executable ADRs, --strict)
build/F + musl pieces ────► build/static.ok                     (SKIPPED when there are no musl pieces: FIB_MUSL_DIR or build/musl/<arch>/)
build/F + wasi-sdk ───────► build/wasm.ok                       (SKIPPED when there is no node or no wasi-sdk)
VERSION + version.fib ────► build/version.ok
all of the above ─────────► gate                                (prints the summary; GATE PASS (full) / GATE FAIL (full))
build/F + scripts/mutant-X.sh + the files it plants into ► build/mutants/<X>.ok   (`make mutants`; not in the gate)
build/F + scripts/bench/*.fib ► build/bench/quick.tsv            (`make bench`: scripts/bench/quick.sh against baseline.tsv)
seed + llvm static archives ► build/release/F, F3, fibc (shipped) ► dist/fibc-VERSION-PLATFORM.tar.gz, dist/SHA256SUMS   (`make release`)
scripts/fetch-*.sh ───────► ~/.cache/fibber-scratch/tools/<tool>/… (`make fetch-tools`; each fetch checks the sha256 written in its script)
```

`make help` lists the public targets (the `## ` comments of the rules). `make -n gate` shows what would run.

## 3. Decisions

- **Stamps carry data and are written last.** A stamp's recipe removes the old stamp first, runs the stage with its output in
  `<stamp>.log`, and writes the elapsed seconds into the stamp only when the stage passed (`mk/config.mk`, `stamp`). The shell is
  `bash -eu -o pipefail`, so a failing command fails the recipe; `.DELETE_ON_ERROR` removes a target a failed recipe changed.
- **No `.ONESHELL`.** Each recipe is one shell line (continued with `\`), or calls a kept script. `.ONESHELL` would make every
  recipe in the tree one script, where a failed line in the middle is only caught with `-e` on every recipe; the per-line
  model is the classic one and the kept scripts hold the long logic.
- **`.SECONDARY` with no prerequisites:** no intermediate is deleted (the shard tables, the emits, the golden tools are what you
  read when a stage fails).
- **Parallelism is Make's jobserver.** `make -jN` replaces `scripts/lib/slots.sh` (the slot files) and `GATE_BUDGET`: `scripts/gate.sh`
  passes `-j$GATE_BUDGET`. A recipe holds one job slot. Inside a recipe, `fibc cases -j $(CASE_JOBS)` (default 4) overlaps the
  runs of a shard as before, and a tool script's own parallelism is its own: the kept scripts are not jobserver-aware, and the
  policy is that a recipe is one heavy process plus whatever that process always ran. The stdlib cases are `SHARDS` (default 16)
  targets, ownership 4, so Make schedules them.
- **Recursion:** none, except `gate-report`, which asks `$(MAKE) -q` whether the gate's stamps are up to date (the report must
  print PASS or FAIL after a `-k` run that left some stamps missing), and `scripts/batch.sh`, which runs `make` on a scratch
  worktree.
- **Build directory:** `build/` in the tree (`BUILD=…` moves it), `dist/` for the release; both ignored by git and skipped by
  the ADR scan (`lib/fib/test/arch/repo.fib`). The seed is unpacked under `build/seed/<sha256>/`; downloads are cached across
  worktrees in `FIB_SEED_CACHE` (default `~/.cache/fibber-scratch/seeds`).
- **`GATE_FRESH=1`** (or `make clean-build`): `build/F` gets a `FORCE` prerequisite, so it is rebuilt from the seed and everything
  downstream reruns. The default builder is always the seed, as CI's is; `BUILDER=path/to/fibc` names another.
- **Downloads** stay in the fetch scripts (the recipe body), each with its checksum, so ADR 0020 holds; the lint of ADR 0020 and
  `scripts/lint-pipefail.sh` read `Makefile` and `mk/*.mk` too (a recipe line is a live line, whatever its leading tab, `@`, `-`).
- **The generated runtime:** `compiler/emit/runtime.fib` is a real target of `rt/*.lir` and `build/gen-runtime` (built by the seed:
  the generator uses the library only); the recipe writes it only when the content changed, so a checkout whose `rt/` is newer
  than the committed file does not rebuild F for nothing. `build/runtime-drift.ok` is the comparison (the committed file equals
  what the generator produces); `compiler/tests/emit/runtime.sh` (the tool `sh-emit-runtime`) still runs unit-runtime.

## 4. Scripts: retired, wrapped, kept

| Script | Now |
|---|---|
| `scripts/gate.sh` | thin wrapper: `--full` = `make -k -j$GATE_BUDGET gate-stamps` then `make gate-report`; `--quick` the same on `quick`; prints the same summary lines (`build:`, `cases:`, `== timing`, `GATE PASS (full)`) |
| `scripts/tools.sh` | retired: its queue is `mk/tools.mk` (`make tools`); the script is a wrapper over `make tools` / `make tools-quick` |
| `scripts/lib/slots.sh`, `scripts/lib/fibc-slot.sh` | retired by the jobserver; kept as no-ops for the scripts that still source them (golden.sh, mutants) |
| `scripts/lib/stage2.sh` | kept for `scripts/bench/quick.sh` and `scripts/batch.sh` (the previous-F cache for the bench) |
| `scripts/package.sh` | wrapper over `make release`; its checks are `mk/release.mk` targets (`build/release/binary.ok`, `build/release/tree.ok`) |
| `scripts/fetch-seed.sh`, `scripts/build-musl.sh`, `scripts/llvm-static.sh`, `scripts/fetch-*.sh` | kept as recipe bodies (each checks its checksum) |
| `scripts/ci-stage2.sh` | the comparison recipe: `--from DIR SPEC..` collects the shard tables Make produced and compares them with the expected set and the floor; the old `ci-stage2.sh F` still runs the directories itself |
| `scripts/check-version.sh` | the recipe of `build/version.ok` |
| `scripts/batch.sh` | kept (the lead's integrator); it calls `scripts/gate.sh`, which calls make |
| `scripts/mac-check.sh` | kept; `make mac-check` runs it (gmake on the Mac) |
| `scripts/mutant-*.sh`, `scripts/bench/*`, `compiler/tests/**/*.sh` | kept: they test something; `make mutants`, `make bench`, `make tools` run them |

## 5. Sibling repositories

A library repository (fib-hocon, fib-zlib, …) gets the same shape, small:

```make
# Makefile of a fibber library: GNU Make 4 (gmake on macOS). `make test` is the gate.
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
BUILD ?= build
FIBC_SHA := $(shell sed -n 's/^SHA256=//p' scripts/fetch-fibc.sh)
FIBC ?= $(BUILD)/fibc/$(FIBC_SHA)/bin/fibc
export FIBC

.PHONY: help test fibc mutants bench clean
help: ## this list
	@grep -hE '^[a-zA-Z0-9_./-]+:.*## ' $(MAKEFILE_LIST) | sed 's/:.*## /\t/' | sort
$(BUILD)/fibc/$(FIBC_SHA)/bin/fibc: scripts/fetch-fibc.sh   ## the pinned release (sha256 checked)
	@mkdir -p $(@D)/.. && scripts/fetch-fibc.sh $(@D)/../unpack > /dev/null && rm -rf $(@D) && mv $(@D)/../unpack/fibc-*/bin $(@D) && mv $(@D)/../unpack/fibc-*/share $(@D)/../share
fibc: $(FIBC)   ## fetch the compiler (`make fibc FIBC=/path` uses one you have)
$(BUILD)/specs.ok: $(FIBC) $(wildcard specs/*.fib src/**/*.fib src/*.fib) | $(BUILD)/
	rm -f $@; $(FIBC) test specs && touch $@
$(BUILD)/tests.ok: $(FIBC) $(BUILD)/specs.ok scripts/test.sh $(wildcard corpus/**) | $(BUILD)/
	rm -f $@; scripts/test.sh && touch $@
test: $(BUILD)/specs.ok $(BUILD)/tests.ok   ## `fibc test specs`, then scripts/test.sh (the differential and fuzz checks)
$(BUILD)/mutants.ok: $(FIBC) scripts/mutants.sh $(wildcard src/**/*.fib specs/*.fib) | $(BUILD)/
	rm -f $@; scripts/mutants.sh && touch $@
mutants: $(BUILD)/mutants.ok   ## the planted faults (each must be caught by its spec)
bench: $(FIBC)   ## the speed measurement
	HOCON_BENCH=1 scripts/test.sh
$(BUILD)/:
	mkdir -p $@
clean:   ## remove build/
	rm -rf $(BUILD)
```

Applied to fib-hocon as the example (its CI runs `make test`); the others follow when they are next touched.

## 6. Tests of the build itself

`compiler/tests/make/graph.sh` (the tool `sh-make-graph`) runs on a copy of the tree with a fake `F`: it checks that `make
help` lists every public target, that a touched tool script reruns only its stamp, that a touched compiler source reruns the
cases stamp, that a stamp is not written when its command fails, and that a planted missing prerequisite (a stamp without `F`)
would be caught: the test edits the copy's Makefile and asserts the difference. `scripts/tests/make-incremental.sh` is the
timed check on the real tree (`make gate` a second time is a no-op under 2 s).
