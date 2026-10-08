# mk/seed.mk: the seed compiler named by SEED (one url and one sha256 per platform; the bare lines are linux-x86_64's), fetched by
# scripts/fetch-seed.sh (the download, the sha256 check, the unpacking, the tree's lib/ in place of the bundled one) into
# build/seed/<sha256>/: a changed SEED is a new directory, a wrong sha256 is a failure that names it. Downloads are cached across
# worktrees in FIB_SEED_CACHE (DIR/<sha256>/seed.tar.gz).
SEED_URL := $(shell sed -n 's/^url\.$(FIB_PLATFORM)=//p' SEED)
SEED_SHA := $(shell sed -n 's/^sha256\.$(FIB_PLATFORM)=//p' SEED)
ifeq ($(SEED_URL),)
  ifeq ($(FIB_PLATFORM),linux-x86_64)
    SEED_URL := $(shell sed -n 's/^url=//p' SEED)
    SEED_SHA := $(shell sed -n 's/^sha256=//p' SEED)
  endif
endif
ifeq ($(SEED_SHA),)
  SEED_SHA := no-seed-for-$(FIB_PLATFORM)
endif
SEED_DIR := $(BUILD)/seed/$(SEED_SHA)
SEED_FIBC := $(SEED_DIR)/bin/fibc
export FIB_SEED_CACHE ?= $(HOME)/.cache/fibber-scratch/seeds

# the directory is named by the sha256 in SEED, so SEED itself is no prerequisite: a touched SEED with the same sha256 changes nothing
$(SEED_FIBC): | scripts/fetch-seed.sh
	rm -rf $(SEED_DIR); mkdir -p $(SEED_DIR)/unpack
	FIB_PLATFORM=$(FIB_PLATFORM) scripts/fetch-seed.sh $(SEED_DIR)/unpack > /dev/null
	mv $(SEED_DIR)/unpack/fibc-*/* $(SEED_DIR)/ && rm -rf $(SEED_DIR)/unpack
	touch $@ && test -x $@ && $@ --version   # the tarball keeps the release-time mtime: newer than SEED only once touched

seed: $(SEED_FIBC)   ## fetch and verify the seed compiler that SEED names (build/seed/<sha256>/bin/fibc)
.PHONY: seed
print-seed: $(SEED_FIBC)   ## print the path of the seed compiler (for a script that wants FIBC)
	@echo $(abspath $(SEED_FIBC))
.PHONY: print-seed
