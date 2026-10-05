#!/bin/bash
# scripts/mutant-call-pos.sh: planted faults for the core form `(call-pos)` (compiler/expand/overload.fib `pick-call`; case 7533, and the
# `file:line:col` check of compiler/tests/harness-proto/run.sh). Copies compiler/ and lib/ to a scratch directory, breaks ONE rule in the
# copy, builds a stage 2 from the copy with FIBC (the seed or a stage 2) and runs the checks that pin the rule: each must FAIL.
# usage: FIBC=fibc scripts/mutant-call-pos.sh MODE
#   MODE    what is broken                          what must fail
#   col     the column is one too high               case 7533 and harness-proto run.sh check 7
#   line    the line is one too high                 case 7533 and harness-proto run.sh check 7
# environment: FIBC (required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-call-pos-MODE). Run under ulimit -v 16000000.
# exit: 0 when the case failed under the mutant, 1 when it survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-call-pos.sh col|line}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-call-pos-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-call-pos: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
ulimit -v 16000000
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
f=$OUT/tree/compiler/expand/overload.fib
case $MODE in
  col)  perl -0pi -e 's/\(show \(\. pos col\)\)/(show (+ (. pos col) 1))/' "$f" ;;
  line) perl -0pi -e 's/\(show \(\. pos line\)\)/(show (+ (. pos line) 1))/' "$f" ;;
  *) echo "mutant-call-pos: unknown MODE $MODE" >&2; exit 2 ;;
esac
cmp -s "$f" "$R/compiler/expand/overload.fib" && { echo "mutant-call-pos: the mutation changed nothing" >&2; exit 2; }
(cd "$OUT/tree" && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F") > "$OUT/build.log" 2>&1 \
  || { tail -5 "$OUT/build.log"; echo "mutant-call-pos: the mutant did not build" >&2; exit 2; }
export FIB_LIB=$OUT/tree/lib
line=$(cd "$OUT/tree" && "$OUT/F" cases cases/stdlib --only 7533- 2>&1 | grep -E '^7533-' | head -1)
case $line in
  *" pass"*) echo "SURVIVED  $MODE: $line"; exit 1 ;;
  *) echo "killed    $MODE: ${line:0:160}"; exit 0 ;;
esac
