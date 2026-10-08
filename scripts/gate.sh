#!/bin/bash
# The gate: one command that says whether a tree is good. Since MAKE-1 (docs/design/build.md) it is a thin wrapper over the Makefile: the
# stages are file targets under build/ and GNU Make runs them with its jobserver, incrementally (a stage whose inputs have not changed since
# it passed is not run again; nothing else is cached). The summary lines are the ones the lead's automation reads: `gate:`, `build:`,
# `cases: DIR Ns (exit N), ..`, `== timing`, `GATE PASS (full)` or `GATE FAIL (full)` (exit 0 or 1; 2 for a usage or setup error).
#   scripts/gate.sh [--quick|--full] [-j N]
#   --quick (default)  `make quick`: stage 2, the tool skeletons, ownership, modules, the stdlib sample (GATE_SPECS=1 adds the specs)
#   --full             `make gate`: stage 2 built by the seed, the fixed point, golden, every tool test, the ADRs, all three case directories,
#                      static (when the musl pieces are there), wasm (when the wasi-sdk is there), the specs
#   --incremental      accepted and ignored: every run is incremental now
#   -j N               `fibc cases -j N` inside a shard (CASE_JOBS, default 4)
# GATE_BUDGET (default the number of cores, at most 16) is make's -j: how many recipes run at once. GATE_FRESH=1 rebuilds F from the seed.
# FIBC or SEED name the fibc that builds stage 2 (CI gives the seed; the default is the seed SEED names, fetched under build/seed/).
# GATE_ROOT judges another checkout (scripts/batch.sh). GATE_OUT is not read any more: the build directory is build/ of the tree (BUILD=..).
# Takes /tmp/fibsuite.lock for its whole run, so a benchmark (scripts/bench/quick.sh) never overlaps it.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/.." && pwd)}
mode=quick; jobs=${GATE_JOBS:-4}
while [ $# -gt 0 ]; do
  case $1 in
    --quick) mode=quick ;; --full) mode=full ;; --incremental) ;;
    -j) jobs=${2:?-j needs a number}; shift ;;
    -j*) jobs=${1#-j} ;;
    *) echo "usage: scripts/gate.sh [--quick|--full] [-j N]" >&2; exit 2 ;;
  esac
  shift
done
case $jobs in ''|*[!0-9]*|0) echo "gate: -j wants a whole number of at least 1" >&2; exit 2 ;; esac
cores=$(nproc 2> /dev/null || echo 16); [ "$cores" -gt 16 ] && cores=16
budget=${GATE_BUDGET:-$cores}
case $budget in *[!0-9]*|'') echo "gate: GATE_BUDGET wants a whole number" >&2; exit 2 ;; esac
[ "$budget" -ge 1 ] || { echo "gate: GATE_BUDGET wants at least 1" >&2; exit 2; }
now() { date +%s.%N; }
elapsed() { echo "$2 - $1" | bc | awk '{printf "%.1f", $1}'; }
# The lock every heavy run takes (a gate run, a bench run), held by this process and its children (FIBSUITE_LOCKED says so).
if [ -z "${FIBSUITE_LOCKED:-}" ]; then
  exec 9> /tmp/fibsuite.lock
  flock -n 9 || { echo "waiting for /tmp/fibsuite.lock (another gate or bench is running)"; flock 9; }
  export FIBSUITE_LOCKED=1
fi
cd "$root" || exit 2
builder=()
if [ -n "${FIBC:-}${SEED:-}" ]; then
  s=${FIBC:-$SEED}
  if [ -d "$s" ]; then if [ -x "$s/bin/fibc" ]; then s=$s/bin/fibc; else s=$s/fibc; fi; fi
  builder=(BUILDER="$s")
fi
make=${MAKE:-make}; command -v gmake > /dev/null 2>&1 && [ "$(uname -s)" = Darwin ] && make=gmake
t0=$(now)
if [ "$mode" = full ]; then stamps=gate-stamps; else stamps=quick-stamps; fi
echo "gate: $mode, -j $jobs, budget $budget (make -j$budget), tree $(git -C "$root" rev-parse --short HEAD 2> /dev/null || echo '?')"
"$make" -k -j"$budget" "$stamps" CASE_JOBS="$jobs" GATE_MODE="$mode" "${builder[@]}" 2>&1 | grep -v '^make: \*\*\* \[' | grep -v '^make: Target .* not remade'
"$make" --no-print-directory gate-report GATE_MODE="$mode" "${builder[@]}" | sed "s/^  total .*//; /^GATE /i\\  total $(elapsed "$t0" "$(now)") s"
exit "${PIPESTATUS[0]}"
