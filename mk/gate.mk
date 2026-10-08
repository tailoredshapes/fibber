# mk/gate.mk: the gate as a set of stamps, and the report the lead's automation greps (`build:`, `cases:`, `== timing`, `GATE PASS (full)`).
#   make -j8 gate          every stamp of the full gate, then the report (exit 1 on a failure: make stops at the first, -k runs the rest)
#   make -j8 quick         the quick gate: the tool skeletons, ownership, modules, the stdlib sample (GATE_SPECS=1 adds the specs)
#   make gate-report GATE_MODE=full|quick    the report alone, from the stamps that are there (`$(MAKE) -q` says whether they are current)
# GATE_STATIC=1 / GATE_WASM=1 add those stages to the quick gate. GATE_FRESH=1 rebuilds F from the seed (mk/stage2.mk).
GATE_MODE ?= full
GATE_SPECS ?= 0
GATE_STATIC ?= 0
GATE_WASM ?= 0
FULL_STAMPS := $(BUILD)/version.ok $(BUILD)/runtime-drift.ok $(BUILD)/fixed-point.ok $(GOLDEN_STAMPS) $(TOOLS_FULL_STAMPS) $(BUILD)/adr.ok $(CASES_DIR)/full.ok \
  $(BUILD)/static.ok $(BUILD)/wasm.ok $(if $(wildcard specs),$(BUILD)/specs.ok)
QUICK_STAMPS := $(BUILD)/version.ok $(TOOLS_QUICK_STAMPS) $(CASES_DIR)/quick.ok $(if $(filter 1,$(GATE_STATIC)),$(BUILD)/static.ok) \
  $(if $(filter 1,$(GATE_WASM)),$(BUILD)/wasm.ok) $(if $(filter 1,$(GATE_SPECS)),$(if $(wildcard specs),$(BUILD)/specs.ok))
GATE_STAMPS := $(if $(filter quick,$(GATE_MODE)),$(QUICK_STAMPS),$(FULL_STAMPS))

gate-stamps: $(FULL_STAMPS)
quick-stamps: $(QUICK_STAMPS)
gate: $(FULL_STAMPS)   ## the full gate: stage 2 from the seed, the fixed point, golden, tools, ADRs, every case directory, static, wasm, specs; then the report
	@$(MAKE) --no-print-directory gate-report GATE_MODE=full
quick: $(QUICK_STAMPS)   ## the quick gate: stage 2, the tool skeletons, ownership, modules and a sample of the stdlib cases; then the report
	@$(MAKE) --no-print-directory gate-report GATE_MODE=quick

# The report: one line per stamp (ok with its seconds, SKIPPED with the reason, or FAILED when it is missing), the `cases:` line from the shard
# tables, the timing block, and PASS when `make -q` finds every stamp of the mode current.
gate-report:
	@echo "gate: $(GATE_MODE), make $(MAKE_VERSION), build $(BUILD), seed $(SEED_SHA)"
	@if [ -f $(BUILD)/F.secs ]; then echo "build: F built in $$(cat $(BUILD)/F.secs) s ($(BUILDER))"; else echo "build: no F"; fi
	@echo "== timing"
	@for s in $(GATE_STAMPS); do n=$${s#$(BUILD)/}; n=$${n%.ok}; if [ -f $$s ]; then if grep -q '^SKIPPED' $$s; then echo "  $$n $$(cat $$s)"; else echo "  $$n $$(cat $$s) s"; fi; else echo "  $$n FAILED (no stamp)"; fi; done
	@echo "  $$(scripts/ci-stage2.sh --cases-line $(CASES_DIR) $(if $(filter quick,$(GATE_MODE)),ownership:$(OWN_SHARDS) modules:1 sample:$(QUICK_SHARDS),ownership:$(OWN_SHARDS) modules:1 stdlib:$(SHARDS)))"
	@if $(MAKE) -q --no-print-directory $(if $(filter quick,$(GATE_MODE)),quick-stamps,gate-stamps) GATE_MODE=$(GATE_MODE); then echo "GATE PASS ($(GATE_MODE))"; else echo "GATE FAIL ($(GATE_MODE))"; exit 1; fi

clean-build:   ## remove F, F3 and every stamp (the next gate builds from the seed, as GATE_FRESH=1 does)
	rm -rf $(BUILD)/F $(BUILD)/F.lir $(BUILD)/F.secs $(BUILD)/F3 $(BUILD)/F3.lir $(BUILD)/*.ok $(BUILD)/golden $(BUILD)/tools $(BUILD)/cases $(BUILD)/adr $(BUILD)/gen-runtime
.PHONY: gate quick gate-stamps quick-stamps gate-report clean-build
