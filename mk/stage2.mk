# mk/stage2.mk: the generated runtime, stage 2 (F), the fixed point (F3) and the version check (docs/design/build.md 2).
# The sources of a stage 2: every .fib under compiler/ (tests and the pending backlog excepted) and lib/ (the prelude is embedded).
COMPILER_SRC := $(shell find compiler lib -name '*.fib' -not -path 'compiler/tests/*' -not -path 'compiler/mirror-pending/*' | LC_ALL=C sort)
RT_LIR := $(wildcard rt/*.lir)
GEN_RUNTIME := $(BUILD)/gen-runtime
RUNTIME_FIB := compiler/emit/runtime.fib
BUILD_ARGS := -I compiler -I lib

# GATE_FRESH=1: F is rebuilt whatever the stamps say (CI's run: nothing cached), and everything downstream reruns.
ifeq ($(GATE_FRESH),1)
  FRESH := FORCE
else
  FRESH :=
endif

# The generator of compiler/emit/runtime.fib uses the library only, so the seed builds it. The generator is an order-only prerequisite of the
# file: a rebuilt generator (a new seed, a prelude change) does not by itself rewrite the committed file, since its content would be the same
# and F would be rebuilt for nothing; a changed rt/*.lir does. A hand-edited runtime.fib is newer than rt/, so it is not regenerated either:
# build/runtime-drift.ok (which does depend on the generator) is what catches both.
$(GEN_RUNTIME): compiler/tests/emit/gen-runtime.fib lib/prelude.fib $(BUILDER) | $(BUILD)/ $(BUILD)/tmp/
	$(BUILDER) build $< $(BUILD_ARGS) -o $@
$(RUNTIME_FIB): $(RT_LIR) | $(GEN_RUNTIME)
	$(GEN_RUNTIME) rt > $@.new && mv $@.new $@
$(BUILD)/runtime-drift.ok: $(RUNTIME_FIB) $(RT_LIR) $(GEN_RUNTIME) | $(BUILD)/
	$(call stamp,$(GEN_RUNTIME) rt > $(BUILD)/runtime.generated && cmp $(BUILD)/runtime.generated $(RUNTIME_FIB))
runtime: $(RUNTIME_FIB) $(BUILD)/runtime-drift.ok   ## regenerate compiler/emit/runtime.fib from rt/*.lir and check the committed file equals it

# Stage 2: F, built by the seed (or BUILDER); F.lir is what F emits for itself; F3 is built by F; the fixed point is F.lir = F3.lir.
$(F): $(COMPILER_SRC) $(BUILDER) $(LLVM_DEP) $(FRESH) | $(BUILD)/ $(BUILD)/tmp/
	rm -f $@ $(BUILD)/F3 $(BUILD)/F.lir $(BUILD)/F3.lir
	t0=$$(date +%s); $(BUILDER) build compiler/fibc.fib $(BUILD_ARGS) $(LLVM_ARGS) -o $@.new > $(BUILD)/F.log 2>&1 || { tail -n 20 $(BUILD)/F.log; exit 1; }; mv $@.new $@; echo "$$(( $$(date +%s) - t0 ))" > $(BUILD)/F.secs
$(BUILD)/F.lir: $(F)
	$(F) emit $(BUILD_ARGS) compiler/fibc.fib > $@.new 2> $(BUILD)/F.lir.err && mv $@.new $@
$(BUILD)/F3: $(F) $(LLVM_DEP)
	$(F) build compiler/fibc.fib $(BUILD_ARGS) $(LLVM_ARGS) -o $@.new > $(BUILD)/F3.log 2>&1 || { tail -n 20 $(BUILD)/F3.log; exit 1; }; mv $@.new $@
$(BUILD)/F3.lir: $(BUILD)/F3
	$(BUILD)/F3 emit $(BUILD_ARGS) compiler/fibc.fib > $@.new 2> $(BUILD)/F3.lir.err && mv $@.new $@
$(BUILD)/fixed-point.ok: $(BUILD)/F.lir $(BUILD)/F3.lir
	$(call stamp,cmp $(BUILD)/F.lir $(BUILD)/F3.lir && echo "fixed point: F and F3 emit the same lIR ($$(wc -c < $(BUILD)/F.lir) bytes)")
F: $(F)   ## stage 2, build/F, built by the seed (BUILDER=... or FIBC=... names another builder; GATE_FRESH=1 rebuilds it)
fixed-point: $(BUILD)/fixed-point.ok   ## the stage check: F builds F3 and both emit the same lIR for compiler/fibc.fib

$(BUILD)/version.ok: VERSION compiler/driver/version.fib scripts/check-version.sh | $(BUILD)/
	$(call stamp,scripts/check-version.sh)

# The merged static LLVM (scripts/llvm-static.sh): an ld script on Linux, a libtool archive on macOS; `args` holds the -L/-l words.
$(LLVM_STATIC_ARGS): scripts/llvm-static.sh
	mkdir -p $(@D) && scripts/llvm-static.sh $(@D) > $@.new && mv $@.new $@
.PHONY: F fixed-point runtime
