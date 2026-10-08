# mk/checks.mk: the executable ADRs, the specs, the static and the wasm stages (each a stamp under build/).
ADR_INPUTS := $(shell find docs spec scripts cases specs rt .github editors compiler lib mk -type f -not -path 'compiler/tests/golden/*' | LC_ALL=C sort) SEED VERSION Makefile
$(BUILD)/adr: compiler/adr.fib $(wildcard compiler/adr/*.fib) $(F) | $(BUILD)/
	$(F) build compiler/adr.fib $(BUILD_ARGS) -o $@.new > $@.log 2>&1 || { tail -n 20 $@.log; exit 1; }; mv $@.new $@
# The ADRs read the whole tree (docs/design/executable-adrs.md); --strict: an accepted ADR with no check that runs is a failure too.
$(BUILD)/adr.ok: $(BUILD)/adr $(ADR_INPUTS)
	$(call stamp,FIBC=$(F_ABS) $(BUILD)/adr --strict)
adr: $(BUILD)/adr.ok   ## the executable ADRs (docs/adr), --strict

SPECS_INPUTS := $(shell find specs -type f 2> /dev/null | LC_ALL=C sort)
$(BUILD)/specs.ok: $(F) $(SPECS_INPUTS) | $(BUILD)/
	$(call stamp,$(F) test specs -j $(CASE_JOBS) --seed 1)
specs: $(BUILD)/specs.ok   ## `F test specs`, deterministic (--seed 1); every scenario must hold

# The static stage (docs/design/static-linking.md 4): `fibc build --static` against the musl pieces, when there are any (FIB_MUSL_DIR, or
# build/musl/<arch>/ from `make musl`); else SKIPPED with the reason: the toolchain is no dependency of the gate.
MUSL_ARCH := $(if $(filter aarch64 arm64,$(UNAME_M)),aarch64,x86_64)
MUSL_DIR ?= $(if $(FIB_MUSL_DIR),$(FIB_MUSL_DIR),$(ABS_BUILD)/musl)
MUSL_LIBC := $(firstword $(wildcard $(MUSL_DIR)/$(MUSL_ARCH)/libc.a $(MUSL_DIR)/libc.a))
STATIC_CASES := 655- 665- 1709- 2228- 2650- 4005- 6106- 6222- 7304- 8000- 8001- 8002- 8003- 8060- 8061-
STATIC_INPUTS := $(OWNERSHIP_NAMES:%=cases/ownership/%) $(call case_files,modules,$(MODULES_NAMES)) $(wildcard compiler/tests/driver/linklib.sh compiler/tests/driver/linklib/*) \
  $(foreach p,$(STATIC_CASES),$(wildcard cases/stdlib/$(p)*))
# the stamp also depends on whether the pieces are there (a SKIPPED stamp must not outlive a musl directory that appears later)
# (written at parse time, only when its content changes, so `make -q` and `make -n` stay honest)
$(call cfg_write,$(BUILD)/static.cfg,$(or $(MUSL_LIBC),none))
$(BUILD)/static.ok: $(F) $(STATIC_INPUTS) $(MUSL_LIBC) $(BUILD)/static.cfg | $(BUILD)/
ifeq ($(MUSL_LIBC),)
	$(call skip,no musl pieces (FIB_MUSL_DIR, or make musl): the static build was not tested)
else
	$(call stamp,FIB_MUSL_DIR=$(MUSL_DIR) F=$(F_ABS) compiler/tests/driver/linklib.sh \
	  && FIB_MUSL_DIR=$(MUSL_DIR) FIB_STATIC=1 FIB_STATIC_CASES=1 $(F) cases cases/ownership -j $(CASE_JOBS) \
	  && FIB_MUSL_DIR=$(MUSL_DIR) FIB_STATIC=1 FIB_STATIC_CASES=1 $(F) cases cases/modules -j $(CASE_JOBS) \
	  && FIB_MUSL_DIR=$(MUSL_DIR) FIB_STATIC=1 FIB_STATIC_CASES=1 $(F) cases cases/stdlib --only $(STATIC_CASES) -j $(CASE_JOBS))
endif
static: $(BUILD)/static.ok   ## cases as static (musl) executables, when the musl pieces are there
# The musl pieces themselves (scripts/build-musl.sh: the pinned musl source, sha256 checked, built with gcc): `make musl`, or for aarch64 `make musl MUSL_ARCH=aarch64`.
$(BUILD)/musl/$(MUSL_ARCH)/libc.a: scripts/build-musl.sh rt/static/cpuid.c |
	scripts/build-musl.sh $(MUSL_ARCH) $(@D) $(MUSL_TARBALL)
musl: $(BUILD)/musl/$(MUSL_ARCH)/libc.a   ## build the musl pieces of `fibc build --static` into build/musl/<arch>/ (downloads musl, sha256 checked)

# The wasm stage (docs/design/wasm.md 7): the wasm32-wasi build under node, when node and a wasi-sdk are there (WASI_SDK, or the one
# scripts/fetch-wasm-tools.sh puts under TOOLS_CACHE); else SKIPPED with the reason.
TOOLS_CACHE ?= $(HOME)/.cache/fibber-scratch/tools
WASI_SDK ?= $(patsubst %/,%,$(lastword $(wildcard $(TOOLS_CACHE)/wasm/wasi-sdk-*/)))
export WASI_SDK
$(call cfg_write,$(BUILD)/wasm.cfg,$(or $(WASI_SDK),none) $(shell command -v node))
WASM_INPUTS := $(wildcard compiler/tests/wasm/*) $(OWNERSHIP_NAMES:%=cases/ownership/%) $(call case_files,modules,$(MODULES_NAMES))
$(BUILD)/wasm.ok: $(F) $(WASM_INPUTS) $(BUILD)/wasm.cfg | $(BUILD)/
ifeq ($(or $(WASI_SDK),$(WASM_LD),$(shell command -v wasm-ld 2> /dev/null)),)
	$(call skip,no wasm toolchain (make fetch-wasm, then WASI_SDK): the wasm32-wasi build was not tested)
else ifeq ($(shell command -v node 2> /dev/null),)
	$(call skip,no node: the wasm32-wasi build was not tested)
else
	$(call stamp,bash compiler/tests/wasm/wasm.sh $(F_ABS) \
	  && python3 compiler/tests/wasm/suite.py --fibc $(F_ABS) cases/ownership -j $(CASE_JOBS) --expected compiler/tests/wasm/expected.txt \
	  && python3 compiler/tests/wasm/suite.py --fibc $(F_ABS) cases/modules -j $(CASE_JOBS) --expected compiler/tests/wasm/expected.txt)
endif
wasm: $(BUILD)/wasm.ok   ## cases as wasm modules under node, when the wasi-sdk is there
.PHONY: adr specs static musl wasm

# docs/design/aarch64-sibcall.md: the LLVM fault behind the DARWIN-2 crash, by llc alone (no fibber, no AArch64 hardware). LLC=/usr/lib/llvm-21/bin/llc to choose.
# min.ll (an unmarked call) must be `bl callee`; min-tail.ll (the same call marked `tail`) is `b callee` while the fault stands. Not part of any gate.
LLC ?= llc
REPRO_SIBCALL := docs/repro/aarch64-sibcall
repro-aarch64-sibcall:   ## the LLVM sibling-call fault by llc alone (docs/design/aarch64-sibcall.md); LLC=path
	@$(LLC) --version | grep 'LLVM version'
	@for f in min.ll min-tail.ll; do printf '%-13s %s\n' $$f "$$($(LLC) -mtriple=aarch64-linux-gnu -O2 $(REPRO_SIBCALL)/$$f -o - | grep -E '^[[:space:]]+bl?[[:space:]]+callee' | tr -s '\t ' ' ')"; done
	@$(LLC) -mtriple=aarch64-linux-gnu -O2 $(REPRO_SIBCALL)/min-frame.ll -o - | grep -E '(sub|add)[[:space:]]+sp, sp|\[sp\], #|^[[:space:]]+bl?[[:space:]]+callee' | tr -s '\t ' ' '
	@asm="$$($(LLC) -mtriple=aarch64-linux-gnu -O2 $(REPRO_SIBCALL)/min.ll -o -)"; printf '%s\n' "$$asm" | grep -qE '^[[:space:]]+bl[[:space:]]+callee' || { echo 'the unmarked call is not a bl: the control failed' >&2; exit 1; }
.PHONY: repro-aarch64-sibcall
