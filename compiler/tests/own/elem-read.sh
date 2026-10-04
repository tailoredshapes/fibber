#!/bin/bash
# The ownership dump of the programs of compiler/tests/own/elem-read/ (performance batch 4, lever B: spec/types.md §6.3, "Element reads"),
# against the goldens elem-read/NAME.txt, which are what the fibber tool prints: an `array-get` of a borrowed or derived array is
# `derived`, one of an owned array operand is `owned`. The Rust `fibref own` gives every `array-get` result the mode `owned`, so there is no Rust
# oracle for this directory. Ids depend on lib/prelude.fib: a change to it needs `elem-read.sh -w TOOL`.
# usage: elem-read.sh TOOL        compare TOOL's output (the built compiler/own.fib) with the goldens; exits 1 on a difference
#        elem-read.sh -w TOOL     write the goldens from TOOL
# Run from the repository root; `same NAME` or `DIFF NAME` for each program.
dir=compiler/tests/own/elem-read; bad=0
if [ "$1" = "-w" ]; then
  for f in $dir/*.fib; do "$2" --implicit-lib "" "$f" > "${f%.fib}.txt"; done; exit 0
fi
for f in $dir/*.fib; do
  n=$(basename "$f" .fib)
  (ulimit -v 4000000; timeout 120 "$1" --implicit-lib "" "$f" 2>&1) | cmp -s - "${f%.fib}.txt" && echo "same $n" || { echo "DIFF $n"; bad=1; }
done
exit $bad
