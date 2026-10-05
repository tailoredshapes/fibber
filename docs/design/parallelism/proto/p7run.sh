#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
declare -A cpus=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
med() { awk "\$1==\"t\"{print \$2}" | tail -n +2 | sort -n | awk "{a[NR]=\$1} END{printf \"%.1f\", a[int((NR+1)/2)]/1e6}"; }
date
echo "# taskset -c SET ./p7 10000000 W H 5   (with-tiles only; the owner array is made outside the timed region)"
for h in 0 30; do for w in 1 2 4 8 16 28; do
  out=$(taskset -c ${cpus[$w]} ./p7 10000000 $w $h 5 2>&1)
  echo "with-tiles n=1e7 H=$h W=$w ms=$(echo "$out" | med) $(echo "$out" | grep -m1 -i trap)"
done; done
date
'
