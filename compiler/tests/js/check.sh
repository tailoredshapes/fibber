#!/bin/bash
# The JS backend's check, as one command for scripts/tools.sh (docs/design/js-backend.md, "Joining the gate"): builds lairf and lir2js
# with $FIBC into OUTDIR, runs the runtime's unit tests, then the differential harness over cases/lir and compiler/tests/js/cases with
# the reject cases, against expected.txt. With --faults also the planted faults (about 2 minutes more: three rebuilds of lir2js).
#   compiler/tests/js/check.sh [--faults] OUTDIR
# Environment: FIBC (a stage 2 or the seed), LLVM_LIB (default /usr/lib/llvm-21/lib), NODE, CHECK_JOBS (programs at once, default 2).
# Exit 0 when everything held.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
faults=0; [ "${1:-}" = --faults ] && { faults=1; shift; }
out=${1:?usage: check.sh [--faults] OUTDIR}; mkdir -p "$out"
: "${FIBC:?}"; llvm=${LLVM_LIB:-/usr/lib/llvm-21/lib}
cd "$root" || exit 2
"$FIBC" build compiler/lairf.fib -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$out/lairf" || { echo "FAIL: lairf did not build"; exit 1; }
"$FIBC" build compiler/lir2js.fib -I compiler -I lib -o "$out/lir2js" || { echo "FAIL: lir2js did not build"; exit 1; }
export LAIRF=$out/lairf LIR2JS=$out/lir2js LD_LIBRARY_PATH=$llvm${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH} TMPDIR=$out
ok=0
"${NODE:-node}" "$here/rt-test.mjs" || ok=1
"$here/diff.sh" -j "${CHECK_JOBS:-2}" --reject --expect "$here/expected.txt" > "$out/diff.log" 2>&1 || { ok=1; grep -v '^pass ' "$out/diff.log"; }
tail -2 "$out/diff.log"
if [ $faults -eq 1 ]; then "$here/faults.sh" -j "${CHECK_JOBS:-2}" || ok=1; fi
exit $ok
