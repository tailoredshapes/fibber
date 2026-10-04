#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
export MODES="axpyf axpyi sq"
run() { n=$1; shift; echo "### $n: $*"; bash exp.sh $n "$@" 2>&1 | grep "axpy-f64\|map-sq\|Vectorized f.axpy-f64\|Vectorized f.map-sq"; bash timeit.sh $n; }
run t_tbaa tbaa
run t_tbaa_uniq tbaa uniq
run t_tbaa_bounds_uniq tbaa bounds uniq
run t_tbaa_rc_uniq tbaa rc uniq
run t_tbaa_bounds_rc_uniq tbaa bounds rc uniq
run t_tbaa_bounds_rc tbaa bounds rc
