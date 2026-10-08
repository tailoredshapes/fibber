# mk/golden.mk: the golden checks of the passes (compiler/tests/golden): the pass tools built by F, then one stamp per suite of suites.sh,
# depending on the tools the suite uses, its .files list, its .golden and every input the list names.
GOLDEN_DIR := $(BUILD)/golden
GOLDEN_TOOLS := read expand types own explain emit lairf
GOLDEN_TOOL_BIN := $(addprefix $(GOLDEN_DIR)/tools/,$(GOLDEN_TOOLS))
# NAME|TOOL rows of suites.sh
GOLDEN_ROWS := $(shell bash -c '. compiler/tests/golden/suites.sh; suite_table' | cut -d'|' -f1,2)
GOLDEN_SUITES := $(foreach r,$(GOLDEN_ROWS),$(word 1,$(subst |, ,$(r))))
golden_tool = $(word 2,$(subst |, ,$(filter $(1)|%,$(GOLDEN_ROWS))))
GOLDEN_STAMPS := $(addsuffix .ok,$(addprefix $(GOLDEN_DIR)/,$(GOLDEN_SUITES)))

$(GOLDEN_DIR)/tools/%: compiler/%.fib $(F) $(LLVM_DEP) | $(GOLDEN_DIR)/tools/
	$(F) build $< $(BUILD_ARGS) $(LLVM_ARGS) -o $@.new > $@.log 2>&1 || { tail -n 20 $@.log; exit 1; }; mv $@.new $@

define golden_suite
$(GOLDEN_DIR)/$(1).ok: $(GOLDEN_DIR)/tools/$(call golden_tool,$(1)) compiler/tests/golden/$(1).files compiler/tests/golden/$(1).golden compiler/tests/golden/golden.sh compiler/tests/golden/suites.sh $$(shell cat compiler/tests/golden/$(1).files) | $(GOLDEN_DIR)/
	$$(call stamp,GOLDEN_OUT=$(ABS_BUILD)/golden/out.$(1) compiler/tests/golden/golden.sh --tools $(ABS_BUILD)/golden/tools --only $(1))
endef
$(foreach s,$(GOLDEN_SUITES),$(eval $(call golden_suite,$(s))))

golden: $(GOLDEN_STAMPS)   ## the golden checks of the passes, one stamp per suite (build/golden/<suite>.ok)
golden-update: $(GOLDEN_TOOL_BIN)   ## regenerate every golden from stage 2 after an intended change (read the diff)
	GOLDEN_OUT=$(ABS_BUILD)/golden/out compiler/tests/golden/golden.sh --update --tools $(ABS_BUILD)/golden/tools
.PHONY: golden golden-update
