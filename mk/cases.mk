# mk/cases.mk: the case directories under F (cases/ownership, cases/modules, cases/stdlib), as shard tables build/cases/<dir>.<k>.txt that
# Make schedules, then the comparison with scripts/ci-stage2.expected and the floor of scripts/case-floor.expected (scripts/ci-stage2.sh --from).
# A shard is `F cases DIR --only NAMES -j CASE_JOBS` under a time limit: its table is the target; it is a failure of the shard only when the
# run timed out or printed no case count (a case that fails is the comparison's business). Shard k of a directory takes every K-th case name,
# in the byte order of `ls` (a *.fib file, or a directory holding main.fib), as scripts/ci-stage2.sh dealt them.
CASES_DIR := $(BUILD)/cases
case_names = $(shell cd cases/$(1) && for e in $$(LC_ALL=C ls); do if [ -f "$$e" ]; then [[ $$e == *.fib ]] && echo "$$e" || true; elif [ -f "$$e/main.fib" ]; then echo "$$e"; fi; done)
STDLIB_NAMES := $(call case_names,stdlib)
OWNERSHIP_NAMES := $(call case_names,ownership)
MODULES_NAMES := $(call case_names,modules)
# The quick gate's stdlib sample: every 10th case by name, and every expected failure, so the harness is seen reporting one.
SAMPLE_STEP := $(shell n=$(words $(STDLIB_NAMES)); echo $$(( n / 100 > 0 ? n / 100 : 1 )))
SAMPLE_NAMES := $(shell { printf '%s\n' $(STDLIB_NAMES) | awk -v k=$(SAMPLE_STEP) 'NR % k == 0'; sed -n 's|^cases/stdlib/\([^ ]*\) .*|\1|p' scripts/ci-stage2.expected | sed 's|/main.fib$$||'; } | LC_ALL=C sort -u)
# the k-th of K shards of a name list (0-based, round robin)
shard_of = $(shell printf '%s\n' $(1) | awk -v k=$(2) -v i=$(3) '(NR - 1) % k == i')
# the files a list of case names stands for: the .fib, or everything under the case's directory
case_files = $(foreach n,$(2),$(if $(filter %.fib,$(n)),cases/$(1)/$(n),$(wildcard cases/$(1)/$(n)/*)))
SUPPORT_FILES := $(shell find cases/stdlib/support cases/ownership/support cases/modules/support -type f 2> /dev/null)
SHARD_IDX = $(shell seq 0 $$(( $(1) - 1 )))

# $(call run_shard,DIR,LIMIT,ONLY..): the recipe of one shard table
define run_shard
start=$$(date +%s); code=0; timeout $(2) $(F) cases cases/$(1) $(3) -j $(CASE_JOBS) > $@.tmp 2> $(@:%.txt=%.err) || code=$$?; \
echo $$code > $(@:%.txt=%.code); echo $$(( $$(date +%s) - start )) > $(@:%.txt=%.secs); \
if [ $$code -eq 124 ]; then echo "cases/$(1) shard timed out after $(2) s"; exit 1; fi; \
grep -q '^[0-9][0-9]* cases:' $@.tmp || { echo "cases/$(1) shard printed no case count (exit $$code)"; tail -n 20 $(@:%.txt=%.err); exit 1; }; mv $@.tmp $@
endef
# $(call shard_rule,TABLE-NAME,DIR,K,k,LIMIT,NAMES): shard k of K over NAMES, the table build/cases/TABLE-NAME.k.txt (its names in NAMES_TABLE-NAME.k);
# a directory run in one shard (K = 1) is run without --only, as CI always ran it.
define shard_rule
NAMES_$(1).$(4) := $(call shard_of,$(6),$(3),$(4))
$(CASES_DIR)/$(1).$(4).txt: $(F) $$(call case_files,$(2),$$(NAMES_$(1).$(4))) $(SUPPORT_FILES) | $(CASES_DIR)/ $(BUILD)/tmp/
	$$(call run_shard,$(2),$(5),$(if $(or $(7),$(filter-out 1,$(3))),--only $$(NAMES_$(1).$(4)),))
endef
$(foreach k,$(call SHARD_IDX,$(SHARDS)),$(eval $(call shard_rule,stdlib,stdlib,$(SHARDS),$(k),$(LIMIT_STDLIB),$(STDLIB_NAMES))))
$(foreach k,$(call SHARD_IDX,$(OWN_SHARDS)),$(eval $(call shard_rule,ownership,ownership,$(OWN_SHARDS),$(k),$(LIMIT_OWNERSHIP),$(OWNERSHIP_NAMES))))
$(eval $(call shard_rule,modules,modules,1,0,$(LIMIT_MODULES),$(MODULES_NAMES)))
# the quick gate's sample: its own shards over the sample names, always with --only
$(foreach k,$(call SHARD_IDX,$(QUICK_SHARDS)),$(eval $(call shard_rule,sample,stdlib,$(QUICK_SHARDS),$(k),$(LIMIT_STDLIB),$(SAMPLE_NAMES),only)))

STDLIB_TABLES := $(foreach k,$(call SHARD_IDX,$(SHARDS)),$(CASES_DIR)/stdlib.$(k).txt)
OWNERSHIP_TABLES := $(foreach k,$(call SHARD_IDX,$(OWN_SHARDS)),$(CASES_DIR)/ownership.$(k).txt)
MODULES_TABLES := $(CASES_DIR)/modules.0.txt
SAMPLE_TABLES := $(foreach k,$(call SHARD_IDX,$(QUICK_SHARDS)),$(CASES_DIR)/sample.$(k).txt)
CASES_DATA := scripts/ci-stage2.expected scripts/case-floor.expected scripts/ci-stage2.sh

$(CASES_DIR)/full.ok: $(OWNERSHIP_TABLES) $(MODULES_TABLES) $(STDLIB_TABLES) $(CASES_DATA)
	$(call stamp,scripts/ci-stage2.sh --from $(CASES_DIR) ownership:ownership:$(OWN_SHARDS) modules:modules:1 stdlib:stdlib:$(SHARDS))
$(CASES_DIR)/quick.ok: $(OWNERSHIP_TABLES) $(MODULES_TABLES) $(SAMPLE_TABLES) $(CASES_DATA)
	$(call stamp,scripts/ci-stage2.sh --from $(CASES_DIR) ownership:ownership:$(OWN_SHARDS) modules:modules:1 sample:stdlib:$(QUICK_SHARDS))
cases: $(CASES_DIR)/full.ok   ## every case directory under F, in shards, compared with scripts/ci-stage2.expected and the floor
cases-quick: $(CASES_DIR)/quick.ok   ## ownership, modules and the stdlib sample of the quick gate
.PHONY: cases cases-quick
