#!/bin/bash
# compiler/tests/live/swap.sh [FIBC]: the checks of body swaps in a running program (package LV2 of docs/design/live-repl.md section 6;
# serve.liveswap, serve.livetable, serve.liverules, emit.live): swap.py over a real socket against swap-app.fib (its docstring lists the
# checks: the transcripts 1, 4 and 5 of the design, a closure value made before the swap, a method, a generic function, 200 swaps under
# a worker thread, the refusals), about two minutes. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh
# (patch-before-jit, old-tracker-removed, summary-ignored, signature-ignored) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: swap.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
cd "$root" || exit 2
python3 "$here/swap.py" "$fibc" "$root"
