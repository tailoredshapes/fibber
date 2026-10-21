# mk/tools.mk: the tool tests (the queue of the old scripts/tools.sh): one stamp build/tools/<name>.ok for each script, depending on F, the
# script and its directory (the inputs it reads, where knowable: a change under compiler/tests/lsp/ reruns the three lsp stamps and no other).
# A tool script holds one job of make's -j; what it runs inside is its own (docs/design/build.md 3). Each runs under TOOLS_TIMEOUT seconds
# with FIBC = F, its scratch under build/tools/out/<name>; its output is build/tools/<name>.log.
TOOLS_DIR := $(BUILD)/tools
TOOLS_OUT := $(SCRATCH)/tools
T := compiler/tests
FUZZ_SELFTEST_DEADLINE ?= 8000
tool_env := env FIBC=$(F_ABS) FUZZ_SELFTEST_DEADLINE=$(FUZZ_SELFTEST_DEADLINE) RESUME_OUT=$(TOOLS_OUT)/resume WINDOWS_SCRATCH=$(TOOLS_OUT)/windows

# name := command | inputs (a directory's files, or files)
tool_fibref-skeleton := $(T)/fibref/skeleton.sh $(TOOLS_OUT)/fibref-skeleton | $(wildcard $(T)/fibref/*) $(wildcard compiler/fibref/*.fib compiler/fibref/*/*.fib)
tool_gen-skeleton := $(T)/gen/skeleton.sh $(TOOLS_OUT)/gen-skeleton | $(wildcard $(T)/gen/*) $(wildcard compiler/gen/*.fib compiler/gen/*/*.fib)
tool_lint-pipefail := scripts/lint-pipefail.sh | $(shell find scripts $(T) .github -type f \( -name '*.sh' -o -name '*.yml' \)) Makefile $(wildcard mk/*.mk)
tool_lint-portable := scripts/lint-portable.sh | $(shell find scripts $(T) -type f \( -name '*.sh' -o -name '*.mk' \)) Makefile $(wildcard mk/*.mk)
tool_sh-driver-check-lib := $(T)/driver/check-lib.sh $(F_ABS) | $(T)/driver/check-lib.sh $(wildcard $(T)/driver/linklib/*)
tool_unit-api := $(T)/unit/run.sh $(TOOLS_OUT)/unit-api | $(wildcard $(T)/unit/*) $(T)/unit-groups.py $(T)/unit-report.py
# units-all: every unit module (units.run) as ONE executable at -O 1; units-rest: the ordinary control, a group built as before at -O 2 (docs/design/test-pyramid.md).
# One group per directory (UNITS_DIR=emit|own|types|rest) still works by hand.
tool_units-all := env UNITS_DIR=run UNITS_MERGE=1 UNITS_OPT=1 $(T)/units.sh $(TOOLS_OUT)/units-all | $(T)/units.sh $(T)/unit-groups.py $(T)/unit-report.py $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/*/unit-*.fib) $(wildcard $(T)/types/*.fib) $(wildcard $(T)/gen/*.fib)
tool_units-rest := env UNITS_DIR=rest $(T)/units.sh $(TOOLS_OUT)/units-rest | $(T)/units.sh $(T)/unit-groups.py $(T)/unit-report.py $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/*/unit-*.fib) $(wildcard $(T)/gen/*.fib)
tool_units-pending := env UNITS_DIR=pending $(T)/units.sh $(TOOLS_OUT)/units-pending | $(T)/units.sh $(T)/unit-groups.py $(T)/unit-report.py $(T)/units.run $(T)/units.pending $(T)/units.elsewhere $(wildcard $(T)/*/unit-*.fib)
tool_shootout-compile := $(T)/shootout-compile.sh $(TOOLS_OUT)/shootout-compile | $(T)/shootout-compile.sh $(wildcard docs/shootout/parallel/*.fib)
tool_sh-driver-cli := $(T)/driver/cli.sh $(F_ABS) | $(T)/driver/cli.sh
tool_sh-driver-demand := $(T)/driver/demand.sh $(F_ABS) | $(T)/driver/demand.sh
tool_sh-driver-muladd := $(T)/driver/muladd.sh $(F_ABS) | $(T)/driver/muladd.sh
tool_sh-driver-no-fma := $(T)/driver/no-fma.sh $(F_ABS) | $(T)/driver/no-fma.sh $(T)/driver/no-fma-faults.sh
tool_sh-driver-cpu-check := $(T)/driver/cpu-check.sh $(F_ABS) | $(T)/driver/cpu-check.sh
tool_sh-driver-target := $(T)/driver/target.sh $(F_ABS) | $(T)/driver/target.sh
tool_sh-driver-test-cmd := $(T)/driver/test-cmd.sh $(F_ABS) | $(T)/driver/test-cmd.sh $(T)/driver/test-project-root.sh
tool_sh-driver-static-host := $(T)/driver/static-host.sh $(F_ABS) | $(T)/driver/static-host.sh
tool_sh-emit-runtime := $(T)/emit/runtime.sh $(F_ABS) $(TOOLS_OUT)/runtime | $(T)/emit/runtime.sh $(T)/emit/gen-runtime.fib $(T)/emit/unit-runtime.fib $(RT_LIR)
tool_sh-emit-defs-order := $(T)/emit/defs-order.sh $(F_ABS) | $(T)/emit/defs-order.sh
tool_sh-emit-prune := $(T)/emit/prune.sh $(F_ABS) $(TOOLS_OUT)/prune | $(T)/emit/prune.sh $(wildcard compiler/emit/prune.fib)
tool_sh-stack-stack := $(T)/stack/stack.sh $(F_ABS) | $(wildcard $(T)/stack/*)
tool_sh-own-peek := $(T)/own/peek.sh $(F_ABS) | $(T)/own/peek.sh $(T)/emit/inspect.fib $(wildcard $(T)/own/peek*)
tool_sh-specs-plant-vec := $(T)/specs/plant-vec.sh $(F_ABS) | $(wildcard $(T)/specs/*)
tool_sh-lanes-lanes := $(T)/lanes/lanes.sh $(F_ABS) | $(wildcard $(T)/lanes/*)
tool_sh-repl-run := $(T)/repl/run.sh $(F_ABS) | $(wildcard $(T)/repl/*)
tool_sh-serve-run := $(T)/serve/run.sh $(F_ABS) | $(wildcard $(T)/serve/*)
# live mode (docs/design/live-repl.md): LV0's stubs and slots (hello's golden, the deep recursion, the case tables with and without --live), and the
# resident-session prototype's checks (compiler/live, built as a tool that links LLVM: about 3 minutes)
# The stubs test is three stamps (stubs.sh --part): the golden and the deep recursion, then each table (three runs of a case directory, plain,
# catching and live): whole, it was one stamp of about 1000 s at CASE_JOBS=1 on a 4-vCPU hosted runner, over TOOLS_TIMEOUT.
tool_sh-live-stubs := $(T)/live/stubs.sh $(F_ABS) --part core | $(T)/live/stubs.sh $(T)/live/hello.fib $(T)/live/deep.fib $(T)/live/hello-stubs.golden compiler/emit/live.fib
tool_sh-live-stubs-own := $(T)/live/stubs.sh $(F_ABS) --part ownership -j $(CASE_JOBS) | $(T)/live/stubs.sh $(T)/live/catching.expected compiler/emit/live.fib
tool_sh-live-stubs-sample := $(T)/live/stubs.sh $(F_ABS) --part sample -j $(CASE_JOBS) | $(T)/live/stubs.sh $(T)/live/catching.expected compiler/emit/live.fib
tool_sh-live-run := $(T)/live/run.sh $(F_ABS) | $(wildcard $(T)/live/*) $(wildcard compiler/live/*.fib) compiler/live.fib
# LT3 (docs/design/live-meshql.md 4): the live module rule in a project (a fixture project with a git dependency: the project's modules are live, the
# libraries' under --live-deps); LT1 (3.2): fib.test.run's argv override and report sink (the collected report is the child's, byte for byte)
tool_sh-live-project := $(T)/live/project.sh $(F_ABS) | $(T)/live/project.sh $(T)/live/project-stubs.golden $(T)/deps/fixtures.sh compiler/emit/live.fib compiler/pkg/modules.fib compiler/driver/front.fib compiler/fibc.fib $(wildcard compiler/serve/live*.fib)
tool_sh-live-hooks := $(T)/live/hooks.sh $(F_ABS) | $(T)/live/hooks.sh $(T)/live/hooks.fib $(T)/live/hooks-spec.fib $(T)/live/hookspecs.fib lib/fib/test/run.fib
# LV1: the attach socket of `fibc run --live` (serve.live*), over a real socket against app.fib, and the peer-uid unit (about 2 minutes); LE5 (the
# engine warmed before the first connection: serve.incr, serve.replcore) and LE4 (`inspect` as a lookup) of docs/design/live-incremental.md
tool_sh-live-attach := $(T)/live/attach.sh $(F_ABS) | $(T)/live/attach.sh $(T)/live/attach.py $(T)/live/app.fib $(T)/live/unit-livesock.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/serve/incr.fib compiler/serve/replcore.fib
# LV2: body swaps in the running program (serve.liveswap, serve.livetable, serve.liverules): swap-app.fib's server holds the handler as a value, a worker
# thread calls the swapped function; the design's transcripts 1, 4 and 5 through the terminal client (about 2 minutes)
tool_sh-live-swap := $(T)/live/swap.sh $(F_ABS) | $(T)/live/swap.sh $(T)/live/swap.py $(T)/live/unit-livediff.fib $(T)/live/swap-app.fib $(wildcard $(T)/live/swapalt/*.fib) $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/emit/live.fib compiler/serve/incr.fib
# docs/design/inline-lir.md: a body swap of a function that holds a `lir` form re-emits the site (lir.sh: the running program takes the new body)
tool_sh-live-lir := $(T)/live/lir.sh $(F_ABS) | $(T)/live/lir.sh $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/types/lower/expr.fib compiler/lir/site.fib
# LE1 (docs/design/live-incremental.md 2.2, 4): the constant defs' values kept in the session's Tables (emit.defs.cache): defcache-app.fib's defs read
# through three generations and by expressions, a required module edited on disk read anew, FIB_LIVE_EMIT=off and =verify agreeing with the cache
# on, the emit of a swap under a third of the cache-off one (about two minutes)
tool_sh-live-defcache := $(T)/live/defcache.sh $(F_ABS) | $(T)/live/defcache.sh $(T)/live/defcache.py $(T)/live/defcache-app.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/emit/defs/*.fib) compiler/emit/program.fib
# LE3 (docs/design/live-incremental.md 2.3): the queue mode (emit.program `resident`/`program-request`, emit.typetext, serve.liveswap
# `requests-of`): queue-app.fib's swaps lower only the changed family against the resident program; the handler swapped twice, a generic
# swapped through a resident caller, a new specialisation, and the swap's emit under 60 ms with the queue mode on vs a whole emit off
tool_sh-live-queue := $(T)/live/queue.sh $(F_ABS) | $(T)/live/queue.sh $(T)/live/queue.py $(T)/live/queue-app.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/emit/*.fib) $(wildcard compiler/emit/defs/*.fib) compiler/emit/typetext.fib
# LT2 (docs/design/live-meshql.md 3): the `test` op of a live session (serve.livetest, serve.livewords, `--with`): test-app.fib's spec run from the socket
# and the terminal client, a swap during a two-second scenario (stale), busy, the spec loaded beside the app; the unit of the pure half (about three minutes)
tool_sh-live-test := $(T)/live/test.sh $(F_ABS) | $(T)/live/test.sh $(T)/live/test.py $(T)/live/test-app.fib $(T)/live/test-spec.fib $(T)/live/unit-livetest.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/expand/modules.fib compiler/expand/roots.fib lib/fib/test/run.fib
# LT5 (docs/design/live-meshql.md 3.8): `watch` of a live session (serve.livewatch): an edit of the handler's module file loaded and the last test
# rerun as events, a feature file's change rerun without a load, a refused load rerun nothing, `watch off` quiet (about a minute)
tool_sh-live-watch := $(T)/live/watch.sh $(F_ABS) | $(T)/live/watch.sh $(T)/live/watch.py $(T)/live/watch-app.fib $(T)/live/watch-handler.fib $(wildcard compiler/serve/live*.fib) lib/fib/test/run.fib
tool_sh-live-testlive := $(T)/live/testlive.sh $(F_ABS) | $(T)/live/testlive.sh compiler/driver/testlive.fib compiler/driver/test.fib compiler/fibc.fib $(wildcard compiler/serve/live*.fib) lib/fib/test/run.fib
# the statics numbered per program (emit.statics `program-static?`: a quoted form `@q.N`, a closure constant, a vtable) across a swap: a spec function
# swapped at the REPL (the `expect` macro quotes its form) and the base's scenarios run after it, `specs` itself swapped between three bindings and one
# (serve.livesplit `split` keeps them with the code, never exports them); the unit of the names (a nested literal's generation; about a minute)
tool_sh-live-quotes := $(T)/live/quotes.sh $(F_ABS) | $(T)/live/quotes.sh $(T)/live/quotes.py $(T)/live/unit-livetable.fib $(T)/live/test-app.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/emit/statics.fib compiler/emit/defs/cache.fib
# LE2 (docs/design/live-incremental.md 2.2 item 3): closures and vtables named by content in live mode (`clo.CODE`, `vt.NAME`, emit.statics): a body
# swapped to call through a def's closures and to make a closure of its own, a method swap, a new instance in an input; the cuts declare the base's
# (FIB_LIVE_DEBUG=cut) and `status` holds one of each (about ten seconds)
tool_sh-live-statics := $(T)/live/statics.sh $(F_ABS) | $(T)/live/statics.sh $(T)/live/statics.py $(T)/live/statics-app.fib $(wildcard compiler/serve/live*.fib) compiler/emit/statics.fib compiler/emit/defs/cache.fib
# LM1 (docs/design/live-incremental.md 3): `load` and `reload` of a live session (serve.liveload; the engine's module text overrides, serve.incr `Job.texts`
# and expand.modules `try-load-texts`; emit.compile `BodyRequest.module`): load-app.fib with load-routes.fib edited on disk and loaded through the socket
# and the terminal client; all or nothing, live/Removed, live/NotLoaded, the override against a half-saved file (about two minutes)
tool_sh-live-load := $(T)/live/load.sh $(F_ABS) | $(T)/live/load.sh $(T)/live/load.py $(T)/live/load-app.fib $(T)/live/load-routes.fib $(wildcard compiler/serve/live*.fib) compiler/serve/incr.fib compiler/serve/replcore.fib compiler/expand/modules.fib compiler/emit/live.fib compiler/emit/compile.fib compiler/types/program.fib
# The loader's memo of parsed files (expand.parsed, serve.incr `Stats.parsed`; docs/design/live-incremental.md 2.6): an input parses only the files
# whose text changed; an edit loaded, a one-byte edit and a file put back are seen (about a minute)
tool_sh-live-parsed := $(T)/live/parsed.sh $(F_ABS) | $(T)/live/parsed.sh $(T)/live/parsed.py $(T)/live/load-app.fib $(T)/live/load-routes.fib compiler/expand/parsed.fib compiler/expand/modules.fib compiler/serve/incr.fib compiler/serve/proto.fib
# LV3: swaps under load, the pause and the retirement (serve.liveretire, serve.livesplit, rt/live.lir, fib.os.live, the pause points of fib.http.server):
# the socket stress (stress-app.fib: 8 threads and the pool under 1000 swaps, pause, retire, resume; swap-app.fib's http pause points) and the
# ThreadSanitizer run of the stub and slot protocol (stress-tsan.fib in live mode with stress-swapper.lir appended), about two minutes each
tool_sh-live-stress := $(T)/live/stress.sh $(F_ABS) --socket-only | $(T)/live/stress.sh $(T)/live/stress.py $(T)/live/stress-app.fib $(T)/live/swap-app.fib $(wildcard $(T)/live/swapalt/*.fib) $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/emit/live.fib rt/live.lir lib/fib/os/live.fib $(wildcard lib/fib/http/server/*.fib)
tool_sh-live-stress-tsan := env TSAN_OUT=$(TOOLS_OUT)/live-stress-tsan $(T)/live/stress.sh $(F_ABS) --tsan-only | $(T)/live/stress.sh $(T)/live/stress-tsan.fib $(T)/live/stress-swapper.lir scripts/tsan.sh scripts/tsan-baseline.txt compiler/emit/live.fib rt/live.lir lib/fib/os/live.fib
# The persistent checker's controls (run.sh, about 2 minutes whole) as independent stamps of at most about 50 s: the command-line programs in three groups,
# the three batch comparisons (each builds its own emit tool), and the server edits. PERSIST_PARTS are queued right after the fixed point (mk/gate.mk).
PERSIST_PARTS := cmd-0 cmd-1 cmd-2 batch-verify batch-onoff batch-single server prefix
$(foreach p,$(PERSIST_PARTS),$(eval tool_sh-persist-$(p) := $(T)/persist/run.sh $(F_ABS) $(p) | $(wildcard $(T)/persist/*)))
# The three batch shares use the emit tool the golden suites already build (build/golden/tools/emit: F builds compiler/emit.fib, the same program they
# would each build, 24 s of one core each): one build for the four users (docs/design/gate-profile.md).
PERSIST_EMIT_BIN := $(abspath $(GOLDEN_DIR)/tools/emit)
$(foreach p,batch-verify batch-onoff batch-single,$(eval tool_sh-persist-$(p) := env PERSIST_EMIT=$(PERSIST_EMIT_BIN) $(T)/persist/run.sh $(F_ABS) $(p) | $(wildcard $(T)/persist/*) $(GOLDEN_DIR)/tools/emit))
PERSIST_STAMPS := $(addprefix sh-persist-,$(PERSIST_PARTS))
tool_sh-emit-resume := $(T)/emit/resume.sh $(F_ABS) | $(T)/emit/resume.sh
tool_sh-emit-windows := $(T)/emit/windows.sh $(F_ABS) | $(T)/emit/windows.sh $(T)/emit/inspect.fib
tool_sh-fibref-heap-gold := $(T)/fibref/heap-gold.sh | $(wildcard $(T)/fibref/heap-gold*)
tool_sh-native-h-checks := $(T)/native/h-checks.sh | $(wildcard $(T)/native/h-*) $(wildcard $(T)/native/h-cases/*)
tool_jit-selfhosted-threads := $(T)/driver/jit-selfhosted.sh $(F_ABS) $(abspath $(BUILD)/F3) | $(T)/driver/jit-selfhosted.sh $(T)/driver/jit/threads.fib $(BUILD)/F3 $(wildcard rt/*.lir)
tool_sh-native-l1-unit := $(T)/native/l1-unit.sh | $(wildcard $(T)/native/l1-*)
# The liro suite is one stamp per independent job (checks, and one per planted fault: each builds its own probe), so make runs them side by side.
LIRO_PLANTS := lookup-disabled stale-body-key optimization-omitted abi-check-disabled checksum-disabled
LIRO_INPUTS := $(T)/native/liro.sh $(T)/native/liro/check.py $(T)/native/liro/mutants.py $(T)/native/liro/api.fib $(T)/native/liro/reuse.fib $(wildcard compiler/native/liro/*.fib)
tool_sh-native-liro-checks := env LLVMLIB=$(LLVM_LIBDIR) bash $(T)/native/liro.sh checks | $(LIRO_INPUTS)
$(foreach p,$(LIRO_PLANTS),$(eval tool_sh-native-liro-plant-$(p) := env LLVMLIB=$(LLVM_LIBDIR) bash $(T)/native/liro.sh plant $(p) | $(LIRO_INPUTS)))
LIRO_STAMPS := sh-native-liro-checks $(addprefix sh-native-liro-plant-,$(LIRO_PLANTS))
tool_sh-native-a64-sibcall := $(T)/native/a64-sibcall.sh $(F_ABS) | $(T)/native/a64-sibcall.sh cases/lir/instr/stackargs-sibcall.lir $(wildcard compiler/native/lower/*.fib)
tool_sh-harness-proto-run := $(T)/harness-proto/run.sh | $(wildcard $(T)/harness-proto/*) $(wildcard $(T)/harness-proto/cases/*)
tool_lsp-unit := env LSP_UNIT_OUT=$(TOOLS_OUT)/lsp-unit $(T)/lsp/run.sh | $(wildcard $(T)/lsp/*) $(wildcard $(T)/lsp/fix/*) $(wildcard compiler/lsp/*.fib)
tool_lsp-server := $(T)/lsp/server.sh $(TOOLS_OUT)/lsp-server | $(wildcard $(T)/lsp/*) $(wildcard $(T)/lsp/fix/*) $(wildcard compiler/lsp/*.fib)
tool_lsp-hardening := $(T)/lsp/hardening.sh $(TOOLS_OUT)/lsp-hard | $(wildcard $(T)/lsp/*) $(wildcard compiler/lsp/*.fib)
tool_fibref-heap := $(T)/fibref/heap.sh | $(wildcard $(T)/fibref/*) $(wildcard compiler/fibref/*.fib compiler/fibref/*/*.fib)
tool_gen-rng := $(T)/gen/rng-check.sh | $(wildcard $(T)/gen/*)
tool_gen-compare := bash -c "$(F_ABS) build compiler/fibgen.fib -I compiler -I lib -o $(TOOLS_OUT)/fibgen && $(T)/gen/compare.sh $(TOOLS_OUT)/fibgen pipelines 1 300" | $(wildcard $(T)/gen/*) $(wildcard compiler/gen/*.fib compiler/gen/*/*.fib)
tool_gen-planted := $(T)/gen/planted.sh | $(wildcard $(T)/gen/*)
tool_js-backend := $(T)/js/check.sh $(TOOLS_OUT)/js | $(wildcard $(T)/js/*) $(wildcard $(T)/js/cases/*) $(wildcard compiler/js/*.fib)
tool_deps := $(T)/deps/run.sh $(F_ABS) | $(wildcard $(T)/deps/*)
tool_sh-native-gpu-emit := $(T)/native/gpu-emit.sh | $(T)/native/gpu-emit.sh $(wildcard examples/gpu/*) $(wildcard compiler/native/*.fib)
tool_sh-native-gpu-device := $(T)/native/gpu-device.sh | $(T)/native/gpu-device.sh $(wildcard compiler/tests/native/gpu/*) $(wildcard examples/gpu/*) $(wildcard lib/fib/gpu/*.fib) $(wildcard compiler/native/*.fib compiler/native/wgsl/*.fib)
tool_sh-make-graph := $(T)/make/graph.sh | $(wildcard $(T)/make/*) scripts/mac-gate.sh Makefile $(wildcard mk/*.mk)
# The retirement against what running state holds (docs/design/live-incremental.md 7; serve.livereach): closures of a retired generation in an
# atom, a def and a spawned thread; a worker parked at a pause point it reaches through a function value, in a body swapped and retired under it
tool_sh-live-retire := $(T)/live/retire.sh $(F_ABS) | $(T)/live/retire.sh $(T)/live/retire.py $(T)/live/retire-app.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) rt/live.lir lib/fib/os/live.fib

TOOLS_QUICK := sh-emit-prune fibref-skeleton gen-skeleton lint-pipefail lint-portable sh-driver-check-lib sh-make-graph
# LT4 (docs/design/live-meshql.md, docs/design/live-repl.md 9): `def`s redefined in a live session (serve.livedefs, serve.liveretype, emit.livedef):
# the guard (no def slot in a plain emit, a duplicate def still an error), defs.py over a real socket (type changes with their readers swapped or
# refused naming the reader, boxed values of more than one word, 1000 redefinitions under 8 reading threads, pause and retire drop the old values),
# and ThreadSanitizer over the slot's acquire load against the setter's release store, a boxed value's too (about two minutes each)
tool_sh-live-defs := $(T)/live/defs.sh $(F_ABS) --socket-only | $(T)/live/defs.sh $(T)/live/defs.py $(T)/live/defs-app.fib $(T)/live/defs-cfg.fib $(wildcard compiler/serve/live*.fib) $(wildcard compiler/live/*.fib) compiler/emit/program.fib compiler/emit/livedef.fib compiler/emit/inits.fib compiler/emit/lower/objects.fib
tool_sh-live-defs-tsan := env TSAN_OUT=$(TOOLS_OUT)/live-defs-tsan $(T)/live/defs.sh $(F_ABS) --tsan-only | $(T)/live/defs.sh $(T)/live/defs-tsan.fib $(T)/live/defs-swapper.lir scripts/tsan.sh scripts/tsan-baseline.txt compiler/emit/program.fib compiler/emit/livedef.fib compiler/emit/lower/objects.fib
TOOLS_FULL := $(TOOLS_QUICK) unit-api units-all units-rest units-pending shootout-compile sh-driver-cli sh-driver-demand sh-driver-muladd \
  sh-driver-no-fma sh-driver-cpu-check sh-driver-target sh-driver-test-cmd sh-driver-static-host sh-emit-runtime sh-emit-defs-order sh-stack-stack \
  sh-own-peek sh-specs-plant-vec sh-lanes-lanes sh-repl-run sh-serve-run sh-live-stubs sh-live-stubs-own sh-live-stubs-sample sh-live-run sh-live-project sh-live-hooks sh-live-attach sh-live-swap sh-live-lir sh-live-defcache sh-live-queue sh-live-test sh-live-testlive sh-live-watch sh-live-quotes sh-live-statics sh-live-load sh-live-parsed sh-live-stress sh-live-stress-tsan sh-live-defs sh-live-defs-tsan sh-live-retire \
  $(PERSIST_STAMPS) sh-emit-resume sh-emit-windows sh-fibref-heap-gold sh-native-h-checks \
  sh-native-l1-unit $(LIRO_STAMPS) jit-selfhosted-threads sh-native-a64-sibcall sh-native-gpu-emit sh-native-gpu-device sh-harness-proto-run lsp-unit lsp-server lsp-hardening fibref-heap gen-rng gen-compare gen-planted js-backend deps
# The command is every word up to the `|`, the inputs every word after it.
upto_bar = $(if $(filter |,$(firstword $(1))),,$(if $(1),$(firstword $(1)) $(call upto_bar,$(wordlist 2,999999,$(1))),))
after_bar = $(if $(1),$(if $(filter |,$(firstword $(1))),$(wordlist 2,999999,$(1)),$(call after_bar,$(wordlist 2,999999,$(1)))),)

define tool_target
$(TOOLS_DIR)/$(1).ok: $(F) $(call after_bar,$(tool_$(1))) | $(TOOLS_DIR)/
	rm -rf $(TOOLS_OUT)/$(1); mkdir -p $(TOOLS_OUT)
	$$(call stamp,timeout $(TOOLS_TIMEOUT) $(tool_env) TMPDIR=$(TOOLS_OUT) $(call upto_bar,$(tool_$(1))))
endef
$(foreach t,$(TOOLS_FULL),$(eval $(call tool_target,$(t))))
TOOLS_QUICK_STAMPS := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(TOOLS_QUICK)))
TOOLS_FULL_STAMPS := $(addsuffix .ok,$(addprefix $(TOOLS_DIR)/,$(TOOLS_FULL)))
tools: $(TOOLS_FULL_STAMPS)   ## every tool test (the unit programs, the language server, the fibref and fibgen ports, the js backend, deps)
tools-quick: $(TOOLS_QUICK_STAMPS)   ## the quick gate's tool tests (the skeletons, the lint, check-lib, the build's own graph test)
.PHONY: tools tools-quick
tools-list:   ## the stamps of the tool tests (TOOLS_MODE=quick for the quick gate's)
	@printf '%s\n' $(if $(filter quick,$(TOOLS_MODE)),$(TOOLS_QUICK_STAMPS),$(TOOLS_FULL_STAMPS))
.PHONY: tools-list
