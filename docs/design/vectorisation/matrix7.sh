#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
export MODES="mapf sq"
export SRC=kern.ll FRE='^f\.'
run() { n=$1; shift; echo "### $n (functions inlined as the compiler leaves them): $*"; bash exp.sh $n "$@" 2>&1 | grep "Vectorized"; bash timeit.sh $n; }
run i_base
run i_all_nonoalias ovf bounds rc uniq reassoc tbaa
run i_uflag_tbaa_rc uflag tbaa rc
