#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
declare -A cpus=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
med() { awk "\$1==\"t\"{print \$2}" | tail -n +2 | sort -n | awk "{a[NR]=\$1} END{printf \"%.1f\", a[int((NR+1)/2)]/1e6}"; }
row() { label=$1; cs=$2; shift 2; out=$(taskset -c $cs "$@" 2>&1); echo "$label ms=$(echo "$out" | med) v=$(echo "$out" | awk "\$1==\"v\"{print \$2}" | sort -u | tr "\n" ",")"; }
date
row "seqsumfast 1e8 (inexact data)" 0 ./p4 seqsumfast 100000000 1 4
for w in 1 2 4 8 16 28; do row "sumfast 1e8 W=$w" ${cpus[$w]} ./p4 sumfast 100000000 $w 4; done
for w in 1 2 4 8 16 28; do row "dsumfast 1e8 W=$w" ${cpus[$w]} ./p4 dsumfast 100000000 $w 4; done
echo "## p2 refcount (again with exclusive machine)"
date
'
