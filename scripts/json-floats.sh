#!/bin/bash
# The float verification of fib.json at scale (docs/design/json.md 6). Needs a stage 2 (FIBC, default `fibc`) and python3. Prints one result line per check; exits 1 on any difference.
#   1. N random decimal texts of every shape parsed by fib.json and by libc strtod: the bits must be equal            (N default 1000000)
#   2. N random doubles written by fib.json and read back by strtod: the bits must be equal
#   3. an edge corpus (halfway points between doubles, nudged neighbours, subnormals, boundaries, long mantissas; expected bits from Python's float()): scripts/json-floats.py
#   4. M doubles of several shapes written by fib.json: each must read back to the same bits and be, digit for digit, Python's repr (the shortest closest text): scripts/json-floats-check.py
# usage: scripts/json-floats.sh [N] [CORPUS_N] [M]
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$here/..
N=${1:-1000000}; CN=${2:-40000}; M=${3:-2000000}
FIBC=${FIBC:-fibc}; work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
FIB_LIB=$root/lib "$FIBC" build "$here/json-floats.fib" -I "$root/lib" -o "$work/jf" || exit 2
rc=0
"$work/jf" "$N" || rc=1
python3 "$here/json-floats.py" "$CN" > "$work/corpus.txt" && "$work/jf" "$work/corpus.txt" || rc=1
"$work/jf" print "$M" | python3 "$here/json-floats-check.py" || rc=1
exit $rc
