# mk/release.mk: the release tarball (the steps of the old scripts/package.sh as targets), dist/fibc-VERSION-PLATFORM.tar.gz and SHA256SUMS.
#   make release                 the tarball for this platform (linux-x86_64 or darwin-arm64), LLVM linked statically (RELEASE_LLVM_LINK=shared is for trying only)
#   make seed-bump TAG=v0.1.13   rewrite SEED from a published release's SHA256SUMS (and verify the linux tarball against it)
# Everything built here targets a baseline CPU (x86-64-v3, apple-m1: docs/adr/0008), so the tarball runs on machines other than the builder's.
RELEASE_DIR := $(BUILD)/release
RELEASE_NAME := fibc-$(VERSION)-$(FIB_PLATFORM)
RELEASE_TREE := $(RELEASE_DIR)/$(RELEASE_NAME)
RELEASE_LLVM_LINK ?= static
ifeq ($(RELEASE_LLVM_LINK),static)
  RELEASE_LLVM_ARGS = $$(cat $(LLVM_STATIC_ARGS))
  RELEASE_LLVM_DEP := $(LLVM_STATIC_ARGS)
else
  RELEASE_LLVM_ARGS := -L $(LLVM_LIBDIR) -l LLVM-21
  RELEASE_LLVM_DEP :=
endif
ifeq ($(FIB_PLATFORM),darwin-arm64)
  RELEASE_CPU ?= apple-m1
else
  RELEASE_CPU ?= x86-64-v3
endif
release_build = FIB_TARGET_CPU=$(RELEASE_CPU) $(1) build compiler/fibc.fib $(BUILD_ARGS) $(RELEASE_LLVM_ARGS) -o $(2)
release_emit = FIB_TARGET_CPU=$(RELEASE_CPU) $(1) emit $(BUILD_ARGS) compiler/fibc.fib

$(RELEASE_DIR)/F: $(COMPILER_SRC) $(BUILDER) $(RELEASE_LLVM_DEP) $(BUILD)/version.ok | $(RELEASE_DIR)/
	$(call release_build,$(BUILDER),$@.new) && mv $@.new $@
$(RELEASE_DIR)/emit.seed: $(COMPILER_SRC) $(BUILDER) | $(RELEASE_DIR)/
	$(call release_emit,$(BUILDER)) > $@.new && mv $@.new $@
$(RELEASE_DIR)/emit.F: $(RELEASE_DIR)/F
	$(call release_emit,$<) > $@.new && mv $@.new $@
$(RELEASE_DIR)/F3: $(RELEASE_DIR)/F $(RELEASE_LLVM_DEP)
	$(call release_build,$<,$@.new) && mv $@.new $@
$(RELEASE_DIR)/emit.F3: $(RELEASE_DIR)/F3
	$(call release_emit,$<) > $@.new && mv $@.new $@
# The stage check: F3 emits what F emits (the seed's emit is a note: its embedded prelude may lag).
$(RELEASE_DIR)/stage.ok: $(RELEASE_DIR)/emit.seed $(RELEASE_DIR)/emit.F $(RELEASE_DIR)/emit.F3
	$(call stamp,{ cmp -s $(RELEASE_DIR)/emit.seed $(RELEASE_DIR)/emit.F && echo "note: F emits the seed's lIR" || echo "note: F emits different lIR from the seed (its embedded prelude lags this tree's)"; } \
	  && cmp $(RELEASE_DIR)/emit.F $(RELEASE_DIR)/emit.F3 && echo "stage check: F (built by the seed) and F3 (built by F) emit the same lIR ($$(wc -c < $(RELEASE_DIR)/emit.F) bytes)")

# The cc shim drops the rpath `fibc build` writes for each -L directory (the LLVM archives' scratch directory): the shipped binary has none.
define SHIM_CC
#!/bin/sh
# Drops `-Xlinker -rpath -Xlinker DIR` (mk/release.mk): the shipped fibc needs no library of its own, LLVM is inside it.
n=$$#
while [ "$$n" -gt 0 ]; do
  a=$$1; shift; n=$$((n - 1))
  if [ "$$a" = -Xlinker ] && [ "$$n" -ge 3 ] && [ "$$1" = -rpath ]; then shift; shift; shift; n=$$((n - 3)); continue; fi
  set -- "$$@" "$$a"
done
exec "$$REAL_CC" "$$@"
endef
$(RELEASE_DIR)/shim/cc: mk/release.mk | $(RELEASE_DIR)/shim/
	$(file >$@,$(SHIM_CC))chmod +x $@
$(RELEASE_TREE)/bin/fibc: $(RELEASE_DIR)/F3 $(RELEASE_DIR)/stage.ok $(RELEASE_DIR)/shim/cc $(RELEASE_LLVM_DEP)
	mkdir -p $(@D) && REAL_CC=$$(command -v cc) PATH="$(abspath $(RELEASE_DIR)/shim):$$PATH" $(call release_build,$(RELEASE_DIR)/F3,$@.new) && mv $@.new $@
# The checks of the shipped binary (scripts/lib/check-shipped.sh: no rpath, no libLLVM, only the system libraries, no AVX-512 outside BLAKE3, the
# start-up CPU check in it, and it emits what F emits).
$(RELEASE_DIR)/binary.ok: $(RELEASE_TREE)/bin/fibc $(RELEASE_DIR)/emit.F scripts/lib/check-shipped.sh
	$(call stamp,scripts/lib/check-shipped.sh $(RELEASE_TREE)/bin/fibc $(FIB_PLATFORM) $(RELEASE_CPU) $(RELEASE_LLVM_LINK) \
	  && FIB_TARGET_CPU=$(RELEASE_CPU) $(RELEASE_TREE)/bin/fibc emit $(BUILD_ARGS) compiler/fibc.fib > $(RELEASE_DIR)/emit.ship && cmp $(RELEASE_DIR)/emit.F $(RELEASE_DIR)/emit.ship)

define RELEASE_README
fibc $(VERSION): the fibber compiler, written in fibber (stage 2).

Layout
  bin/fibc                 the compiler
  share/fibber/lib/        the standard library source, found beside bin/ by fibc itself

Use it where it is unpacked; no environment variable is needed:
  bin/fibc --version
  bin/fibc run hello.fib
  bin/fibc build hello.fib -o hello      (needs a C compiler, `cc`, to link)
Move the whole directory as one: bin/ and share/ stay side by side.
FIB_LIB and -I add module roots in front of the library as usual. `fibc help` lists the commands.

Needs only the system libraries (on Linux libc, libm, libstdc++, libgcc_s, libz and libzstd; on macOS the ones under /usr/lib); the code generator, LLVM, is inside bin/fibc,
so it does not need LLVM installed.
Licence: BSD 3-Clause, see LICENSE.
endef
LIB_FILES := $(shell find lib -type f | LC_ALL=C sort)
$(RELEASE_TREE)/README.txt: mk/release.mk VERSION | $(RELEASE_TREE)/
	$(file >$@,$(RELEASE_README))test -s $@
$(RELEASE_TREE)/LICENSE: LICENSE
	mkdir -p $(@D) && cp $< $@
$(RELEASE_DIR)/lib.ok: $(LIB_FILES)
	rm -rf $(RELEASE_TREE)/share/fibber/lib && mkdir -p $(RELEASE_TREE)/share/fibber && cp -r lib $(RELEASE_TREE)/share/fibber/lib && touch $@
# WITH_MUSL=1 adds share/fibber/musl/ARCH/ (docs/design/static-linking.md 6), the pieces of `fibc build --static`; MUSL_TARBALL names a local musl-1.2.5.tar.gz.
$(RELEASE_DIR)/musl.ok: scripts/build-musl.sh rt/static/cpuid.c
ifeq ($(WITH_MUSL)-$(FIB_PLATFORM),1-linux-x86_64)
	scripts/build-musl.sh x86_64 $(RELEASE_TREE)/share/fibber/musl/x86_64 $(MUSL_TARBALL)
	if command -v aarch64-linux-gnu-gcc > /dev/null 2>&1; then scripts/build-musl.sh aarch64 $(RELEASE_TREE)/share/fibber/musl/aarch64 $(MUSL_TARBALL); fi
	touch $@
else
	rm -rf $(RELEASE_TREE)/share/fibber/musl && touch $@
endif
# The unpacked tree, in an empty environment, runs --version and builds and runs hello.
$(RELEASE_DIR)/tree.ok: $(RELEASE_DIR)/binary.ok $(RELEASE_TREE)/README.txt $(RELEASE_TREE)/LICENSE $(RELEASE_DIR)/lib.ok $(RELEASE_DIR)/musl.ok
	$(call stamp,env -i PATH=/usr/bin:/bin $(abspath $(RELEASE_TREE))/bin/fibc --version \
	  && echo '(defun main () -> i64 (do (println "hello from fibber") 0))' > $(RELEASE_DIR)/hello.fib \
	  && cd $(RELEASE_DIR) && env -i PATH=/usr/bin:/bin $(abspath $(RELEASE_TREE))/bin/fibc build hello.fib -o hello && [ "$$(env -i ./hello)" = "hello from fibber" ] \
	  && echo "the shipped fibc builds hello and it runs")
$(DIST)/$(RELEASE_NAME).tar.gz: $(RELEASE_DIR)/tree.ok | $(DIST)/
	rm -f $@; v=$$(tar --version 2> /dev/null || true); if grep -q 'GNU tar' <<< "$$v"; then tar -C $(RELEASE_DIR) --owner=0 --group=0 --sort=name -czf $@ $(RELEASE_NAME); \
	else tar -C $(RELEASE_DIR) --uid 0 --gid 0 -czf $@ $(RELEASE_NAME); fi
$(DIST)/SHA256SUMS: $(DIST)/$(RELEASE_NAME).tar.gz
	cd $(DIST) && { sha256sum $(RELEASE_NAME).tar.gz 2> /dev/null || shasum -a 256 $(RELEASE_NAME).tar.gz; } > SHA256SUMS && cat SHA256SUMS
release: $(DIST)/SHA256SUMS   ## the release tarball dist/fibc-VERSION-PLATFORM.tar.gz and its SHA256SUMS (static LLVM, the shipped binary's checks)
	@ls -l $(DIST)/$(RELEASE_NAME).tar.gz

# seed-bump: SEED from a published release (TAG=vX.Y.Z): its SHA256SUMS, one url and sha256 per platform the release has; the linux tarball is
# downloaded and checked against the sums before SEED is written (a published sum that does not match its tarball is not a seed).
seed-bump:   ## rewrite SEED from the release TAG=vX.Y.Z on GitHub (its SHA256SUMS; the linux tarball verified)
	@test -n "$(TAG)" || { echo "make seed-bump TAG=v0.1.13"; exit 2; }
	rm -rf $(BUILD)/seed-bump && mkdir -p $(BUILD)/seed-bump
	gh release download $(TAG) -R tailoredshapes/fibber -p SHA256SUMS -p 'fibc-*-linux-x86_64.tar.gz' -D $(BUILD)/seed-bump
	cd $(BUILD)/seed-bump && grep linux-x86_64 SHA256SUMS | sha256sum -c -
	v=$(TAG:v%=%); { echo "url=https://github.com/tailoredshapes/fibber/releases/download/$(TAG)/fibc-$$v-linux-x86_64.tar.gz"; \
	  echo "sha256=$$(awk '/linux-x86_64/ { print $$1 }' $(BUILD)/seed-bump/SHA256SUMS)"; \
	  echo "# One url and one sha256 per platform: url.PLATFORM= and sha256.PLATFORM= (scripts/fetch-seed.sh). The lines above are linux-x86_64's."; \
	  awk -v t="$(TAG)" -v v="$$v" '$$2 ~ /darwin-arm64/ { print "url.darwin-arm64=https://github.com/tailoredshapes/fibber/releases/download/" t "/fibc-" v "-darwin-arm64.tar.gz"; print "sha256.darwin-arm64=" $$1 }' $(BUILD)/seed-bump/SHA256SUMS; } > SEED
	cat SEED
.PHONY: release seed-bump
