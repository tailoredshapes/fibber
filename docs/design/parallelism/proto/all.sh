#!/bin/bash
# all.sh: every scaling measurement of docs/design/parallelism.md section 2, under one flock, ulimit -v 16000000
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
declare -A cpus=( [1]=0 [2]=0,2 [4]=0,2,4,6 [8]=0,2,4,6,8,10,12,14 [16]=0-15 [28]=0-27 )
declare -A depth=( [1]=0 [2]=1 [4]=2 [8]=3 [16]=4 [28]=5 )
med() { awk "\$1==\"t\"{print \$2}" | tail -n +2 | sort -n | awk "{a[NR]=\$1} END{printf \"%.1f\", a[int((NR+1)/2)]/1e6}"; }
# row LABEL CPUSET CMD..  : median ms of runs 1.. (run 0 is the warmup)
row() { label=$1; cs=$2; shift 2; out=$(taskset -c $cs "$@" 2>&1); echo "$label ms=$(echo "$out" | med) v=$(echo "$out" | awk "\$1==\"v\"{print \$2}" | sort -u | tr "\n" ",") $(echo "$out" | grep -i -m1 -E "trap|cannot|abort")"; }
date; uptime
echo "## sequential baselines (taskset -c 0)"
row "seqsum 1e7" 0 ./p1 seqsum 10000000 0 5
row "seqsum 1e8" 0 ./p1 seqsum 100000000 0 4
row "seqmap 1e7" 0 ./p1 seqmap 10000000 0 4
row "sfib 35" 0 ./p1 sfib 35 0 5
row "seqmsort 1e7 (p3 d=0)" 0 ./p3 10000000 0 4
echo "## spawn and async costs (taskset -c 0; ms for the whole loop)"
row "spawnjoin 20000" 0 ./p1 spawnjoin 20000 0 4
row "asyncjoin 1e6" 0 ./p1 asyncjoin 1000000 0 4
echo "## pmap (thread per element) on 28 cpus"
row "pmap 500" 0-27 ./p1 pmap 500 0 3
row "pmap 1000" 0-27 ./p1 pmap 1000 0 3
row "pmap 4000" 0-27 ./p1 pmap 4000 0 2
echo "## scaling: W tasks on W cpus"
for w in 1 2 4 8 16 28; do row "sum 1e7 W=$w" ${cpus[$w]} ./p1 sum 10000000 $w 5; done
for w in 1 2 4 8 16 28; do row "sum 1e8 W=$w" ${cpus[$w]} ./p1 sum 100000000 $w 4; done
for w in 1 2 4 8 16 28; do row "map 1e7 W=$w" ${cpus[$w]} ./p1 map 10000000 $w 4; done
for w in 1 2 4 8 16 28; do row "for(with-tiles) 1e7 W=$w" ${cpus[$w]} ./p1 for 10000000 $w 4; done
for w in 1 2 4 8 16 28; do row "pfib 35 depth=${depth[$w]} (W=$w)" ${cpus[$w]} ./p1 pfib 35 ${depth[$w]} 5; done
for d in 8 10; do row "pfib 35 depth=$d (W=28)" 0-27 ./p1 pfib 35 $d 5; done
for w in 1 2 4 8 16 28; do row "msort 1e7 depth=${depth[$w]} (W=$w)" ${cpus[$w]} ./p3 10000000 ${depth[$w]} 4; done
row "msort 1e7 depth=7 (W=28)" 0-27 ./p3 10000000 7 4
echo "## tensor"
row "seqmm 1024" 0 ./p4 seqmm 1024 1 4
row "seqmm 2048" 0 ./p4 seqmm 2048 1 3
for w in 1 2 4 8 16 28; do row "mm 1024 W=$w" ${cpus[$w]} ./p4 mm 1024 $w 4; done
for w in 1 2 4 8 16 28; do row "mm 2048 W=$w" ${cpus[$w]} ./p4 mm 2048 $w 3; done
row "seqfma 1e8" 0 ./p4 seqfma 100000000 1 3
for w in 1 2 4 8 16 28; do row "fma 1e8 W=$w" ${cpus[$w]} ./p4 fma 100000000 $w 3; done
row "seqsumfast 1e8" 0 ./p4 seqsumfast 100000000 1 4
for w in 1 2 4 8 16 28; do row "sumfast 1e8 W=$w" ${cpus[$w]} ./p4 sumfast 100000000 $w 4; done
echo "## atoms"
for m in swap-own swap read-own read; do for w in 1 2 4 8 16 28; do row "atom $m T=$w n=200000" ${cpus[$w]} ./p5 $m $w 200000; done; done
date
'
