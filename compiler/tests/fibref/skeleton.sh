#!/bin/bash
# Builds compiler/tests/fibref/skeleton.fib, which requires every module of the fibref port contract (compiler/fibref, compiler/lsp), runs it and
# prints ok. A module file that skeleton.fib does not list is a failure (a module nothing refers to is never loaded, so never type-checked).
# usage: skeleton.sh [OUT]     FIBC names the fibc to build with (default `fibc` on the PATH); FIB_LIB the library (default the tree's lib/)
# Exit: 0 ok; 1 the build, the run or the module list failed; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "skeleton: no fibc: set FIBC" >&2; exit 2; }
out=${1:-${TMPDIR:-/tmp}/fibref-skeleton-$$}
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
missing=0
for f in $(find compiler/fibref compiler/lsp -name '*.fib' | LC_ALL=C sort); do
  m=${f#compiler/}; m=${m%.fib}; m=${m//\//.}
  if ! grep -q "\[$m :as " compiler/tests/fibref/skeleton.fib; then echo "skeleton: $m is not listed in skeleton.fib" >&2; missing=1; fi
  n=$(wc -l < "$f"); [ "$n" -le 500 ] || { echo "skeleton: $f has $n lines (limit 500)" >&2; missing=1; }
done
[ "$missing" = 0 ] || exit 1
llvmdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
if ! "$fibc" build compiler/tests/fibref/skeleton.fib -I compiler -I lib -L "$llvmdir" -l LLVM-21 -o "$out" 2> "$out.err"; then
  cat "$out.err" >&2; rm -f "$out" "$out.err"; echo "skeleton: FAILED to build" >&2; exit 1
fi
rm -f "$out.err"
got=$("$out"); rc=$?
rm -f "$out"
if [ "$rc" = 0 ] && [ "$got" = ok ]; then echo ok; exit 0; fi
echo "skeleton: FAILED to run (exit $rc, printed: $got)" >&2
exit 1
