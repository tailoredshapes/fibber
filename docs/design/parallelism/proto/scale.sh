#!/bin/bash
# scale.sh BIN KERNEL N [RUNS] : median ms (runs 1..) for W = 1 2 4 8 16 28 workers (k = W tasks, taskset to W logical CPUs)
ulimit -v 16000000
bin=$1; kern=$2; n=$3; runs=${4:-5}
declare -A set=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
for w in ${WS:-1 2 4 8 16 28}; do
  out=$(flock /tmp/fibsuite.lock taskset -c ${set[$w]} $bin $kern $n $w $runs 2>&1)
  med=$(echo "$out" | awk '$1=="t"{print $2}' | tail -n +2 | sort -n | awk '{a[NR]=$1} END{printf "%.1f", a[int((NR+1)/2)]/1e6}')
  v=$(echo "$out" | awk '$1=="v"{print $2}' | sort -u | tr '\n' ',')
  echo "$kern n=$n W=$w ms=$med v=$v"
done
