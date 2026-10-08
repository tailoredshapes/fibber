# mk/tools.mk: the tool tests (the queue of the old scripts/tools.sh): one stamp build/tools/<name>.ok for each script, depending on F, the
# script and its directory (the inputs it reads, where knowable: a change under compiler/tests/lsp/ reruns the three lsp stamps and no other).
# A tool script holds one job of make's -j; what it runs inside is its own (docs/design/build.md 3). Each runs under TOOLS_TIMEOUT seconds
# with FIBC = F, its scratch under build/tools/out/<name>; its output is build/tools/<name>.log.
TOOLS_DIR := $(BUILD)/tools
TOOLS_OUT := $(SCRATCH)/tools
T := compiler/tests
FUZZ_SELFTEST_DEADLINE ?= 8000
tool_env := env FIBC=$(F_ABS) FUZZ_SELFTEST_DEADLINE=$(FUZZ_SELFTEST_DEADLINE) RESUME_OUT=$(TOOLS_OUT)/resume WINDOWS_SCRATCH=$(TOOLS_OUT)/windows

# name := command | inputs (a directory's files, or files)
tool_fibref-skeleton := $(T)/fibref/skeleton.sh $(TOOLS_OUT)/fibref-skeleton | $(wildcard $(T)/fibref/*) $(wildcard compiler/fibref/*.fib compiler/fibref/*/*.fib)
tool_gen-skeleton := $(T)/gen/skeleton.sh $(TOOLS_OUT)/gen-skeleton | $(wildcard $(T)/gen/*) $(wildcard compiler/gen/*.fib compiler/gen/*/*.fib)
tool_lint-pipefail := scripts/lint-pipefail.sh | $(shell find scripts $(T) .github -type f \( -name '*.sh' -o -name '*.yml' \)) Makefile $(wildcard mk/*.mk)
tool_sh-driver-check-lib := $(T)/driver/check-lib.sh $(F_ABS) | $(T)/driver/check-lib.sh $(wildcard $(T)/driver/linklib/*)
tool_units-emit := env UNITS_DIR=emit $(T)/units.sh $(TOOLS_OUT)/units-emit | $(T)/units.sh $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/emit/unit-*.fib)
tool_units-own := env UNITS_DIR=own $(T)/units.sh $(TOOLS_OUT)/units-own | $(T)/units.sh $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/own/unit-*.fib)
tool_units-types := env UNITS_DIR=types $(T)/units.sh $(TOOLS_OUT)/units-types | $(T)/units.sh $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/types/unit-*.fib)
tool_units-rest := env UNITS_DIR=rest $(T)/units.sh $(TOOLS_OUT)/units-rest | $(T)/units.sh $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/*/unit-*.fib)
tool_units-pending := env UNITS_DIR=pending $(T)/units.sh $(TOOLS_OUT)/units-pending | $(T)/units.sh $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/*/unit-*.fib)
tool_shootout-compile := $(T)/shootout-compile.sh $(TOOLS_OUT)/shootout-compile | $(T)/shootout-compile.sh $(wildcard docs/shootout/parallel/*.fib)
tool_sh-driver-cli := $(T)/driver/cli.sh $(F_ABS) | $(T)/driver/cli.sh
tool_sh-driver-demand := $(T)/driver/demand.sh $(F_ABS) | $(T)/driver/demand.sh
tool_sh-driver-muladd := $(T)/driver/muladd.sh $(F_ABS) | $(T)/driver/muladd.sh
tool_sh-driver-no-fma := $(T)/driver/no-fma.sh $(F_ABS) | $(T)/driver/no-fma.sh $(T)/driver/no-fma-faults.sh
tool_sh-driver-cpu-check := $(T)/driver/cpu-check.sh $(F_ABS) | $(T)/driver/cpu-check.sh
tool_sh-driver-target := $(T)/driver/target.sh $(F_ABS) | $(T)/driver/target.sh
tool_sh-driver-test-cmd := $(T)/driver/test-cmd.sh $(F_ABS) | $(T)/driver/test-cmd.sh $(T)/driver/test-project-root.sh
tool_sh-driver-static-host := $(T)/driver/static-host.sh $(F_ABS) | $(T)/driver/static-host.sh
tool_sh-emit-runtime := $(T)/emit/runtime.sh $(F_ABS) $(TOOLS_OUT)/runtime | $(T)/emit/runtime.sh $(T)/emit/gen-runtime.fib $(T)/emit/unit-runtime.fib $(RT_LIR)
tool_sh-emit-defs-order := $(T)/emit/defs-order.sh $(F_ABS) | $(T)/emit/defs-order.sh
tool_sh-stack-stack := $(T)/stack/stack.sh $(F_ABS) | $(wildcard $(T)/stack/*)
tool_sh-own-peek := $(T)/own/peek.sh $(F_ABS) | $(T)/own/peek.sh $(wildcard $(T)/own/peek*)
tool_sh-specs-plant-vec := $(T)/specs/plant-vec.sh $(F_ABS) | $(wildcard $(T)/specs/*)
tool_sh-lanes-lanes := $(T)/lanes/lanes.sh $(F_ABS) | $(wildcard $(T)/lanes/*)
tool_sh-repl-run := $(T)/repl/run.sh $(F_ABS) | $(wildcard $(T)/repl/*)
tool_sh-serve-run := $(T)/serve/run.sh $(F_ABS) | $(wildcard $(T)/serve/*)
tool_sh-emit-resume := $(T)/emit/resume.sh $(F_ABS) | $(T)/emit/resume.sh
tool_sh-emit-windows := $(T)/emit/windows.sh $(F_ABS) | $(T)/emit/windows.sh
tool_sh-fibref-heap-gold := $(T)/fibref/heap-gold.sh | $(wildcard $(T)/fibref/heap-gold*)
tool_sh-native-h-checks := $(T)/native/h-checks.sh | $(wildcard $(T)/native/h-*) $(wildcard $(T)/native/h-cases/*)
tool_sh-native-l1-unit := $(T)/native/l1-unit.sh | $(wildcard $(T)/native/l1-*)
tool_sh-harness-proto-run := $(T)/harness-proto/run.sh | $(wildcard $(T)/harness-proto/*) $(wildcard $(T)/harness-proto/cases/*)
tool_lsp-unit := $(T)/lsp/run.sh | $(wildcard $(T)/lsp/*) $(wildcard compiler/lsp/*.fib)
tool_lsp-server := $(T)/lsp/server.sh $(TOOLS_OUT)/lsp-server | $(wildcard $(T)/lsp/*) $(wildcard $(T)/lsp/fix/*) $(wildcard compiler/lsp/*.fib)
tool_lsp-hardening := bash -c "$(T)/lsp/build.sh $(TOOLS_OUT)/lsp-hard && node $(T)/lsp/hardening.js --server $(TOOLS_OUT)/lsp-hard && node $(T)/lsp/fuzz.js --selftest" | $(wildcard $(T)/lsp/*) $(wildcard compiler/lsp/*.fib)
tool_fibref-heap := $(T)/fibref/heap.sh | $(wildcard $(T)/fibref/*) $(wildcard compiler/fibref/*.fib compiler/fibref/*/*.fib)
tool_gen-rng := $(T)/gen/rng-check.sh | $(wildcard $(T)/gen/*)
tool_gen-compare := bash -c "$(F_ABS) build compiler/fibgen.fib -I compiler -I lib -o $(TOOLS_OUT)/fibgen && $(T)/gen/compare.sh $(TOOLS_OUT)/fibgen pipelines 1 300" | $(wildcard $(T)/gen/*) $(wildcard compiler/gen/*.fib compiler/gen/*/*.fib)
tool_gen-planted := $(T)/gen/planted.sh | $(wildcard $(T)/gen/*)
tool_js-backend := $(T)/js/check.sh $(TOOLS_OUT)/js | $(wildcard $(T)/js/*) $(wildcard $(T)/js/cases/*) $(wildcard compiler/js/*.fib)
tool_deps := $(T)/deps/run.sh $(F_ABS) | $(wildcard $(T)/deps/*)
tool_sh-native-gpu-emit := $(T)/native/gpu-emit.sh | $(T)/native/gpu-emit.sh $(wildcard examples/gpu/*) $(wildcard compiler/native/*.fib)
tool_sh-make-graph := $(T)/make/graph.sh | $(wildcard $(T)/make/*) Makefile $(wildcard mk/*.mk)

TOOLS_QUICK := fibref-skeleton gen-skeleton lint-pipefail sh-driver-check-lib sh-make-graph
TOOLS_FULL := $(TOOLS_QUICK) units-emit units-own units-types units-rest units-pending shootout-compile sh-driver-cli sh-driver-demand sh-driver-muladd \
  sh-driver-no-fma sh-driver-cpu-check sh-driver-target sh-driver-test-cmd sh-driver-static-host sh-emit-runtime sh-emit-defs-order sh-stack-stack \
  sh-own-peek sh-specs-plant-vec sh-lanes-lanes sh-repl-run sh-serve-run sh-emit-resume sh-emit-windows sh-fibref-heap-gold sh-native-h-checks \
  sh-native-l1-unit sh-native-gpu-emit sh-harness-proto-run lsp-unit lsp-server lsp-hardening fibref-heap gen-rng gen-compare gen-planted js-backend deps
# The command is every word up to the `|`, the inputs every word after it.
upto_bar = $(if $(filter |,$(firstword $(1))),,$(if $(1),$(firstword $(1)) $(call upto_bar,$(wordlist 2,999999,$(1))),))
after_bar = $(if $(1),$(if $(filter |,$(firstword $(1))),$(wordlist 2,999999,$(1)),$(call after_bar,$(wordlist 2,999999,$(1)))),)

define tool_target
$(TOOLS_DIR)/$(1).ok: $(F) $(call after_bar,$(tool_$(1))) | $(TOOLS_DIR)/
	rm -rf $(TOOLS_OUT)/$(1) $(TOOLS_OUT)/$(1).tmp; mkdir -p $(TOOLS_OUT)/$(1).tmp
	$$(call stamp,timeout $(TOOLS_TIMEOUT) $(tool_env) TMPDIR=$(TOOLS_OUT)/$(1).tmp $(call upto_bar,$(tool_$(1))))
endef
$(foreach t,$(TOOLS_FULL),$(eval $(call tool_target,$(t))))
TOOLS_QUICK_STAMPS := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(TOOLS_QUICK)))
TOOLS_FULL_STAMPS := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(TOOLS_FULL)))
tools: $(TOOLS_FULL_STAMPS)   ## every tool test (the unit programs, the language server, the fibref and fibgen ports, the js backend, deps)
tools-quick: $(TOOLS_QUICK_STAMPS)   ## the quick gate's tool tests (the skeletons, the lint, check-lib, the build's own graph test)
.PHONY: tools tools-quick
tools-list:   ## the stamps of the tool tests (TOOLS_MODE=quick for the quick gate's)
	@printf '%s\n' $(if $(filter quick,$(TOOLS_MODE)),$(TOOLS_QUICK_STAMPS),$(TOOLS_FULL_STAMPS))
.PHONY: tools-list
