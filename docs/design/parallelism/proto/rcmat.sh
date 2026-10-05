#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
declare -A cpus=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
date
echo "# taskset -c SET ./rc MODE K T 2000000   (ns per retain+release pair per thread; ideal = flat)"
for cfg in "p 64" "f 64" "c 64" "c 1" "a 64" "a 1" "P 64" "P 1"; do
  set -- $cfg
  line="$1 K=$2:"
  for w in 1 2 4 8 16 28; do
    v=$(taskset -c ${cpus[$w]} ./rc $1 $2 $w 2000000 | sed "s/.*per_thread=//")
    line="$line T$w=$v"
  done
  echo "$line"
done
date
'
