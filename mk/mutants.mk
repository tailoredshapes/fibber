# mk/mutants.mk: the planted faults (scripts/mutant-*.sh: each breaks one rule in a copy of the tree and expects the test that pins the rule
# to fail). One stamp build/mutants/<script>[-<mode>].ok for each script, or each mode of a script that takes exactly one MODE, depending on
# F, the script and the files it plants into (the .fib and .lir paths the script names). Not in the gate (mutation reviews run beside it);
# `make -j8 mutants` runs them all, `make build/mutants/json.ok` one. Each runs with FIBC = F and its scratch under build/mutants/out/<name>.
MUTANTS_DIR := $(BUILD)/mutants
MUTANTS_OUT := $(ABS_BUILD)/mutants/out
# what a mutant script plants into: the lib/, compiler/ and rt/ files its text names
mutant_files = $(sort $(wildcard $(shell grep -o -E '(lib|compiler|rt)/[A-Za-z0-9_./-]+\.(fib|lir)' scripts/mutant-$(1).sh)))
# the scripts that take [MUTANT..] (default: all) or nothing
MUTANTS_ALL := blit deps elem-get fieldmove followup http json json-schema lz4 merkle peek recur regex-literal simd-literal utf8-rt crypto-encoding dns
# the scripts that take MODE|all
MUTANTS_MODE_ALL := obs parallel
# the scripts that take F as their argument
MUTANTS_F := avx512 autodiff tensor-fused tensor-gaps
# the scripts that take BUILDER [OUTDIR]
MUTANTS_BUILDER := views wasm simd-lower simd-p4b
# the scripts that take one MODE: script:mode
MUTANTS_MODES := $(foreach m,mention param release writeback,amp-param:$(m)) $(foreach m,floor-128k class-up no-floor-op,alloc-floor:$(m)) \
  $(foreach m,cas-blind cas-says-true swap-no-retry read-first cas-obj-leak,atoms:$(m)) \
  $(foreach m,no-release no-pending double-release finally-catchable no-catch catch-everywhere take-unwinds update-unwinds no-write-back swap-keeps-snapshot ordinary,catch:$(m)) \
  $(foreach m,skip-children keep-count shared-mask write-in-place no-check frozen-false no-send copy-same copy-no-retain copy-shared chunk-no-copy,freeze:$(m)) \
  $(foreach m,stale-cache no-dependents no-supervisor repl-trap-kills redefine-keeps-old,serve:$(m)) \
  $(foreach m,no-isolate join-ignores ok-failed no-release double-release leak-message main-isolated oom-isolated ex-cause ordinary,task-trap:$(m)) \
  $(foreach m,push-old push-shared push-full kw-no-literal,libfix:$(m)) $(foreach m,detach join-all ordinary,spawn-leak:$(m)) \
  $(foreach m,no-handler no-altstack no-task-guard any-fault window no-main-stack,stack-guard:$(m)) $(foreach m,col line,call-pos:$(m)) \
  $(foreach m,no-guard no-signal no-redirect no-cleanup no-exit,fork:$(m)) $(foreach m,resolution-off no-gain binders private-plain helpers-off,macro-ns:$(m)) \
  $(foreach m,defs-order no-ctor-eta lit-no-range lit-no-adopt pat-no-adopt no-shadow-note,lang2:$(m)) \
  $(foreach m,noescape wrong-instance not-ready,impl-escape:$(m)) $(foreach m,always nocount,unique:$(m))
mutant_env = env FIBC=$(F_ABS) MUT_OUT=$(MUTANTS_OUT)/$(1) TMPDIR=$(MUTANTS_OUT)/$(1) $(if $(MUSL_LIBC),FIB_MUSL_DIR=$(MUSL_DIR),)

# $(call mutant_rule,NAME,SCRIPT,ARGS): the stamp build/mutants/NAME.ok
define mutant_rule
$(MUTANTS_DIR)/$(1).ok: $(F) scripts/mutant-$(2).sh $(call mutant_files,$(2)) | $(MUTANTS_DIR)/
	mkdir -p $(MUTANTS_OUT)/$(1)
	$$(call stamp,$(call mutant_env,$(1)) scripts/mutant-$(2).sh $(3))
endef
$(foreach s,$(MUTANTS_ALL),$(eval $(call mutant_rule,$(s),$(s),)))
$(foreach s,$(MUTANTS_MODE_ALL),$(eval $(call mutant_rule,$(s),$(s),all)))
$(foreach s,$(MUTANTS_F),$(eval $(call mutant_rule,$(s),$(s),$(F_ABS))))
$(foreach s,$(MUTANTS_BUILDER),$(eval $(call mutant_rule,$(s),$(s),$(F_ABS) $(MUTANTS_OUT)/$(s))))
$(foreach sm,$(MUTANTS_MODES),$(eval $(call mutant_rule,$(subst :,-,$(sm)),$(word 1,$(subst :, ,$(sm))),$(word 2,$(subst :, ,$(sm))))))
MUTANT_STAMPS := $(addsuffix .ok,$(addprefix $(MUTANTS_DIR)/,$(MUTANTS_ALL) $(MUTANTS_MODE_ALL) $(MUTANTS_F) $(MUTANTS_BUILDER) $(subst :,-,$(MUTANTS_MODES))))
mutants: $(MUTANT_STAMPS)   ## every planted-fault script (scripts/mutant-*.sh), one stamp each; not part of the gate
mutants-list:   ## the names of the mutant stamps (`make build/mutants/NAME.ok` runs one)
	@printf '%s\n' $(MUTANT_STAMPS)
.PHONY: mutants mutants-list
