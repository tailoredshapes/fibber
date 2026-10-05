#!/bin/bash
ulimit -v 16000000
declare -A set=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
cd /home/tmarsh/.cache/fibber-scratch/par/w
flock /tmp/fibsuite.lock bash -c '
declare -A set=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
echo "# taskset -c SET ./sched IMPL W CUT 40 5"
for cut in 5 10 20; do
 for w in 1 2 4 8 16 28; do
  for i in A B; do timeout 120 taskset -c ${set[$w]} ./sched $i $w $cut 40 5; done
 done
done
echo "# micro"; taskset -c 0 ./sched micro
'
