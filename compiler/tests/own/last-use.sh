#!/bin/bash
# The ownership dump of the programs of compiler/tests/own/last-use/ (lever L1, docs/design/in-place-update.md: moves at last use),
# against the goldens last-use/NAME.txt, which are what the fibber tool prints. The Rust seed's `fibref own` does not know the pass
# and prints `retain` where these print `moved` (compiler/mirror-pending/last-use.md): there is no Rust oracle for this directory.
# usage: last-use.sh TOOL        compare TOOL's output (the built compiler/own.fib) with the goldens; exits 1 on a difference
#        last-use.sh -w TOOL     write the goldens from TOOL
# Run from the repository root; `same NAME` or `DIFF NAME` for each program.
dir=compiler/tests/own/last-use; bad=0
if [ "$1" = "-w" ]; then
  for f in $dir/*.fib; do "$2" --implicit-lib "" "$f" > "${f%.fib}.txt"; done; exit 0
fi
for f in $dir/*.fib; do
  n=$(basename "$f" .fib)
  (ulimit -v 4000000; timeout 120 "$1" --implicit-lib "" "$f" 2>&1) | cmp -s - "${f%.fib}.txt" && echo "same $n" || { echo "DIFF $n"; bad=1; }
done
exit $bad
