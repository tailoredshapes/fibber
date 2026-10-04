#!/bin/bash
# `explain` of the programs of compiler/tests/own/owned-param/ (batch 6, spec/types.md §6.4 "Declared owned" and §6.6 "Taking the content"),
# against the goldens owned-param/NAME.txt: a parameter declared `:owned` is `owned (not inferred)` although the body only reads it, and its caller
# hands the argument over (`a: moved`), where the same function without the qualifier is `borrowed` and its caller passes `borrow`; an `&` parameter
# forwarded at a call that is not a tail call is `move in`. No Rust oracle: the Rust `fibref explain` does not read `:owned` on a `defun`.
# usage: owned-param.sh F        compare `F explain` with the goldens; exits 1 on a difference
#        owned-param.sh -w F     write the goldens from F
# Run from the repository root; `same NAME` or `DIFF NAME` for each program.
dir=compiler/tests/own/owned-param; bad=0
if [ "$1" = "-w" ]; then
  for f in $dir/*.fib; do "$2" explain "$f" > "${f%.fib}.txt" 2>&1; done; exit 0
fi
for f in $dir/*.fib; do
  n=$(basename "$f" .fib)
  (ulimit -v 4000000; timeout 120 "$1" explain "$f" 2>&1) | cmp -s - "${f%.fib}.txt" && echo "same $n" || { echo "DIFF $n"; bad=1; }
done
exit $bad
