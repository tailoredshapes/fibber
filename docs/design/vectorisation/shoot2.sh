#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
r() { bash bench.sh "$@" 2>&1 | grep -v "^  packed"; }
export NN=4000
r spectral-norm S_base
r spectral-norm S_ovf ovf
r spectral-norm S_ovf_reassoc ovf reassoc
r spectral-norm S_ovf_bounds_reassoc ovf bounds reassoc
r spectral-norm S_uflag_tbaa_rc uflag tbaa rc
r spectral-norm S_all_sound ovf reassoc uflag tbaa rc
export NN=2000000
r n-body N_base
r n-body N_rc rc
r n-body N_uniq uniq
r n-body N_noalias noalias
r n-body N_tbaa tbaa
r n-body N_reassoc reassoc
r n-body N_uflag_tbaa_rc uflag tbaa rc
r n-body N_ovf_bounds_reassoc ovf bounds reassoc
r n-body N_all ovf bounds rc uniq noalias reassoc
