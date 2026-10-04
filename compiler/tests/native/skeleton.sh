#!/bin/bash
# Builds compiler/tests/native/skeleton.fib, a program that requires every module of the lair-in-fibber skeleton (compiler/lir, compiler/native),
# with a seed fibc, runs it and prints ok. A module of the tree that skeleton.fib does not list is a failure: a module nothing refers to is never
# loaded, so an unlisted module would not be type-checked.
# usage: skeleton.sh [OUT]        (from anywhere; FIBC names the fibc to build with, default `fibc` on the PATH; FIB_LIB the library, default the tree's lib/)
# Exit: 0 ok; 1 the build, the run or the module list failed; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "skeleton: no fibc: set FIBC" >&2; exit 2; }
out=${1:-${TMPDIR:-/tmp}/skeleton-$$}
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2

# every module file of compiler/lir, compiler/native and compiler/lir.fib must be listed in the program
missing=0
for f in compiler/lir.fib $(find compiler/lir compiler/native -name '*.fib' | LC_ALL=C sort); do
  m=${f#compiler/}; m=${m%.fib}; m=${m//\//.}
  if ! grep -q "\[$m :as " compiler/tests/native/skeleton.fib; then echo "skeleton: $m is not listed in skeleton.fib" >&2; missing=1; fi
done
[ "$missing" = 0 ] || exit 1

if ! "$fibc" build compiler/tests/native/skeleton.fib -I compiler -I lib -o "$out" 2> "$out.err"; then
  cat "$out.err" >&2; rm -f "$out" "$out.err"; echo "skeleton: FAILED to build" >&2; exit 1
fi
rm -f "$out.err"
got=$("$out"); rc=$?
rm -f "$out"
if [ "$rc" = 0 ] && [ "$got" = ok ]; then echo ok; exit 0; fi
echo "skeleton: FAILED to run (exit $rc, printed: $got)" >&2
exit 1
