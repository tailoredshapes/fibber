# mk/config.mk: the settings every section shares (docs/design/build.md 3). Nothing here is a target but the directories.
ifeq ($(filter 4.% 5.%,$(MAKE_VERSION)),)
  $(error fibber needs GNU Make 4 or later (this is $(MAKE_VERSION)). macOS ships 3.81 as /usr/bin/make: `brew install make` and run `gmake` (README.md, Install))
endif
.DEFAULT_GOAL := help
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
.SECONDARY:
.SUFFIXES:
MAKEFLAGS += --no-builtin-rules --no-builtin-variables

ROOT := $(CURDIR)
BUILD ?= build
DIST ?= dist
ABS_BUILD := $(abspath $(BUILD))
VERSION := $(strip $(shell tr -d '[:space:]' < VERSION))
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)
# macOS: what the scripts expect from GNU coreutils comes from scripts/portable (timeout, flock, nproc, sha256sum, sed -i, date %N, a ulimit -v that succeeds)
ifeq ($(UNAME_S),Darwin)
  export PATH := $(CURDIR)/scripts/portable/bin:$(PATH)
  export BASH_ENV := $(CURDIR)/scripts/portable/bash-env.sh
endif
PLATFORM := $(shell echo "$(UNAME_S)-$(UNAME_M)" | tr 'A-Z' 'a-z' | sed 's/linux-amd64/linux-x86_64/; s/darwin-aarch64/darwin-arm64/')
FIB_PLATFORM ?= $(PLATFORM)

# How stage 2 links LLVM 21: shared (the default of a gate) or static (the release; mk/release.mk uses static whatever this says).
LLVM_LINK ?= shared
ifeq ($(UNAME_S),Darwin)
  LLVM_LIBDIR ?= /opt/homebrew/opt/llvm@21/lib
else
  LLVM_LIBDIR ?= /usr/lib/llvm-21/lib
endif
LLVM_STATIC_DIR := $(BUILD)/llvm-static
LLVM_STATIC_ARGS := $(LLVM_STATIC_DIR)/args
ifeq ($(LLVM_LINK),static)
  LLVM_ARGS = $$(cat $(LLVM_STATIC_ARGS))
  LLVM_DEP := $(LLVM_STATIC_ARGS)
else ifeq ($(LLVM_LINK),shared)
  LLVM_ARGS := -L $(LLVM_LIBDIR) -l LLVM-21
  LLVM_DEP :=
else
  $(error LLVM_LINK is shared or static, not $(LLVM_LINK))
endif

# Parallelism inside a recipe (docs/design/build.md 3): `fibc cases -j CASE_JOBS` overlaps the runs of one shard; SHARDS stdlib shards,
# OWN_SHARDS ownership shards, QUICK_SHARDS shards of the quick gate's stdlib sample. Make's -j is what runs recipes side by side.
CASE_JOBS ?= 4
SHARDS ?= 16
OWN_SHARDS ?= 4
QUICK_SHARDS ?= 4
TOOLS_TIMEOUT ?= 900
LIMIT_OWNERSHIP ?= 600
LIMIT_MODULES ?= 300
LIMIT_STDLIB ?= 1800

# The environment every fibc of the build sees: the tree's library, scratch under build/, no LD_LIBRARY_PATH (a stage 2 links LLVM itself).
export FIB_LIB := $(ROOT)/lib
# TMPDIR is outside the tree on purpose: with TMPDIR inside the worktree, cases/stdlib/8283 fails in a shard (expected 0, got 4294967296) and passes
# with TMPDIR elsewhere (found in MAKE-1; reported, not explained). SCRATCH holds it, and the tool scripts' scratch.
SCRATCH ?= $(HOME)/.cache/fibber-scratch/mk-$(shell echo $(ROOT) | { md5sum 2> /dev/null || md5 -r; } | cut -c1-8)
export TMPDIR := $(SCRATCH)/tmp
$(shell mkdir -p $(TMPDIR))
unexport LD_LIBRARY_PATH
unexport GATE_SLOTS

# The stage 2 of this tree and the fibc that builds it (the seed by default: CI's truth; BUILDER=/path/to/fibc or FIBC=... names another).
F := $(BUILD)/F
F_ABS := $(ABS_BUILD)/F
ifneq ($(origin FIBC),undefined)
  BUILDER ?= $(FIBC)
endif
BUILDER ?= $(SEED_FIBC)

# $(call stamp,CMD): runs CMD with its output in the stamp's .log; the stamp holds the elapsed seconds and exists only when CMD passed.
# The old stamp goes first (a stamp a failed recipe did not change would otherwise survive .DELETE_ON_ERROR). A comma in CMD: write $(,).
, := ,
define stamp
rm -f $@; t0=$$(date +%s); if { $(1); } > $(@:%.ok=%.log) 2>&1; then echo "$$(( $$(date +%s) - t0 ))" > $@; else echo "FAIL $@ (log: $(@:%.ok=%.log))"; tail -n 20 $(@:%.ok=%.log); exit 1; fi
endef
# $(call cfg_write,FILE,TEXT): at parse time, FILE holds TEXT; its mtime moves only when the content changes. A stamp or a table that depends on
# FILE therefore reruns when the configuration (or a shard's membership) changes, and not otherwise; under -n and -q the file is the same.
cfg_write = $(shell mkdir -p $(dir $(1)); echo '$(2)' | cmp -s - $(1) || echo '$(2)' > $(1))
# $(call skip,REASON): the stamp says the stage was skipped, with the reason (a toolchain that is not there is no failure of the tree).
define skip
rm -f $@; echo "SKIPPED: $(1)" > $@; echo "$@: SKIPPED: $(1)"
endef

# a directory, as an order-only prerequisite (`| $(BUILD)/x/`): made when it is not there
%/:
	mkdir -p $@
FORCE:
.PHONY: FORCE
