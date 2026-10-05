#!/bin/bash
# usage: proto.sh NAME (NAME.fib in c/): emit, patch, build (original and patched), run with FIB_TRACE and count A/F lines
ulimit -v 16000000
export FIB_LIB=/home/tmarsh/.cache/fibber-scratch/EXC/W/lib
cd /home/tmarsh/.cache/fibber-scratch/EXC/c
n=$1
../F emit $n.fib > $n.lir || exit 1
python3 patch.py $n.lir $n.p.lir || exit 1
../lairf build $n.lir -o $n.orig -O 2 2>&1 | head -3
../lairf build $n.p.lir -o $n.proto -O 2 2>&1 | head -3
for v in orig proto; do
  echo "--- $n.$v"
  FIB_TRACE=1 ./$n.$v > $n.$v.out 2> $n.$v.trace
  echo "exit=$?  stdout: $(head -c 80 $n.$v.out | tr '\n' ' ')"
  echo "stderr non-trace: $(grep -v -E '^[ADFST]( |$)' $n.$v.trace | head -3 | tr '\n' '|')"
  echo "A lines: $(grep -c '^A ' $n.$v.trace)  F lines: $(grep -c '^F ' $n.$v.trace)"
done
