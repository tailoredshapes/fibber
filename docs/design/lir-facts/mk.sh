#!/bin/bash
# usage: mk.sh NAME FLAGS..   strip cut-O0.ll with FLAGS over the program and library functions, build v-NAME, check its output
cd /tank/data/fibber-scratch/lirfacts
n=$1; shift
FRE=${FRE:-^(f|l|m)\.}
SRC=${SRC:-cut-O0.ll}
python3 lfstrip.py $SRC v-$n.ll "$FRE" "$@" || exit 1
./build.sh v-$n.ll v-$n 2> v-$n.err || { echo "$n: build failed"; tail -3 v-$n.err; exit 1; }
mkdir -p OUT/$n
ulimit -v 4000000
timeout 60 ./v-$n sheet-healer.png OUT/$n/b happy sneezy resting --size 360 >/dev/null || echo "$n: run failed"
for k in happy sneezy resting; do cmp -s OUT/$n/b-$k.webp OUT/b-$k.webp || echo "$n: output differs ($k)"; done
echo "$n built"
