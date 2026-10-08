# mk/mac.mk: `gmake mac-check`, the gate on an Apple Silicon Mac (package MAC-1; macOS needs GNU Make 4, which Homebrew installs as gmake: mk/config.mk
# stops early with that message on the 3.81 of /usr/bin/make). It is the full gate with the stages and tools that cannot run on macOS left out, each named
# with its reason in compiler/tests/expected-macos.txt (a `tool` or `stage` line), and with this machine's own checks (scripts/mac-check.sh --with-f F:
# arm64, the LLVM 21 dylib, hello world by AOT and JIT, every -O level) added:
#   stage 2 (F) built by the seed (the darwin-arm64 row of SEED), the fixed point, the generated runtime, golden, the tools, the ADRs, the case
#   directories (compared with scripts/ci-stage2.expected and, on macOS, scripts/ci-stage2.expected-darwin), the specs, static and wasm (SKIPPED
#   without their toolchains: a stamp that says SKIPPED is no pass, the report lists it).
# LLVM_LINK=shared (the default of a gate) links F to /opt/homebrew/opt/llvm@21/lib; the release (mk/release.mk) links it statically.
# usage: gmake -j8 mac-check [CASE_JOBS=8]       MAC_QUICK=1: the quick set only (the tool skeletons, ownership, modules, the stdlib sample, the machine checks)
MAC_EXPECTED := compiler/tests/expected-macos.txt
MAC_SKIP_TOOLS := $(shell awk '$$1 == "tool" { print $$2 }' $(MAC_EXPECTED) 2> /dev/null)
MAC_SKIP_STAGES := $(shell awk '$$1 == "stage" { print $$2 }' $(MAC_EXPECTED) 2> /dev/null)
MAC_TOOLS := $(filter-out $(MAC_SKIP_TOOLS),$(TOOLS_FULL))
MAC_TOOL_STAMPS := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(MAC_TOOLS)))
MAC_FULL_STAMPS := $(BUILD)/version.ok $(BUILD)/runtime-drift.ok $(BUILD)/fixed-point.ok $(GOLDEN_STAMPS) $(MAC_TOOL_STAMPS) $(BUILD)/adr.ok $(CASES_DIR)/full.ok \
  $(if $(wildcard specs),$(BUILD)/specs.ok) $(filter-out $(MAC_SKIP_STAGES:%=$(BUILD)/%.ok),$(BUILD)/static.ok $(BUILD)/wasm.ok)
MAC_QUICK_STAMPS := $(BUILD)/version.ok $(TOOLS_QUICK_STAMPS) $(CASES_DIR)/quick.ok
MAC_STAMPS := $(if $(MAC_QUICK),$(MAC_QUICK_STAMPS),$(MAC_FULL_STAMPS)) $(BUILD)/mac-machine.ok

$(BUILD)/mac-machine.ok: $(F) scripts/mac-check.sh scripts/portable/env.sh compiler/tests/native/a64-o0.sh | $(BUILD)/
	$(call stamp,scripts/mac-check.sh --with-f $(F_ABS))

mac-stamps: $(MAC_STAMPS)
mac-check: $(MAC_STAMPS)   ## on an Apple Silicon Mac, with gmake: the gate minus what macOS cannot run (compiler/tests/expected-macos.txt), plus the machine checks; MAC_QUICK=1 for the short set
	@$(MAKE) --no-print-directory mac-report

# The report: one line per stamp, the tools and stages skipped with their reasons, and PASS when every stamp is current.
mac-report:
	@echo "mac-check: $(PLATFORM), GNU Make $(MAKE_VERSION), LLVM_LINK=$(LLVM_LINK), build $(BUILD), seed $(SEED_SHA)"
	@if [ -f $(BUILD)/F.secs ]; then echo "build: F built in $$(cat $(BUILD)/F.secs) s ($(BUILDER))"; fi
	@echo "== stamps"
	@for s in $(MAC_STAMPS); do n=$${s#$(BUILD)/}; n=$${n%.ok}; if [ -f $$s ]; then if grep -q '^SKIPPED' $$s; then echo "  $$n $$(cat $$s)"; else echo "  $$n $$(cat $$s) s"; fi; else echo "  $$n FAILED (no stamp)"; fi; done
	@echo "  $$(scripts/ci-stage2.sh --cases-line $(CASES_DIR) $(if $(MAC_QUICK),ownership:$(OWN_SHARDS) modules:1 sample:$(QUICK_SHARDS),ownership:$(OWN_SHARDS) modules:1 stdlib:$(SHARDS)))"
	@echo "== not run on macOS ($(MAC_EXPECTED))"
	@awk '$$1 == "tool" || $$1 == "stage" { r = $$0; sub(/^[^ ]+ +[^ ]+ +/, "", r); printf "  %s %s: %s\n", $$1, $$2, r }' $(MAC_EXPECTED)
	@if $(MAKE) -q --no-print-directory mac-stamps $(if $(MAC_QUICK),MAC_QUICK=1,); then echo "MAC-CHECK PASS$(if $(MAC_QUICK), (quick),)"; else echo "MAC-CHECK FAIL"; exit 1; fi
.PHONY: mac-stamps mac-report mac-check
