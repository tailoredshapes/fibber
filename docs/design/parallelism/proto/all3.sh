#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
med() { awk "\$1==\"t\"{print \$2}" | tail -n +2 | sort -n | awk "{a[NR]=\$1} END{printf \"%.1f\", a[int((NR+1)/2)]/1e6}"; }
row() { label=$1; cs=$2; shift 2; out=$(taskset -c $cs "$@" 2>&1); echo "$label ms=$(echo "$out" | med)"; }
date
echo "## one thread of each kind: seqsum 1e8 (taskset -c CPU ./p1 seqsum 100000000 0 5) and sfib 35"
row "seqsum on cpu0 (P, SMT sibling idle)" 0 ./p1 seqsum 100000000 0 5
row "seqsum on cpu16 (E)" 16 ./p1 seqsum 100000000 0 5
row "sfib35 on cpu0 (P)" 0 ./p1 sfib 35 0 5
row "sfib35 on cpu16 (E)" 16 ./p1 sfib 35 0 5
echo "## two threads: distinct P cores (0,2) vs SMT siblings (0,1) vs two E (16,17): sum 1e8, W=2"
row "sum 1e8 W=2 on 0,2" 0,2 ./p1 sum 100000000 2 4
row "sum 1e8 W=2 on 0,1 (siblings)" 0,1 ./p1 sum 100000000 2 4
row "sum 1e8 W=2 on 16,17 (E)" 16,17 ./p1 sum 100000000 2 4
echo "## P cores only vs E only vs all, 16 tasks of equal size on 8 P / 12 E / 28"
row "sfib-like: sum 1e8 W=16 on 0-15 (8P x2 SMT)" 0-15 ./p1 sum 100000000 16 4
row "sum 1e8 W=16 on 0,2,..14 + 16-23 (8P+8E)" 0,2,4,6,8,10,12,14,16,17,18,19,20,21,22,23 ./p1 sum 100000000 16 4
row "sum 1e8 W=12 on 16-27 (12E)" 16-27 ./p1 sum 100000000 12 4
row "sum 1e8 W=20 on 0,2,..14 + 16-27 (8P+12E, no SMT)" 0,2,4,6,8,10,12,14,16-27 ./p1 sum 100000000 20 4
row "sum 1e8 W=28 on all" 0-27 ./p1 sum 100000000 28 4
row "sum 1e8 W=112 on all (over-decomposed 4x)" 0-27 ./p1 sum 100000000 112 4
row "sum 1e8 W=20 on 8P+12E, 4x over-decomposed: W=80" 0,2,4,6,8,10,12,14,16-27 ./p1 sum 100000000 80 4
echo "## share-marking walk"
taskset -c 0 ./p6 1000000
taskset -c 0 ./p6 10000
date
'
