#!/bin/bash
# Builds and runs one unit test file of the fibgen port: compiler/tests/gen/unit-NAME.fib prints `ok` and exits 0, or prints FAIL lines and exits non-zero.
# usage: unit.sh NAME [OUTDIR]     FIBC names the fibc (a stage 2 from the tree); FIB_LIB the library.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
name=${1:?usage: unit.sh NAME}
export FIB_LIB=${FIB_LIB:-$root/lib}
out=${2:-${TMPDIR:-/tmp}}/gen-unit-$name-$$
cd "$root" || exit 2
"$fibc" build "compiler/tests/gen/unit-$name.fib" -I compiler -I lib -o "$out" 2> "$out.err" || { cat "$out.err" >&2; rm -f "$out" "$out.err"; echo "unit-$name: FAILED to build" >&2; exit 2; }
"$out"; rc=$?
rm -f "$out" "$out.err"
exit $rc
