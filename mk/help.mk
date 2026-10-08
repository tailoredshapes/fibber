# mk/help.mk: `make help` (every rule whose line carries a `## ` comment), clean, distclean (mac-check is mk/mac.mk).
help:   ## this list: the public targets and what they make
	@echo "fibber: GNU Make $(MAKE_VERSION) coordinates the build (docs/design/build.md). Targets are files under $(BUILD)/; -jN runs them side by side."
	@echo "  make -j8 gate | quick     the gates;   GATE_FRESH=1 rebuilds F from the seed;   BUILDER=/path/fibc builds with another compiler"
	@grep -h -E '^[a-zA-Z0-9_./$$()-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/^\$$\((BUILD|TOOLS_CACHE)\)/build/; s/:[^#]*## /\t/' | sort | awk -F'\t' '{ printf "  %-22s %s\n", $$1, $$2 }'
clean:   ## remove build/ (everything built: the seed stays in FIB_SEED_CACHE) and dist/
	rm -rf $(BUILD) $(DIST)
distclean: clean   ## clean, and the tool downloads under TOOLS_CACHE's wasm and lz4 (the seed cache stays)
	rm -rf $(TOOLS_CACHE)/wasm $(TOOLS_CACHE)/lz4
.PHONY: help clean distclean
