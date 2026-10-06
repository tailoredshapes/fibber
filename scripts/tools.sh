#!/bin/bash
# The tool tests of the gate: the fast scripts of the language server, the fibref port and the fibgen port, each under a time bound, one
# line each (`ok NAME N s` or `FAIL NAME N s`), exit 0 when every one passed. Called by scripts/gate.sh (the `tools` stage); also runnable alone.
#   scripts/tools.sh [--quick|--full] FIBC [OUTDIR]
#   --quick   the skeletons only (every module of the ports builds and links): fibref/skeleton.sh, gen/skeleton.sh
#   --full    the skeletons, and: units.sh in five parts (the unit programs of the passes; each unit-*.fib is run, or is listed as pending or run
#             elsewhere in compiler/tests/units.*), shootout-compile.sh (the programs of docs/shootout/parallel compile), lsp/run.sh (the unit programs), lsp/server.sh (the replay of the recorded transcripts), lsp/hardening.js (the cases
#             of the fuzz findings), lsp/fuzz.js --selftest, fibref/heap.sh, gen/rng-check.sh, gen/compare.sh pipelines 1 300, gen/planted.sh
#   FIBC      a stage 2 built from this tree (several of the scripts need what is newer than the seed); FIB_LIB the library (default lib/)
#   TOOLS_ONLY names (space separated: fibref-skeleton gen-skeleton units-emit units-own units-types units-rest units-pending shootout-compile sh-* (the scripts of compiler/tests listed below: sh-driver-cli, sh-emit-resume, ..) lsp-unit lsp-server lsp-hardening fibref-heap gen-rng gen-compare gen-planted) to run only those
#   TOOLS_JOBS how many run at once (default 3: each builds a program; 64 under the gate, whose slots limit the heavy processes); TOOLS_TIMEOUT seconds per script (default 900)
# The fuzz run itself (lsp/fuzz.sh) and the planted faults of the language server (lsp/mutants.sh) are slower and are not run here.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/.." && pwd)}
mode=full
case ${1:-} in --quick) mode=quick; shift ;; --full) shift ;; esac
fibc=${1:?usage: tools.sh [--quick|--full] FIBC [OUTDIR]}
out=${2:-${TMPDIR:-/tmp}/tools-$$}
jobs=${TOOLS_JOBS:-$([ -n "${GATE_SLOTS:-}" ] && echo 64 || echo 3)}; limit=${TOOLS_TIMEOUT:-900}
mkdir -p "$out"
# Under the gate (GATE_SLOTS, scripts/lib/slots.sh) every script runs at once and each fibc it starts holds a slot, through the wrapper
# scripts/lib/fibc-slot.sh; alone, three scripts run at a time and FIBC is the fibc itself.
if [ -n "${GATE_SLOTS:-}" ]; then export GATE_REAL_FIBC=$fibc; fibc=$here/lib/fibc-slot.sh; fi
export FUZZ_SELFTEST_DEADLINE=${FUZZ_SELFTEST_DEADLINE:-8000}   # lsp/fuzz.js --selftest: the crash probe of its fake server must be seen as a crash, not a hang, on a busy machine
export FIBC=$fibc FIB_LIB=${FIB_LIB:-$root/lib} TMPDIR=$out
cd "$root" || exit 2
t=compiler/tests

# one NAME CMD..: runs CMD with the time bound, its output to OUTDIR/NAME.log, its verdict line to OUTDIR/NAME.res
one() {
  local name=$1; shift
  local t0; t0=$(date +%s)
  timeout "$limit" "$@" > "$out/$name.log" 2>&1
  local rc=$? s=$(( $(date +%s) - t0 ))
  if [ $rc -eq 0 ]; then echo "ok $name $s s" > "$out/$name.res"
  elif [ $rc -eq 124 ]; then echo "FAIL $name $s s (timed out after $limit s)" > "$out/$name.res"
  else echo "FAIL $name $s s (exit $rc)" > "$out/$name.res"; fi
}

names=()
queue() { if [ -n "${TOOLS_ONLY:-}" ] && [[ " $TOOLS_ONLY " != *" $1 "* ]]; then return; fi; names+=("$1"); ( one "$@" ) & while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do wait -n; done; }

queue fibref-skeleton "$t/fibref/skeleton.sh" "$out/fibref-skeleton"
queue gen-skeleton "$t/gen/skeleton.sh" "$out/gen-skeleton"
if [ "$mode" = full ]; then
  # the unit programs of the passes: every compiler/tests/*/unit-*.fib is run, or listed as pending (failing, with the reason) or run elsewhere
  for part in emit own types rest pending; do queue "units-$part" env UNITS_DIR=$part "$t/units.sh" "$out/units-$part"; done
  # the programs of docs/shootout/parallel are measurements, not tests, but they must keep compiling against the library's API
  queue shootout-compile compiler/tests/shootout-compile.sh "$out/shootout-compile"
  # the scripts under compiler/tests that nothing ran and that take seconds: the command line, the demand-driven check, the target and fma
  # checks, the `test` command, the runtime drift test, the resume builders, the window lowering, the cell peeks, the Vec contract's planted faults,
  # the heap golden traces, the lair header and exec checks, the L1 unit programs, the test harness's own check
  for s in driver/cli driver/demand driver/muladd driver/target driver/test-cmd emit/runtime own/peek specs/plant-vec; do
    queue "sh-${s//\//-}" "$t/$s.sh" "$FIBC"
  done
  queue sh-emit-resume env RESUME_OUT="$out/resume" "$t/emit/resume.sh" "$FIBC"
  queue sh-emit-windows env WINDOWS_SCRATCH="$out/windows" "$t/emit/windows.sh" "$FIBC"
  for s in fibref/heap-gold native/h-checks native/l1-unit harness-proto/run; do queue "sh-${s//\//-}" "$t/$s.sh"; done
  queue lsp-unit "$t/lsp/run.sh"
  queue lsp-server "$t/lsp/server.sh" "$out/lsp-server"
  queue lsp-hardening bash -c "'$t/lsp/build.sh' '$out/lsp-hard' && node '$t/lsp/hardening.js' --server '$out/lsp-hard' && node '$t/lsp/fuzz.js' --selftest"
  queue fibref-heap "$t/fibref/heap.sh"
  queue gen-rng "$t/gen/rng-check.sh"
  queue gen-compare bash -c "'$FIBC' build compiler/fibgen.fib -I compiler -I lib -o '$out/fibgen' && '$t/gen/compare.sh' '$out/fibgen' pipelines 1 300"
  queue gen-planted "$t/gen/planted.sh"
  queue js-backend "$t/js/check.sh" "$out/js"
  queue deps "$t/deps/run.sh" "$FIBC"
fi
wait
bad=0
for n in "${names[@]}"; do
  cat "$out/$n.res" 2>/dev/null || { echo "FAIL $n (no result)"; }
  grep -q '^ok' "$out/$n.res" 2>/dev/null || { bad=1; tail -n 12 "$out/$n.log" | sed 's/^/    /'; }
done
exit $bad
