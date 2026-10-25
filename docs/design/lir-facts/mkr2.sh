#!/bin/bash
# usage: mkr2.sh NAME EXTRA_REGEX EXTRA_FLAGS SPEC..   like mkr.sh with LIBX=inl, then EXTRA_FLAGS over the functions matching EXTRA_REGEX
cd /tank/data/fibber-scratch/lirfacts
n=$1; xre=$2; xfl=$3; shift 3
NTH='^(f\.fib\.prelude\.(vec-nth|vnode-get)\.|m\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec)'
python3 redirect.py ${BASE:-cut-O0.ll} v-$n.r.ll "$@" || exit 1
SRC=v-$n.r.ll ./mkc.sh $n '\.nc$:nthrange' "$NTH:ovfnsw nomatch bounds range vecinv inl" "^(f|l|m)\.:range nomatch" "$xre:$xfl"
