#!/bin/bash
# usage: mk2.sh NAME "LIBFLAGS" "ALLFLAGS"   LIBFLAGS over the Vec read path (vec-nth, vnode-get, Reducible.nth) only, ALLFLAGS over every program and library function
cd /tank/data/fibber-scratch/lirfacts
n=$1
python3 lfstrip.py ${SRC:-cut-O0.ll} v-$n.pre.ll '^(f\.fib\.prelude\.(vec-nth|vnode-get)\.|m\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec)' $2 || exit 1
SRC=v-$n.pre.ll ./mk.sh $n $3
