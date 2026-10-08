# mk/bench.mk: the benchmarks. `make bench` runs scripts/bench/quick.sh with this tree's F against scripts/bench/baseline.tsv (a delta over 10%
# is flagged, a changed checksum is a failure: docs/adr/0019) and keeps the rows in build/bench/quick.tsv; `make bench-record` rewrites
# the baseline (on a quiet machine, from a tree whose answers are right). quick.sh takes /tmp/fibsuite.lock itself, so a benchmark never
# overlaps a gate.
BENCH_DIR := $(BUILD)/bench
BENCH_SRC := $(wildcard scripts/bench/*.fib)
$(BENCH_DIR)/quick.tsv: $(F) scripts/bench/quick.sh scripts/bench/baseline.tsv $(BENCH_SRC) | $(BENCH_DIR)/
	rm -f $@; BENCH_FIBC=$(F_ABS) GATE_OUT=$(abspath $(BENCH_DIR)) scripts/bench/quick.sh $(BENCH) | tee $(BENCH_DIR)/quick.log; cp $(BENCH_DIR)/bench/rows.tsv $@
bench: $(BENCH_DIR)/quick.tsv   ## the quick benchmarks against scripts/bench/baseline.tsv (BENCH="name.." for some); build/bench/quick.tsv
bench-record: $(F)   ## rewrite scripts/bench/baseline.tsv from this tree's F
	BENCH_FIBC=$(F_ABS) GATE_OUT=$(abspath $(BENCH_DIR)) scripts/bench/quick.sh --record
.PHONY: bench bench-record
