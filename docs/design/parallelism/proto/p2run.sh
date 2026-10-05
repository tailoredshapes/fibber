#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
declare -A set=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
echo "# taskset -c SET ./p2 MODE K T 2000000   (T threads, each 2e6 iterations; median of 3 runs, ms; ideal = flat)"
for cfg in "private 64" "shared 1" "shared 64" "static 1" "none 64"; do
  set -- $cfg
  for w in 1 2 4 8 16 28; do
    med=$(taskset -c ${set[$w]} ./p2 $1 $2 $w 2000000 | awk "\$1==\"t\"{print \$2}" | sort -n | sed -n 2p)
    echo "$1 K=$2 T=$w ms=$(echo "$med/1000000" | bc -l | cut -c1-6)"
  done
done
'
