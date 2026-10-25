#!/bin/bash
# usage: mkc.sh NAME "REGEX:FLAGS" ..   chain lfstrip passes, then build and check like mk.sh
cd /tank/data/fibber-scratch/lirfacts
n=$1; shift
cur=${SRC:-cut-O0.ll}; k=0
for pass in "$@"; do
  k=$((k+1)); re=${pass%%:*}; fl=${pass#*:}
  python3 lfstrip.py $cur v-$n.p$k.ll "$re" $fl || exit 1
  cur=v-$n.p$k.ll
done
cp $cur v-$n.chain.ll
SRC=v-$n.chain.ll ./mk.sh $n
