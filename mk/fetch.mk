# mk/fetch.mk: the checksummed downloads (docs/adr/0020) as targets under TOOLS_CACHE (default ~/.cache/fibber-scratch/tools): each recipe is the
# fetch script that records the sha256 it checks; a target is a file the fetch leaves, so `make fetch-tools` downloads nothing that is there.
# None of these is a dependency of the gate.
$(TOOLS_CACHE)/JSONTestSuite/test_parsing/.verified: scripts/fetch-json-testsuite.sh specs/json-testsuite.sha256
	scripts/fetch-json-testsuite.sh $(@D)/.. && touch $@
$(TOOLS_CACHE)/JSON-Schema-Test-Suite/.commit-$(shell sed -n 's/^COMMIT=//p' scripts/fetch-json-schema-suite.sh): scripts/fetch-json-schema-suite.sh
	scripts/fetch-json-schema-suite.sh $(@D)
$(TOOLS_CACHE)/crypto-vectors/.verified: scripts/fetch-crypto-vectors.sh specs/crypto-vectors.sha256
	scripts/fetch-crypto-vectors.sh $(@D) && touch $@
$(TOOLS_CACHE)/lz4/lz4bench: scripts/fetch-lz4-tools.sh
	scripts/fetch-lz4-tools.sh $(@D)
$(TOOLS_CACHE)/unicode-16.0.0/.verified: scripts/fetch-unicode.sh specs/unicode-16.0.0.sha256
	scripts/fetch-unicode.sh $(@D) && touch $@
$(TOOLS_CACHE)/wasm/wasi-sdk-34.0-x86_64-linux/bin/wasm-ld: scripts/fetch-wasm-tools.sh
	scripts/fetch-wasm-tools.sh $(TOOLS_CACHE)/wasm
FETCH_TARGETS := $(TOOLS_CACHE)/JSONTestSuite/test_parsing/.verified $(TOOLS_CACHE)/JSON-Schema-Test-Suite/.commit-$(shell sed -n 's/^COMMIT=//p' scripts/fetch-json-schema-suite.sh) \
  $(TOOLS_CACHE)/crypto-vectors/.verified $(TOOLS_CACHE)/lz4/lz4bench $(TOOLS_CACHE)/unicode-16.0.0/.verified
fetch-tools: $(FETCH_TARGETS)   ## the test suites and oracles the offline checks were made from (JSONTestSuite, JSON Schema suite, crypto vectors, lz4, Unicode); sha256 checked
fetch-wasm: $(TOOLS_CACHE)/wasm/wasi-sdk-34.0-x86_64-linux/bin/wasm-ld   ## the wasi-sdk and wasmtime of the wasm stage (Linux x86-64; sha256 checked)
.PHONY: fetch-tools fetch-wasm
