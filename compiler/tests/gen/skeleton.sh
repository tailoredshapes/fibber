#!/bin/bash
# Builds compiler/tests/gen/skeleton.fib (requires every module of compiler/gen) with a fibc, runs it, prints ok. A module of compiler/gen that
# skeleton.fib does not list is a failure. usage: skeleton.sh [OUT]   FIBC names the fibc (default fibc); FIB_LIB the library (default the tree's lib/).
# Exit: 0 ok; 1 failed; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "skeleton: no fibc: set FIBC" >&2; exit 2; }
out=${1:-${TMPDIR:-/tmp}/gen-skeleton-$$}
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
missing=0
for f in $(find compiler/gen -name '*.fib' | LC_ALL=C sort); do
  m=${f#compiler/}; m=${m%.fib}; m=${m//\//.}
  grep -q "\[$m :as " compiler/tests/gen/skeleton.fib || { echo "skeleton: $m is not listed in skeleton.fib" >&2; missing=1; }
done
[ "$missing" = 0 ] || exit 1
if ! "$fibc" build compiler/tests/gen/skeleton.fib -I compiler -I lib -o "$out" 2> "$out.err"; then
  cat "$out.err" >&2; rm -f "$out" "$out.err"; echo "skeleton: FAILED to build" >&2; exit 1
fi
rm -f "$out.err"
got=$("$out"); rc=$?
rm -f "$out"
[ "$rc" = 0 ] && [ "$got" = ok ] && { echo ok; exit 0; }
echo "skeleton: FAILED to run (exit $rc, printed: $got)" >&2
exit 1
