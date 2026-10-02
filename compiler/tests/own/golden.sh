#!/bin/bash
# The golden outputs of `fibref own` (spec/bootstrap.md §7) on the programs of compiler/tests/own/golden/:
# golden/NAME.txt is what `fibref own --implicit-lib "" golden/NAME.fib` prints (first line `== PATH`), run from the
# repository root. Ids depend on lib/prelude.fib (the prelude's bindings and expressions come first), so a change
# to it needs `golden.sh -w FIBREF`; the programs use the prelude alone, so no other library moves them.
# usage: golden.sh -w FIBREF        write every golden from the Rust tool
#        golden.sh FIBREF TOOL      compare FIBREF's output (and TOOL's, the built compiler/own.fib) with the goldens
# Prints `same NAME` or `DIFF NAME` for TOOL's output, and exits 1 if any golden is stale (FIBREF) or any differs (TOOL).
dir=compiler/tests/own/golden; bad=0
if [ "$1" = "-w" ]; then
  for f in $dir/*.fib; do "$2" own --implicit-lib "" "$f" > "${f%.fib}.txt"; done; exit 0
fi
for f in $dir/*.fib; do
  g=${f%.fib}.txt; n=$(basename "$f" .fib)
  "$1" own --implicit-lib "" "$f" | cmp -s - "$g" || { echo "STALE $n"; bad=1; }
  if [ -n "$2" ]; then
    (ulimit -v 4000000; timeout 120 "$2" --implicit-lib "" "$f" 2>&1) | cmp -s - "$g" && echo "same $n" || { echo "DIFF $n"; bad=1; }
  fi
done
exit $bad
