#!/bin/bash
# usage: tphase.sh ROUNDS NAME..  per-phase medians (ms) of the timed variants
cd /tank/data/fibber-scratch/lirfacts
r=$1; shift
ulimit -v 4000000
rm -f phase.raw
for k in $(seq $r); do for n in "$@"; do
  taskset -c ${CORE:-6} timeout 60 ./v-$n sheet-healer.png OUT/$n/b happy sneezy resting --size 360 2>&1 >/dev/null | grep '^T ' | sed "s/^/$n $k /" >> phase.raw
done; done
python3 - "$@" <<'P'
import sys, statistics, collections
d = collections.defaultdict(lambda: collections.defaultdict(float))
for l in open('phase.raw'):
    n, k, _, ph, ms = l.split()
    d[(n, ph)][k] += float(ms)
phases = ['png', 'bgmask', 'alpha', 'regions', 'sprite-src', 'resample', 'sprite-out', 'encode+write']
print('variant      ' + ' '.join(f'{p:>10s}' for p in phases) + '      sum')
for n in sys.argv[1:]:
    meds = [statistics.median(d[(n, p)].values()) for p in phases]
    print(f'{n:12s} ' + ' '.join(f'{m:10.0f}' for m in meds) + f' {sum(meds):8.0f}')
P
