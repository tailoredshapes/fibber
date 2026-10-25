#!/bin/bash
# usage: mkr.sh NAME SPEC..   the `lib` facts everywhere, the chosen nth calls to an unchecked clone, build and check
cd /tank/data/fibber-scratch/lirfacts
n=$1; shift
NTH='^(f\.fib\.prelude\.(vec-nth|vnode-get)\.|m\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec)'
python3 redirect.py ${BASE:-cut-O0.ll} v-$n.r.ll "$@" || exit 1
SRC=v-$n.r.ll ./mkc.sh $n '\.nc$:nthrange' "$NTH:ovfnsw nomatch bounds range vecinv ${LIBX}" "^(f|l|m)\.:range nomatch"
