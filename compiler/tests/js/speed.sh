#!/bin/bash
# The speed of the JavaScript backend against the native build of the same lIR (docs/design/js-backend.md, "Speed").
#   compiler/tests/js/speed.sh [NAME=FILE.fib:ARGS]..     (default: the programs below)
# For each program: `$FIBC emit` once, then `lairf build -O 2` (the native executable) and `lir2js` (the ES module); each runs 3 times
# and the median wall time is printed with the ratio, and both outputs must be the same (a wrong answer is not a speed).
# Environment: FIBC, LAIRF, LIR2JS, NODE (default node). Holds /tmp/fibsuite.lock while it runs, as the benchmarks of scripts/bench do.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
: "${FIBC:?}" "${LAIRF:?}" "${LIR2JS:?}"; NODE=${NODE:-node}
progs=("$@")
[ ${#progs[@]} -eq 0 ] && progs=("n-body=scripts/shootout/n-body/n-body.fib:1000000" "binary-trees=scripts/bench/binary-trees.fib:"
  "num-f64=scripts/bench/num-f64.fib:" "vec-sort=scripts/bench/vec-sort.fib:" "strings=scripts/bench/strings.fib:")
work=$(mktemp -d "${TMPDIR:-/tmp}/jsspeed.XXXXXX"); trap 'rm -rf "$work"' EXIT
exec 9> /tmp/fibsuite.lock; flock 9
median() { sort -n | sed -n 2p; }
timed() { local t0 t1; t0=$(date +%s.%N); "$@" > "$work/out.$$" 2>&1; t1=$(date +%s.%N); echo "$t1 - $t0" | bc; }
printf '%-14s %10s %10s %8s  %s\n' program native_s node_s ratio output
for p in "${progs[@]}"; do
  name=${p%%=*}; rest=${p#*=}; file=${rest%%:*}; args=${rest#*:}
  (cd "$root" && "$FIBC" emit "$file") > "$work/$name.lir" || { echo "$name: emit failed"; continue; }
  "$LAIRF" build "$work/$name.lir" -o "$work/$name.native" -O 2 || { echo "$name: native build failed"; continue; }
  "$LIR2JS" "$work/$name.lir" -o "$work/$name.mjs" --rt "$root/compiler/js/rt" || { echo "$name: lir2js failed"; continue; }
  # shellcheck disable=SC2086
  tn=$(for i in 1 2 3; do timed "$work/$name.native" $args; done | median); cp "$work/out.$$" "$work/$name.nout"
  # shellcheck disable=SC2086
  tj=$(for i in 1 2 3; do timed "$NODE" --stack-size=7000 "$work/$name.mjs" $args; done | median); cp "$work/out.$$" "$work/$name.jout"
  same=$(cmp -s "$work/$name.nout" "$work/$name.jout" && echo same || echo DIFFERENT)
  printf '%-14s %10.3f %10.3f %8.1f  %s\n' "$name" "$tn" "$tj" "$(echo "$tj / $tn" | bc -l)" "$same"
done
