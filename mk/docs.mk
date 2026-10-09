# Documentation checks use Python 3 (the audit's fallback) and the current F.
# All inputs that can change the generated census, linked paths or examples
# invalidate the full stamp. Quick checks are just README and tutorial examples.
DOC_MARKDOWN := README.md ROADMAP.md CHANGELOG.md CONTRIBUTING.md SECURITY.md $(shell find docs spec lib editors examples website -name '*.md' -type f 2>/dev/null | LC_ALL=C sort)
DOC_TOOLS := scripts/doc-examples.py scripts/doc-reference.py compiler/tests/docs/check.py
DOC_INPUTS := $(DOC_MARKDOWN) $(DOC_TOOLS) $(COMPILER_SRC) $(RT_LIR) $(shell find examples -type f | LC_ALL=C sort)
$(BUILD)/docs.ok: $(F) $(DOC_INPUTS) | $(BUILD)/
	$(call stamp,python3 compiler/tests/docs/check.py --fibc $(F_ABS) && python3 scripts/doc-examples.py --fibc $(F_ABS) --jobs $(CASE_JOBS))
$(BUILD)/docs-quick.ok: $(F) README.md $(wildcard docs/tutorial/*.md) $(DOC_TOOLS) | $(BUILD)/
	$(call stamp,python3 scripts/doc-examples.py --fibc $(F_ABS) --quick --jobs $(CASE_JOBS))
doc-examples: $(BUILD)/docs.ok   ## documentation examples, example-file smoke checks, local links and reference drift
doc-reference: $(F)   ## regenerate CLI, targets, builtins, environment and library facade references
	python3 scripts/doc-reference.py --fibc $(F_ABS)
DOCS_PYTHON ?= python3
docs-site:   ## build the GitHub Pages site and validate its generated links (website/requirements.txt)
	$(DOCS_PYTHON) website/tests.py
	$(DOCS_PYTHON) website/prepare.py
	$(DOCS_PYTHON) -m mkdocs build --strict
docs-serve:   ## preview the documentation site at http://127.0.0.1:8000/fibber/
	$(DOCS_PYTHON) website/prepare.py
	$(DOCS_PYTHON) -m mkdocs serve
.PHONY: doc-examples doc-reference docs-site docs-serve
