#!/bin/bash
# Builds and runs the unit programs of package L1 (compiler/tests/native/l1-unit-*.fib) with a seed fibc; every line must be `ok`.
# usage: l1-unit.sh        (FIBC names the fibc, default `fibc`; LLVMLIB the directory of libLLVM-21.so, default /usr/lib/llvm-21/lib)
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root" || exit 2
export FIB_LIB=${FIB_LIB:-$root/lib}
rc=0
for f in compiler/tests/native/l1-unit-*.fib; do
  out=${TMPDIR:-/tmp}/l1-unit-$$
  "${FIBC:-fibc}" build "$f" -I compiler -L "${LLVMLIB:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$out" || { echo "FAILED to build $f"; rc=1; continue; }
  "$out" | tee "$out.txt"
  if grep -q '^FAIL' "$out.txt"; then rc=1; fi
  rm -f "$out" "$out.txt"
done
exit $rc
