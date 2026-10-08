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
- **Recursion:** `gate` and `quick` call `$(MAKE) gate-report` after their stamps, and `gate-report` asks `$(MAKE) -q` whether every
  stamp of the mode is current (so the report prints PASS or FAIL even after a `-k` run that left stamps missing). `scripts/gate.sh`
  and `scripts/batch.sh` call make. Nothing else recurses.
- **Scratch outside the tree.** `TMPDIR` and the tool scripts' scratch are under `SCRATCH` (default `~/.cache/fibber-scratch/mk-<hash of the
  tree path>`), not under `build/`: with `TMPDIR` inside the worktree `cases/stdlib/8283-str-replace-...` fails in a shard (`expected 0, got
  4294967296`) and passes with `TMPDIR` anywhere else (found while writing this; reported, not explained). A tool that opens a Unix socket
  also needs a short path (`fibc serve`: 100 bytes), which is why the hash is short.
- **Configuration is a file too.** `build/static.cfg` and `build/wasm.cfg` hold the musl library and the wasi-sdk the stamp was made for
  (written when `make` reads the Makefile, only if the content changes), so a stamp that said SKIPPED is redone once the pieces exist.
  Consequence: running make with and without `FIB_MUSL_DIR` alternately reruns the static stage each time.
- **The seed is a path.** `build/seed/<sha256>/bin/fibc`: SEED is read for the sha256 and url, and is no prerequisite, so touching SEED
  without changing it rebuilds nothing; a new sha256 is a new directory and a rebuild of F.
- **Build directory:** `build/` in the tree (`BUILD=…` moves it), `dist/` for the release; both ignored by git and skipped by
  the ADR scan (`lib/fib/test/arch/repo.fib`). The seed is unpacked under `build/seed/<sha256>/`; downloads are cached across
  worktrees in `FIB_SEED_CACHE` (default `~/.cache/fibber-scratch/seeds`).
- **`GATE_FRESH=1`** (or `make clean-build`): `build/F` gets a `FORCE` prerequisite, so it is rebuilt from the seed and everything
  downstream reruns. The default builder is always the seed, as CI's is; `BUILDER=path/to/fibc` names another.
- **Downloads** stay in the fetch scripts (the recipe body), each with its checksum, so ADR 0020 holds; the lint of ADR 0020 and
  `scripts/lint-pipefail.sh` read `Makefile` and `mk/*.mk` too (a recipe line is a live line, whatever its leading tab, `@`, `-`).
- **macOS (MAC-1):** `gmake mac-check` (`mk/mac.mk`) is the gate with the stages, tools and golden suites that cannot run on a Mac left out, each
  with its reason in `compiler/tests/expected-macos.txt`, plus `scripts/mac-check.sh --with-f F` (the machine's own checks). GNU Make 3.81
  (`/usr/bin/make`) stops in `mk/config.mk` with a message. The scripts assume GNU coreutils; on Darwin `mk/config.mk` puts `scripts/portable/bin`
  first on `PATH` (`timeout`, `flock`, `nproc`, `sha256sum`, `sha1sum`, `md5sum`, `sed -i`, `date +%N`, `make` = gmake) and `scripts/portable/bash-env.sh` in
  `BASH_ENV` (a `ulimit -v` that succeeds: macOS has no RLIMIT_AS). `scripts/lint-portable.sh` (a tool of the quick gate) fails on every other
  Linux-only tool or bash 4 feature unless the line says `# linux-only: reason`. The cases that fail only on a Mac are
  `scripts/ci-stage2.expected-darwin`; `LLVM_LIBDIR` defaults to Homebrew's `llvm@21`.
- **The generated runtime:** `compiler/emit/runtime.fib` is a real target of `rt/*.lir` and `build/gen-runtime` (built by the seed:
  the generator uses the library only); the recipe writes it only when the content changed, so a checkout whose `rt/` is newer
  than the committed file does not rebuild F for nothing. `build/runtime-drift.ok` is the comparison (the committed file equals
  what the generator produces); `compiler/tests/emit/runtime.sh` (the tool `sh-emit-runtime`) still runs unit-runtime.

## 4. Scripts: retired, wrapped, kept

| Script | Now |
|---|---|
| `scripts/gate.sh` | thin wrapper, still takes `/tmp/fibsuite.lock`: `--full` = `make -k -j$GATE_BUDGET gate-stamps` then `make gate-report`; `--quick` the same on `quick`; prints the same summary lines (`build:`, `cases:`, `== timing`, `GATE PASS (full)`) |
| `scripts/tools.sh` | wrapper: its queue is `mk/tools.mk` (`make tools`, `make tools-quick`, `make build/tools/NAME.ok`); prints the same `ok NAME N s` / `FAIL NAME` lines |
| `scripts/lib/slots.sh`, `scripts/lib/fibc-slot.sh` | kept, unused by the gate: without `GATE_SLOTS` they run the command at once; `golden.sh` and old tool scripts still source them. `-j` of make is the budget |
| `scripts/lib/stage2.sh` | kept for `scripts/bench/quick.sh` and `scripts/batch.sh` (the previous-F cache for the bench) |
| `scripts/package.sh` | wrapper over `make release`; its checks are `mk/release.mk` targets (`build/release/binary.ok`, `build/release/tree.ok`) |
| `scripts/fetch-seed.sh`, `scripts/build-musl.sh`, `scripts/llvm-static.sh`, `scripts/fetch-*.sh` | kept as recipe bodies (each checks its checksum) |
| `scripts/ci-stage2.sh` | the comparison recipe: `--from DIR TABLE:CASEDIR:K..` collects the shard tables Make produced and compares them with the expected set and the floor; the old `ci-stage2.sh F` still runs the directories itself |
| `scripts/check-version.sh` | the recipe of `build/version.ok` |
| `scripts/batch.sh` | kept (the lead's integrator); it calls `scripts/gate.sh`, which calls make |
| `scripts/mac-check.sh` | kept; `make mac-check` runs it (gmake on the Mac) |
| `scripts/mutant-*.sh`, `scripts/bench/*`, `compiler/tests/**/*.sh` | kept: they test something; `make mutants`, `make bench`, `make tools` run them |

## 5. Sibling repositories

A library repository (fib-hocon, fib-zlib, …) gets the same shape, small:

```make
# fib-hocon: GNU Make 4 coordinates the tests (the template of fibber's docs/design/build.md 5). Targets are files under build/; a stamp
# (build/X.ok) exists only when its check passed on the inputs it names, so a second `make test` runs nothing. macOS: `brew install make`, then gmake.
#   make test            `fibc test` (the specs), then scripts/test.sh (the differential check and the fuzz run)
#   make fibc            the pinned release (scripts/fetch-fibc.sh: sha256 checked) under build/fibc/<sha256>/bin/fibc
#   make mutants         the planted faults (scripts/mutants.sh)
#   make bench           the 10 MB speed run
# FIBC=/path/to/fibc uses a compiler you have instead of the pinned release (an unreleased fibber: its stage 2 works).
ifeq ($(filter 4.% 5.%,$(MAKE_VERSION)),)
  $(error GNU Make 4 or later is needed (this is $(MAKE_VERSION)); on macOS: brew install make, then gmake)
endif
.DEFAULT_GOAL := help
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
.SECONDARY:
BUILD ?= build
FIBC_SHA := $(shell sed -n 's/^SHA256=//p' scripts/fetch-fibc.sh)
FIBC_DIR := $(abspath $(BUILD))/fibc/$(FIBC_SHA)
FIBC_PINNED := $(FIBC_DIR)/bin/fibc
FIBC ?= $(FIBC_PINNED)
export FIBC
SRC := $(shell find src specs specs-net tools -name '*.fib') deps.fib
CORPUS := $(shell find corpus -type f)

# $(call stamp,CMD): CMD's output in the stamp's .log; the stamp holds the seconds it took and exists only when CMD passed.
define stamp
rm -f $@; t0=$$(date +%s); if { $(1); } > $(@:%.ok=%.log) 2>&1; then echo "$$(( $$(date +%s) - t0 ))" > $@; else echo "FAIL $@ (log: $(@:%.ok=%.log))"; tail -n 20 $(@:%.ok=%.log); exit 1; fi
endef
%/:
	mkdir -p $@

$(FIBC_PINNED): scripts/fetch-fibc.sh
	rm -rf $(FIBC_DIR) && mkdir -p $(FIBC_DIR)/unpack && scripts/fetch-fibc.sh $(FIBC_DIR)/unpack > /dev/null
	mv $(FIBC_DIR)/unpack/fibc-*/* $(FIBC_DIR)/ && rm -rf $(FIBC_DIR)/unpack && touch $@ && $@ --version   # touched: the tarball keeps the release-time mtime
fibc: $(FIBC)   ## fetch the pinned fibc release (sha256 checked), or check the one FIBC names
$(BUILD)/specs.ok: $(FIBC) $(SRC) | $(BUILD)/
	$(call stamp,$(FIBC) test)
$(BUILD)/tests.ok: $(BUILD)/specs.ok scripts/test.sh scripts/check-manifest.sh scripts/compare.py $(CORPUS) | $(BUILD)/
	$(call stamp,scripts/test.sh)
$(BUILD)/mutants.ok: $(FIBC) scripts/mutants.sh $(SRC) | $(BUILD)/
	$(call stamp,scripts/mutants.sh)
test: $(BUILD)/specs.ok $(BUILD)/tests.ok   ## the specs (`fibc test`), then scripts/test.sh: the differential check and 100000 mutated inputs
	@echo "PASS: $$(cat $(BUILD)/specs.ok) s specs, $$(cat $(BUILD)/tests.ok) s tests"
mutants: $(BUILD)/mutants.ok   ## the planted faults: each must be caught by the spec named for it
bench: $(FIBC)   ## the 10 MB speed measurement (HOCON_BENCH=1 scripts/test.sh)
	HOCON_BENCH=1 scripts/test.sh
help:   ## this list
	@grep -h -E '^[a-zA-Z0-9_./-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/:[^#]*## /\t/' | sort | awk -F'\t' '{ printf "  %-10s %s\n", $$1, $$2 }'
clean:   ## remove build/
	rm -rf $(BUILD)
.PHONY: fibc test mutants bench help clean
```

Applied to fib-hocon as the example (commit f39a538 on its main: `make test` ran the specs in 60 s and scripts/test.sh in 24 s; a second `make test` ran nothing, 0.6 s); the others follow when they are next touched.

## 6. Tests of the build itself

`compiler/tests/make/graph.sh` (the tool `sh-make-graph`, in the quick gate too) runs on a copy of the tree with a fake `F`: it checks that `make
help` lists every public target, that a touched tool script reruns only its stamp, that a touched compiler source reruns the
cases stamp, that a stamp is not written when its command fails, and that a planted missing prerequisite (a stamp without `F`)
would be caught: the test edits the copy's Makefile and asserts the difference. The no-op gate on the real tree (`make gate` a second time: about 2 s) was timed by hand; see the MAKE-1 report.
