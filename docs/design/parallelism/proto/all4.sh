#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/par/w
exec flock /tmp/fibsuite.lock bash -c '
ulimit -v 16000000
date
echo "## pmap, one thread per element, 28 cpus, ulimit -v 16000000, timeout 100 s: ./p1 pmap N 0 2"
for n in 20000 100000 1000000; do
  s=$(date +%s.%N); out=$(timeout 100 ./p1 pmap $n 0 2 2>&1); rc=$?
  echo "pmap $n: rc=$rc wall=$(echo "$(date +%s.%N) - $s" | bc | cut -c1-6) s; $(echo "$out" | tr "\n" " " | cut -c1-200)"
done
date
'
