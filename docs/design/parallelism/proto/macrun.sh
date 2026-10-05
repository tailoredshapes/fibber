#!/bin/bash
# runs on the Mac (bash): rc and sched on the M1 Ultra (20 cores: 16P+4E), no installs, scratch dir only
cd ~/fibber-a64-scratch/par
cc -O2 -o rc rc.c -lpthread || exit 1
echo "# M1 Ultra ./rc MODE K T 2000000 (ns per retain+release pair per thread)"
for cfg in "p 64" "f 64" "c 64" "a 64" "a 1" "P 1"; do
  set -- $cfg; line="$1 K=$2:"
  for t in 1 2 4 8 16 20; do v=$(./rc $1 $2 $t 2000000 | sed "s/.*per_thread=//"); line="$line T$t=$v"; done
  echo "$line"
done
echo "# M1 Ultra ./sched IMPL 1 CUT 40 5"
./sched A 1 10 40 5; ./sched B 1 10 40 5; ./sched A 1 20 40 5; ./sched B 1 20 40 5; ./sched micro
