#!/bin/bash
# The programs of docs/shootout/parallel/*.fib are measurements, not tests, and were written against an API that moved (the tile combinators of
# fib.view.tiles, P-count-b): this builds each with FIBC so that a change to the library that breaks one is seen. It does not run them (they take
# seconds to minutes on 28 cores and are timed under /tmp/fibsuite.lock; the commands are in docs/shootout/parallel.md).
#   compiler/tests/shootout-compile.sh [OUTDIR]     FIBC names the stage 2 (required); FIB_LIB the library (default lib/); LLVM_LIBDIR (default /usr/lib/llvm-21/lib)
# Output: `ok FILE` or `FAIL FILE` with the compiler's first lines; exit 0 when every program compiles.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=${1:-${TMPDIR:-/tmp}/shootout-compile-$$}
fibc=${FIBC:?shootout-compile.sh: set FIBC to a stage 2}
export FIB_LIB=${FIB_LIB:-$root/lib}
mkdir -p "$out"
cd "$root" || exit 2
bad=0; n=0
for f in docs/shootout/parallel/*.fib; do
  n=$((n+1))
  if "$fibc" build "$f" -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$out/prog" > "$out/log" 2>&1; then echo "ok $f"
  else echo "FAIL $f"; head -n 5 "$out/log"; bad=1; fi
done
[ "$n" -gt 0 ] || { echo "FAIL no program in docs/shootout/parallel"; exit 1; }
echo "$n programs compiled"
exit $bad
