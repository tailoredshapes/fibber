#!/bin/bash
# compiler/tests/live/swap.sh [FIBC]: the checks of body swaps in a running program (package LV2 of docs/design/live-repl.md section 6;
# serve.liveswap, serve.livetable, serve.liverules, serve.livediff, emit.live): swap.py over a real socket against swap-app.fib (its docstring lists the
# checks: the transcripts 1, 4 and 5 of the design, a closure value made before the swap, a method, a generic function, 200 swaps under
# a worker thread, the refusals, LM2's change by expanded form; first unit-livediff.fib), about two minutes. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh
# (patch-before-jit, old-tracker-removed, summary-ignored, signature-ignored, diff-by-text, hash-with-positions) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: swap.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
out=${TMPDIR:-/tmp}/live-swap-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
# unit-livediff.fib: the hashes of serve.livediff (LM2), built without LLVM
if (ulimit -v 16000000; "$fibc" build "$here/unit-livediff.fib" -I compiler -I lib -o "$out/unit") > "$out/unit.build" 2>&1 && "$out/unit" > "$out/unit.log" 2>&1; then
  echo "ok unit-livediff: $(tail -n 1 "$out/unit.log")"
else echo "FAIL unit-livediff"; tail -5 "$out/unit.build" "$out/unit.log" 2>/dev/null; bad=1; fi
python3 "$here/swap.py" "$fibc" "$root" || bad=1
exit "$bad"
