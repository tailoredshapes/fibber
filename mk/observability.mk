# Compiled fixtures and test results are files. The full specs stage already
# covers the unit specs; full gates add only the two external integration results.
OBS_DIR := $(BUILD)/observability
OBS_FIBC ?= $(F)
OBS_LIB := $(filter lib/%,$(COMPILER_SRC))
OBS_UNIT_NAMES := otel-http otel-otlp-json otel-otlp-http otel-otlp-log
OBS_UNIT_RESULTS := $(addprefix $(OBS_DIR)/,$(addsuffix -spec.ok,$(OBS_UNIT_NAMES)))
OBSERVABILITY_INTEGRATION_STAMPS := $(OBS_DIR)/otlp-http.ok $(OBS_DIR)/collector.ok
OBS_COLLECTOR_IMAGE := $(strip $(file <scripts/otel-collector-image))
ifneq ($(filter gate gate-stamps gate-report release observability observability-integration $(OBS_DIR)/collector.ok,$(MAKECMDGOALS)),)
OBS_COLLECTOR_ID := $(shell timeout 5 docker image inspect '$(OBS_COLLECTOR_IMAGE)' --format '{{.Id}}' 2>/dev/null)
$(call cfg_write,$(OBS_DIR)/collector.cfg,$(OBS_COLLECTOR_IMAGE) $(or $(OBS_COLLECTOR_ID),missing))
endif
$(call cfg_write,$(OBS_DIR)/compiler.cfg,$(or $(realpath $(OBS_FIBC)),$(abspath $(OBS_FIBC))) $(PLATFORM) -O2 -Ilib -Ispecs $(FIB_TARGET_CPU) $(FIB_TARGET_TRIPLE) $(FIB_LIR_OWN) $(FIB_LIR_CHECKS))
OBS_BUILD_INPUTS := $(OBS_DIR)/compiler.cfg $(MK)/observability.mk $(MK)/config.mk Makefile

$(OBS_DIR)/otel-otlp-json-spec: specs/otel-otlp-fixtures.fib
$(OBS_DIR)/otel-otlp-http-spec: specs/otel-exporter-contract.fib
$(OBS_DIR)/otel-otlp-log-spec: specs/otel-otlp-fixtures.fib
$(OBS_DIR)/%-spec: specs/%-spec.fib $(OBS_FIBC) $(OBS_LIB) $(OBS_BUILD_INPUTS) | $(OBS_DIR)/
	rm -f $@.ok
	$(OBS_FIBC) build specs/$*-spec.fib -O 2 -I lib -I specs -o $@.new > $@.build.log 2>&1 && mv $@.new $@

$(OBS_DIR)/%-spec.ok: $(OBS_DIR)/%-spec scripts/tests/otlp/check-report.py
	$(call stamp,timeout 300 $< --seed 1 --format json > $@.report.json && python3 scripts/tests/otlp/check-report.py $@.report.json)

$(OBS_DIR)/otlp-client: scripts/tests/otlp/client.fib specs/otel-exporter-contract.fib $(OBS_FIBC) $(OBS_LIB) $(OBS_BUILD_INPUTS) | $(OBS_DIR)/
	rm -f $(OBS_DIR)/otlp-http.ok
	$(OBS_FIBC) build $< -O 2 -I lib -I specs -o $@.new > $@.build.log 2>&1 && mv $@.new $@

$(OBS_DIR)/otlp-http.ok: $(OBS_DIR)/otlp-client scripts/tests/otlp/integration.py
	$(call stamp,python3 scripts/tests/otlp/integration.py --client $(abspath $<))

$(OBS_DIR)/collector-fixture: specs/otel-collector.fib $(OBS_FIBC) $(OBS_LIB) $(OBS_BUILD_INPUTS) | $(OBS_DIR)/
	rm -f $(OBS_DIR)/collector.ok
	$(OBS_FIBC) build $< -O 2 -I lib -o $@.new > $@.build.log 2>&1 && mv $@.new $@

$(OBS_DIR)/collector.ok: $(OBS_DIR)/collector-fixture scripts/test-otel-collector.py scripts/otel-collector-image specs/fixtures/otel-collector/config.yaml $(OBS_DIR)/collector.cfg
ifneq ($(OBS_COLLECTOR_ID),)
	$(call stamp,python3 scripts/test-otel-collector.py --binary $(abspath $<) --output $(abspath $(OBS_DIR))/collector-runs/$$(date +%s)-$$$$)
else
	$(call skip,pinned OpenTelemetry Collector image unavailable; pull $(OBS_COLLECTOR_IMAGE) and rerun)
endif

observability: $(OBS_UNIT_RESULTS) $(OBSERVABILITY_INTEGRATION_STAMPS)   ## focused tracing/OTLP specs and real HTTP/collector integration
observability-integration: $(OBSERVABILITY_INTEGRATION_STAMPS)   ## actual TCP receiver and pinned official collector checks
.PHONY: observability observability-integration
