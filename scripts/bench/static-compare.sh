#!/bin/bash
# The static build against the dynamic one (docs/design/static-linking.md 7): the programs of scripts/bench/quick.sh, plus the multi-threaded ones (binary-trees, atom-contention,
# parallel-closure), each built twice with the same CPU (x86-64-v3, the CPU a --static build uses) and the same code, once linked to glibc (`fibc build`) and once as a fully static
# musl executable (`fibc build --static`); the median wall time of N runs of each, the delta, and whether the two printed the same (a wrong answer is a failure).
#   scripts/bench/static-compare.sh [-n RUNS] [NAME..]      FIBC=the compiler  FIB_MUSL_DIR=the musl pieces  (both required)
# Alone: takes /tmp/fibsuite.lock like quick.sh. Does not touch baseline.tsv.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=3
[ "${1:-}" = -n ] && { runs=${2:?-n needs a number}; shift 2; }
: "${FIBC:?FIBC must name a stage 2 fibc}"; : "${FIB_MUSL_DIR:?FIB_MUSL_DIR must name the musl pieces}"
# shellcheck source=../lib/stage2.sh
. "$here/../lib/stage2.sh"
take_suite_lock
names=${*:-num-f64 num-nbody vec-conj-pop vec-index vec-sort map-assoc-get set-conj lazy-fused lazy-bound strings binary-trees atom-contention parallel-closure}
work=$GATE_OUT/static-compare; mkdir -p "$work"
export FIB_TARGET_CPU=x86-64-v3 FIB_LIB=$root/lib
median() { sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
timeit() { # timeit EXE: seconds of one run, output in $work/out
  local t0 t1; t0=$(date +%s.%N); "$1" > "$work/out" 2> /dev/null; t1=$(date +%s.%N); echo "$t1 - $t0" | bc -l; }
printf '%-18s %9s %9s %8s  %s\n' benchmark dynamic static delta answer
bad=0
for n in $names; do
  src=$root/scripts/bench/$n.fib
  [ -f "$src" ] || { echo "$n: no such benchmark"; continue; }
  "$FIBC" build "$src" -I "$root/lib" -o "$work/$n.dyn" > "$work/$n.log" 2>&1 || { echo "$n: dynamic build failed"; bad=1; continue; }
  "$FIBC" build --static "$src" -I "$root/lib" -o "$work/$n.sta" >> "$work/$n.log" 2>&1 || { echo "$n: static build failed"; bad=1; continue; }
  d=; s=
  for _ in $(seq "$runs"); do d="$d $(timeit "$work/$n.dyn")"; done; sum_d=$(sha1sum < "$work/out")
  for _ in $(seq "$runs"); do s="$s $(timeit "$work/$n.sta")"; done; sum_s=$(sha1sum < "$work/out")
  md=$(echo $d | tr ' ' '\n' | median); ms=$(echo $s | tr ' ' '\n' | median)
  same=same; [ "$sum_d" = "$sum_s" ] || { same="DIFFERENT ANSWER"; bad=1; }
  printf '%-18s %8.3fs %8.3fs %+7.1f%%  %s\n' "$n" "$md" "$ms" "$(echo "($ms - $md) * 100 / $md" | bc -l)" "$same"
done
exit $bad
