#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
export MODES="axpyf axpyi sq"
run() { n=$1; shift; echo "### $n: $*"; bash exp.sh $n "$@" 2>&1 | grep "axpy-f64\|map-sq\|Vectorized f.axpy-f64"; bash timeit.sh $n; }
run l_no_bounds rc uniq noalias
run l_no_rc bounds uniq noalias
run l_no_uniq bounds rc noalias
run l_no_noalias bounds rc uniq
run l_uniq_noalias uniq noalias
run l_rc_bounds_noalias rc bounds noalias
