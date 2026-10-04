#!/bin/bash
# compiler/tests/own/peek.sh F: the plan of cell peeks is in the lIR (spec/types.md §6.3, "Cell peeks"). Emits compiler/tests/own/peek-probe.fib with
# the stage 2 F and counts the `fib.retain` and `fib.release` calls of three functions: `peeked` none, `acquired` at least two (the retain of
# `@c` into a binding and its release), `named` at least two (a sibling operand names the cell, so `@c` is no peek). Exit 0 when all hold.
# The pass cases (cases/ownership 286-295) show that a peek is never wrong; this shows that it happens.
set -u
F=${1:?usage: peek.sh F}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
cd "$root" || exit 2
export FIB_LIB=${FIB_LIB:-$root/lib}
lir=$("$F" emit "$here/peek-probe.fib" 2>&1) || { echo "peek.sh: emit failed"; echo "$lir" | tail -5; exit 1; }
counts() { # counts NAME: the retain and release calls inside the function `(f.NAME ..`
  echo "$lir" | awk -v n="(f.$1 " '/^\(define/{inside = index($0, n) > 0} inside && /call @fib\.(retain|release) /{c++} END{print c+0}'
}
bad=0
p=$(counts peeked); a=$(counts acquired); n=$(counts named)
[ "$p" -eq 0 ] || { echo "peek.sh: peeked has $p count operations, want 0"; bad=1; }
[ "$a" -ge 2 ] || { echo "peek.sh: acquired has $a count operations, want at least 2"; bad=1; }
[ "$n" -ge 2 ] || { echo "peek.sh: named has $n count operations, want at least 2"; bad=1; }
[ $bad -eq 0 ] && echo "peek.sh: ok (peeked $p, acquired $a, named $n)"
exit $bad
