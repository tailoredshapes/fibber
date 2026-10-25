# mk/gate.mk: the gate as a set of stamps, and the report the lead's automation greps (`build:`, `cases:`, `== timing`, `GATE PASS (full)`).
#   make -j8 gate          every stamp of the full gate, then the report (exit 1 on a failure: make stops at the first, -k runs the rest)
#   make -j8 quick         the quick gate: the tool skeletons, ownership, modules, the stdlib sample (GATE_SPECS=1 adds the specs)
#   make gate-report GATE_MODE=full|quick    the report alone, from the stamps that are there (`$(MAKE) -q` says whether they are current)
#   make gate-baseline     rewrite the gate's cost budget (scripts/gate-budget.tsv) from the last complete run; a reviewed change
# GATE_STATIC=1 / GATE_WASM=1 add those stages to the quick gate. GATE_FRESH=1 rebuilds F from the seed (mk/stage2.mk).
GATE_MODE ?= full
GATE_SPECS ?= 0
GATE_STATIC ?= 0
GATE_WASM ?= 0
GATE_CONTROL ?= 0
GATE_BASELINE := scripts/gate-budget.tsv
FULL_UNORDERED := $(BUILD)/gate-report.ok $(BUILD)/version.ok $(BUILD)/runtime-drift.ok $(BUILD)/fixed-point.ok $(GOLDEN_STAMPS) $(TOOLS_FULL_STAMPS) $(BUILD)/adr.ok $(BUILD)/docs.ok $(CASES_DIR)/full.ok $(CASES_DIR)/trap.ok \
  $(BUILD)/static.ok $(BUILD)/wasm.ok $(BUILD)/shared-renderer.ok $(if $(wildcard specs),$(BUILD)/specs.ok) $(if $(filter 1,$(GATE_CONTROL)),$(BUILD)/fixed-point-whole.ok)
# The order make starts the stamps in once F exists: the fixed point first (its chain F.lir, F3, F3.lir is the longest), then by recorded cost,
# longest first (the baseline), then those the baseline has not seen. Only fixed-point.ok and the tool that runs F3 (jit-selfhosted-threads)
# wait for F3; every other stamp needs F alone (compiler/tests/make/graph.sh, check 11).
GATE_COSTLY := $(addprefix $(BUILD)/,$(addsuffix .ok,$(shell [ -f $(GATE_BASELINE) ] && awk -F'\t' '$$1 == "full" && $$2 !~ /^@/ { print $$3 " " $$2 }' $(GATE_BASELINE) | LC_ALL=C sort -rn | cut -d' ' -f2)))
# GATE_EARLY: the persist shards (about 25 to 50 s each, a stamp the baseline has not seen would be queued last) go right after the fixed point.
GATE_EARLY := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(PERSIST_STAMPS)))
FULL_STAMPS := $(BUILD)/fixed-point.ok $(GATE_EARLY) $(filter-out $(BUILD)/fixed-point.ok $(GATE_EARLY),$(filter $(FULL_UNORDERED),$(GATE_COSTLY))) $(filter-out $(BUILD)/fixed-point.ok $(GATE_EARLY) $(GATE_COSTLY),$(FULL_UNORDERED))
QUICK_STAMPS := $(BUILD)/gate-report.ok $(BUILD)/version.ok $(BUILD)/docs-quick.ok $(TOOLS_QUICK_STAMPS) $(CASES_DIR)/quick.ok $(if $(filter 1,$(GATE_STATIC)),$(BUILD)/static.ok) \
  $(if $(filter 1,$(GATE_WASM)),$(BUILD)/wasm.ok) $(if $(filter 1,$(GATE_SPECS)),$(if $(wildcard specs),$(BUILD)/specs.ok))
GATE_STAMPS := $(if $(filter quick,$(GATE_MODE)),$(QUICK_STAMPS),$(FULL_STAMPS))
# when this make started, and its -j: the report measures the run from here (stamps older than this did not run)
GATE_START := $(shell date +%s)
GATE_JOBS := $(or $(patsubst -j%,%,$(filter -j%,$(MAKEFLAGS))),$(shell getconf _NPROCESSORS_ONLN))

# The longest suite (golden/expand-porter) waits for the build of its tool, which is no stamp: asked for early, so that suite does not start last.
GATE_HEAD := $(GOLDEN_DIR)/tools/expand
gate-stamps: $(GATE_HEAD) $(FULL_STAMPS)
quick-stamps: $(QUICK_STAMPS)
gate: $(GATE_HEAD) $(FULL_STAMPS)   ## the full gate: stage 2 from the seed, the fixed point, golden, tools, ADRs, every case directory, static, wasm, specs; then the report
	@$(MAKE) --no-print-directory gate-report GATE_MODE=full GATE_T0=$(GATE_START) GATE_JOBS=$(GATE_JOBS)
quick: $(QUICK_STAMPS)   ## the quick gate: stage 2, the tool skeletons, ownership, modules and a sample of the stdlib cases; then the report
	@$(MAKE) --no-print-directory gate-report GATE_MODE=quick GATE_T0=$(GATE_START) GATE_JOBS=$(GATE_JOBS)

# The report: one line per stamp (ok with its seconds, SKIPPED with the reason, or FAILED when it is missing), the `cases:` line from the shard
# tables, the timing block, the budget block (scripts/gate-report.py: wall, parallelism, critical path, idle time, WARN lines against
# scripts/gate-budget.tsv), and PASS when `make -q` finds every stamp of the mode current.
gate-report:
	@echo "gate: $(GATE_MODE), make $(MAKE_VERSION), build $(BUILD), seed $(SEED_SHA)"
	@if [ -f $(BUILD)/F.secs ]; then echo "build: F built in $$(cat $(BUILD)/F.secs) s ($(BUILDER), $$(cat $(BUILD)/F.mode 2> /dev/null || echo unrecorded))"; else echo "build: no F"; fi
	@if [ -f $(BUILD)/F3.secs ]; then echo "build: F3 built in $$(cat $(BUILD)/F3.secs) s ($$(cat $(BUILD)/F3.mode 2> /dev/null || echo unrecorded))"; fi
	@echo "== timing"
	@for s in $(GATE_STAMPS); do n=$${s#$(BUILD)/}; n=$${n%.ok}; if [ -f $$s ]; then if grep -q '^SKIPPED' $$s; then echo "  $$n $$(cat $$s)"; else echo "  $$n $$(cat $$s) s"; fi; else echo "  $$n FAILED (no stamp)"; fi; done
	@echo "  $$(scripts/ci-stage2.sh --cases-line $(CASES_DIR) $(if $(filter quick,$(GATE_MODE)),ownership:$(OWN_SHARDS) modules:1 sample:$(QUICK_SHARDS),ownership:$(OWN_SHARDS) modules:1 stdlib:$(SHARDS)))"
	@rc=0; scripts/gate-report.py --build $(BUILD) --mode $(GATE_MODE) $(if $(GATE_T0),--t0 $(GATE_T0)) --jobs $(or $(GATE_JOBS),1) --baseline $(GATE_BASELINE) --stamps $(GATE_STAMPS) || rc=$$?; \
	  if ! $(MAKE) -q --no-print-directory $(if $(filter quick,$(GATE_MODE)),quick-stamps,gate-stamps) GATE_MODE=$(GATE_MODE); then echo "GATE FAIL ($(GATE_MODE))"; exit 1; \
	  elif [ $$rc -ne 0 ]; then echo "GATE FAIL ($(GATE_MODE), budget exceeded under GATE_BUDGET_STRICT=1)"; exit 1; else echo "GATE PASS ($(GATE_MODE))"; fi

# The budget is rewritten deliberately, from the last run of the mode when it was clean (every stamp ran) and the box was quiet:
#   make clean-build && make -j12 gate && make gate-baseline        (GATE_MODE=quick for the quick gate's rows)
gate-baseline:   ## rewrite scripts/gate-budget.tsv (the rows of GATE_MODE) from the last complete gate run: a reviewed change
	scripts/gate-report.py --build $(BUILD) --mode $(GATE_MODE) --baseline $(GATE_BASELINE) --write-baseline

# The reporter's own test (a planted slow stamp must warn, an unchanged run must not): a stamp of both gates
$(BUILD)/gate-report.ok: scripts/gate-report.py compiler/tests/make/gate-report.py | $(BUILD)/
	$(call stamp,python3 compiler/tests/make/gate-report.py)

clean-build:   ## remove F, F3 and every stamp (the next gate builds from the seed, as GATE_FRESH=1 does)
	rm -rf $(BUILD)/F $(BUILD)/F.lir $(BUILD)/F.secs $(BUILD)/F3 $(BUILD)/F3w $(BUILD)/F3.lir $(BUILD)/*.ok $(BUILD)/*.secs $(BUILD)/golden $(BUILD)/tools $(BUILD)/cases $(BUILD)/adr $(BUILD)/gen-runtime
.PHONY: gate-baseline gate quick gate-stamps quick-stamps gate-report clean-build
