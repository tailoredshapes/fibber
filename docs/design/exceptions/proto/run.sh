#!/bin/bash
# usage: run.sh DEPTH  : builds and runs every mode with and without a live owned object
ulimit -v 16000000
cd /home/tmarsh/.cache/fibber-scratch/EXC/a
L=/usr/lib/llvm-21/bin
D=${1:-10}
for alloc in 1 0; do
  for mode in plain invoke result flag jmp; do
    n=chain_${mode}_$alloc
    python3 gen.py $mode $alloc $D > $n.ll
    $L/opt -passes='default<O2>' $n.ll -S -o $n.opt.ll || exit 1
    $L/llc -O2 -relocation-model=pic -filetype=obj $n.opt.ll -o $n.o || exit 1
    g++ -O2 -DMODE_$mode drv.cpp $n.o -o $n || exit 1
    sz=$($L/llvm-size -A $n.o | awk '/^\.text/ {t=$2} /eh_frame/ {e=$2} /gcc_except_table/ {g=$2} END {printf "text=%d eh_frame=%d except_table=%d", t, e, g}')
    echo "== alloc=$alloc mode=$mode depth=$D  [$sz]"
    ./$n 20000000 200000
  done
done
