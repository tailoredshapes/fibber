#!/bin/bash
# Builds and runs the heap unit programs of package F1 (compiler/tests/fibref/heap-{core,modes,inplace,adv,fuzz}.fib): each drives fibref.heap directly and
# prints one summary line "NAME: N tests, M failed" (and one FAIL line per failing test); its exit status is the number of failures.
# usage: heap.sh [OVERLAY]    FIBC names the fibc to build with (default `fibc` on the PATH); FIB_LIB the library (default the tree's lib/)
#   OVERLAY is a directory searched before compiler/ (-I): the mutation script (heap-mutants.sh) puts a mutated copy of compiler/fibref there.
# Exit: 0 every test of every program passed; 1 a build or a test failed; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "heap.sh: no fibc: set FIBC" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
overlay=${1:-}
llvmdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/heap-sh-XXXXXX")
trap 'rm -rf "$tmp"' EXIT
incs=(-I compiler -I lib)
[ -n "$overlay" ] && incs=(-I "$overlay" "${incs[@]}")
cd "$root" || exit 2
bad=0
for p in core modes inplace adv fuzz; do
  if ! "$fibc" build "compiler/tests/fibref/heap-$p.fib" "${incs[@]}" -L "$llvmdir" -l LLVM-21 -o "$tmp/$p" 2> "$tmp/$p.err"; then
    head -5 "$tmp/$p.err" >&2; echo "heap-$p: FAILED to build"; bad=1; continue
  fi
  "$tmp/$p"; rc=$?
  [ "$rc" = 0 ] || bad=1
done
exit $bad
