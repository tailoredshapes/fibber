#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
export MODES="sumf sumi dotf doti axpyf axpyi sq"
run() { n=$1; shift; echo "### $n (CPU=$CPU PASSES=${PASSES:-O2}): $*"; bash exp.sh $n "$@" 2>&1 | grep "^  f\.\|Vectorized f\.\(sum\|dot\|axpy\|map\)"| grep -v "packed=0 scalar=0"; bash timeit.sh $n; }
for cpu in x86-64 x86-64-v2 x86-64-v3 raptorlake; do
  CPU=$cpu run cpu_$cpu ovf bounds rc uniq noalias reassoc
done
CPU=raptorlake PASSES='default<O3>' run o3_all ovf bounds rc uniq noalias reassoc
CPU=raptorlake PASSES='default<O3>' run o3_base
