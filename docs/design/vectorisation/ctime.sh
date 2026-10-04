#!/bin/bash
# compile time of the optimisation pipeline over a big module: the compiler's own lIR (fibc.fib) is the largest program available; falls back to kern.ll
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
f=${1:-kern.ll}
wc -l $f
TIMEFORMAT='%R s wall %U s user'
for i in 1 2 3; do
  echo -n "O2 once:  "; { time /usr/lib/llvm-21/bin/opt -passes='default<O2>' -mcpu=raptorlake -disable-output $f; } 2>&1
done
/usr/lib/llvm-21/bin/opt -passes='default<O2>' -mcpu=raptorlake -S $f -o o/ct1.ll
for i in 1 2 3; do
  echo -n "O2 twice: "; { time (/usr/lib/llvm-21/bin/opt -passes='default<O2>' -mcpu=raptorlake -S $f | /usr/lib/llvm-21/bin/opt -passes='default<O2>' -mcpu=raptorlake -disable-output); } 2>&1
done
for i in 1 2 3; do
  echo -n "O3 once:  "; { time /usr/lib/llvm-21/bin/opt -passes='default<O3>' -mcpu=raptorlake -disable-output $f; } 2>&1
done
