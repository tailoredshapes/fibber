#!/bin/bash
# Planted faults for the heap unit programs: breaks one rule of compiler/fibref at a time in a scratch copy (wrong cascade order, double free undetected,
# a leak class misclassified, a cycle missed, a write on a shared object accepted, ...) and shows that heap.sh's programs FAIL on each. A mutant that no
# unit program fails is a gap in the tests. The scratch copies live under SCRATCH (default ~/.cache/fibber-scratch/heap-mutants), not in the tree.
# usage: heap-mutants.sh [NAME-PREFIX ..]    FIBC the fibc to build with (default `fibc`); FIB_LIB the library (default the tree's lib/); JOBS (default 2)
# Exit: 0 every mutant killed; 1 a mutant survived or is invalid; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "heap-mutants: no fibc: set FIBC" >&2; exit 2; }
scratch=${SCRATCH:-$HOME/.cache/fibber-scratch/heap-mutants}
ulimit -v 16000000
exec python3 "$here/heap-mutants.py" "$root" "$fibc" "$scratch" -j "${JOBS:-2}" "$@"
