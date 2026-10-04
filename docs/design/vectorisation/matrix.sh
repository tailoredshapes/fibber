#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
run() { n=$1; shift; echo "### $n: $*"; bash exp.sh $n "$@" 2>&1 | grep -v write-str; bash timeit.sh $n; }
run e_uniq uniq
run e_noalias noalias
run e_rc_uniq rc uniq
run e_bounds_rc_uniq bounds rc uniq
run e_bounds_rc_uniq_noalias bounds rc uniq noalias
run e_all ovf bounds rc uniq noalias reassoc
run e_all_noreassoc ovf bounds rc uniq noalias
