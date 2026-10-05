#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/EXC/a
L=/usr/lib/llvm-21/bin
SO=/usr/lib/x86_64-linux-gnu/libstdc++.so.6
for k in orc mcjit; do
  echo "-- lli --jit-kind=$k"
  $L/lli -jit-kind=$k -load=$SO jit.ll 2>&1 | head -5
  echo "exit=${PIPESTATUS[0]}"
done
$L/llc -relocation-model=pic -filetype=obj jit.ll -o jit.o && g++ jit.o -o jit.bin && ./jit.bin
echo "AOT exit=$?"
$L/lli --version | head -3
