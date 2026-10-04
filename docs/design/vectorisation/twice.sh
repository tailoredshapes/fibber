#!/bin/bash
# usage: twice.sh PREFIX_IN NAME : runs default<O2> a second time over o/PREFIX_IN.ll (an already optimised module), builds o/NAME.exe, times axpyf axpyi sq
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
in=$1; name=$2
bash opt.sh o/$in.ll $name > /dev/null || exit 1
/usr/lib/llvm-21/bin/llc -O2 -mcpu=raptorlake -filetype=obj -relocation-model=pic o/$name.ll -o o/$name.o || exit 1
cc o/$name.o -o o/$name.exe -lm -lpthread || exit 1
grep "Vectorized f\.\(axpy\|map\)" o/$name.remarks
MODES="axpyf axpyi sq" bash timeit.sh $name
