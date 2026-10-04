#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
name=$1; n=${2:-100000}; reps=${3:-2000}
modes=${MODES:-sumf sumi dotf doti axpyf axpyi mapf sq}
for m in $modes; do
  best=999999999
  for r in 1 2 3; do
    t=$(./o/$name.exe $m $n $reps | sed -n 's/.*ms=\([0-9]*\).*/\1/p')
    [ "$t" -lt "$best" ] && best=$t
  done
  echo "$name $m n=$n reps=$reps best_ms=$best"
done
