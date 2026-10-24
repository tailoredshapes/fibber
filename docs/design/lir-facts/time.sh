#!/bin/bash
# usage: time.sh ROUNDS NAME..  interleaved runs pinned to one core; prints median of ROUNDS per variant (seconds, wall)
cd /tank/data/fibber-scratch/lirfacts
r=$1; shift
ulimit -v 4000000
uptime
rm -f times.raw
for k in $(seq $r); do for n in "$@"; do
  t=$( { /usr/bin/time -f "%e" taskset -c ${CORE:-6} timeout 60 ./v-$n sheet-healer.png OUT/$n/b happy sneezy resting --size 360 >/dev/null; } 2>&1 | tail -1)
  echo "$n $t" >> times.raw
done; done
python3 - "$@" <<'P'
import sys, statistics
d = {}
for l in open('times.raw'):
    n, t = l.split(); d.setdefault(n, []).append(float(t))
b = statistics.median(d['base']) if 'base' in d else None
for n in sys.argv[1:]:
    m = statistics.median(d[n])
    print(f'{n:10s} {m:.3f}  {" ".join(f"{x:.2f}" for x in d[n])}' + (f'  {b/m:.2f}x' if b else ''))
P
uptime
