#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
r() { bash bench.sh "$@" 2>&1 | grep -v "^  packed"; }
unset NN
r fannkuch-redux F_uniq uniq
r fannkuch-redux F_rc rc
r fannkuch-redux F_bounds bounds
r fannkuch-redux F_uniq_bounds uniq bounds
r fannkuch-redux F_tbaa tbaa
r fannkuch-redux F_uflag_tbaa uflag tbaa
export NN=2000000
r n-body N_uflag uflag
r n-body N_uflag_tbaa uflag tbaa
r n-body N_uniq_tbaa uniq tbaa
r n-body N_uniq_ovf_bounds_reassoc uniq ovf bounds reassoc
