#!/bin/bash
# The local Mac gate: Make owns the job budget; each recipe runs CASE_JOBS cases and CHECK_JOBS programs at once (4 and 2, the Makefile's
# defaults: on the M1 Ultra, CASE_JOBS=1 left `specs` alone on one core for 8 of the 11 minutes of a cold gate, docs/design/gate-profile.md, Mac).
# Usage: scripts/mac-gate.sh [--full|--quick] [--plan]
# GATE_BUDGET overrides the hardware budget; BUILD, SCRATCH and BUILDER go to Make.
set -euo pipefail
export PATH=${PATH:-/usr/bin:/bin}:/opt/homebrew/bin:/usr/sbin:/sbin
root=${GATE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
root=$(cd "$root" && pwd)
mode=full; plan=0
while [ "$#" -gt 0 ]; do
  case $1 in
    --full) mode=full ;;
    --quick) mode=quick ;;
    --plan) plan=1 ;;
    *) echo 'usage: scripts/mac-gate.sh [--full|--quick] [--plan]' >&2; exit 2 ;;
  esac
  shift
done
[ "$(uname -s)" = Darwin ] || { echo 'mac-gate: requires macOS' >&2; exit 2; }
. "$root/scripts/portable/env.sh"
cores=$(sysctl -n hw.perflevel0.physicalcpu 2>/dev/null || sysctl -n hw.physicalcpu)
memory=$(sysctl -n hw.memsize)
# At most twelve heavy recipes; reserve 16 GiB and allow 3 GiB per recipe.
budget=$cores
[ "$budget" -le 12 ] || budget=12
memory_jobs=$(( (memory / 1073741824 - 16) / 3 ))
[ "$memory_jobs" -ge 1 ] || memory_jobs=1
[ "$budget" -le "$memory_jobs" ] || budget=$memory_jobs
budget=${GATE_BUDGET-$budget}
case $budget in ''|*[!0-9]*|0) echo 'GATE_BUDGET must be a positive integer' >&2; exit 2 ;; esac
make=${MAKE:-gmake}
case_jobs=${CASE_JOBS:-4}; check_jobs=${CHECK_JOBS:-2}
args=("CASE_JOBS=$case_jobs" "CHECK_JOBS=$check_jobs" UNITS_JOBS=1
      "LIMIT_STDLIB=${LIMIT_STDLIB:-3600}" "LIMIT_OWNERSHIP=${LIMIT_OWNERSHIP:-1200}")
[ -z "${BUILD:-}" ] || args+=("BUILD=$BUILD")
[ -z "${SCRATCH:-}" ] || args+=("SCRATCH=$SCRATCH")
[ -z "${BUILDER:-}" ] || args+=("BUILDER=$BUILDER")
if [ "$mode" = quick ]; then args+=(MAC_QUICK=1); else args+=(MAC_QUICK=); fi
printf 'Mac gate: %s, %s performance cores, %s GiB RAM, %s Make jobs, %s case workers per recipe\n' \
  "$mode" "$cores" "$((memory / 1073741824))" "$budget" "$case_jobs"
if [ "$plan" -eq 1 ]; then
  printf '%q -k -j%s mac-stamps' "$make" "$budget"; printf ' %q' "${args[@]}"; printf '\n'
  printf '%q mac-report' "$make"; printf ' %q' "${args[@]}"; printf '\n'
  exit 0
fi
if [ -z "${FIBSUITE_LOCKED:-}" ]; then
  exec 9> /tmp/fibsuite.lock
  flock -n 9 || { echo 'waiting for /tmp/fibsuite.lock (another gate or bench is running)'; flock 9; }
  export FIBSUITE_LOCKED=1
fi
cd "$root"
export CASE_JOBS=$case_jobs CHECK_JOBS=$check_jobs UNITS_JOBS=1
started=$(date +%s)
"$make" -k -j"$budget" mac-stamps "${args[@]}" || true
status=0
"$make" --no-print-directory mac-report "${args[@]}" || status=$?
printf 'Mac gate wall time: %s s\n' "$(( $(date +%s) - started ))"
exit "$status"
