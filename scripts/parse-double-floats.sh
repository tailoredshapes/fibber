#!/bin/bash
# The float verification of parse-double (stdlib 4.7, package FOLLOWUP-1). Needs a stage 2 (FIBC, default `fibc`) and python3.
#   1. N random decimal texts of every shape parsed by parse-double and by libc strtod: the bits must be equal   (N default 1000000)
#   2. the edge corpus of scripts/json-floats.py (halfway points between doubles, nudged neighbours, subnormals, boundaries, long mantissas; expected bits from Python's float())
# usage: scripts/parse-double-floats.sh [N] [CORPUS_N]. Exits 1 on any difference.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$here/..
N=${1:-1000000}; CN=${2:-40000}
FIBC=${FIBC:-fibc}; work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
FIB_LIB=$root/lib "$FIBC" build "$here/parse-double-floats.fib" -I "$root/lib" -o "$work/pdf" || exit 2
rc=0
"$work/pdf" "$N" || rc=1
python3 "$here/json-floats.py" "$CN" > "$work/corpus.txt" && "$work/pdf" "$work/corpus.txt" || rc=1
exit $rc
