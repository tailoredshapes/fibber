#!/bin/bash
# The golden outputs of the ownership dump (spec/bootstrap.md §7) on the programs of compiler/tests/own/golden/: golden/NAME.txt is what
# `own --implicit-lib "" golden/NAME.fib` prints (first line `== PATH`), run from the repository root. They were recorded from the Rust
# `fibref own` of tag seed-1 and are stage 2's own now (docs/rust-legacy.md). Ids depend on lib/prelude.fib (the prelude's bindings and
# expressions come first), so a change to it needs `golden.sh -w TOOL` and a read of the diff; the programs use the prelude alone.
# usage: golden.sh TOOL        compare TOOL's output (the built compiler/own.fib) with the goldens; exits 1 on a difference
#        golden.sh -w TOOL     write the goldens from TOOL
# Prints `same NAME` or `DIFF NAME`.
dir=compiler/tests/own/golden; bad=0
if [ "$1" = "-w" ]; then
  for f in $dir/*.fib; do "$2" --implicit-lib "" "$f" > "${f%.fib}.txt"; done; exit 0
fi
for f in $dir/*.fib; do
  n=$(basename "$f" .fib)
  (ulimit -v 4000000; timeout 120 "$1" --implicit-lib "" "$f" 2>&1) | cmp -s - "${f%.fib}.txt" && echo "same $n" || { echo "DIFF $n"; bad=1; }
done
exit $bad
