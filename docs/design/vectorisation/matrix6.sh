#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
export MODES="axpyf axpyi sq"
run() { n=$1; shift; echo "### $n: $*"; bash exp.sh $n "$@" 2>&1 | grep "axpy-f64\|map-sq\|Vectorized f.axpy-f64\|Vectorized f.map-sq"; bash timeit.sh $n; }
run u_uflag uflag
run u_uflag_tbaa uflag tbaa
run u_uflag_tbaa_rc uflag tbaa rc
