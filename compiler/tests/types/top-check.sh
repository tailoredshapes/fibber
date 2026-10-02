#!/bin/bash
# The judge of package P5 (types.lower.top, types.lower.mod, the fun/def/extern sections): the tool equals
# `fibref types` on top-cases/ (declaration and definition errors of top.rs and mod.rs, impl bodies, macros)
# in the sections and the ast. Run from the repository root.
# usage: top-check.sh FIBREF TOOL     exit 0 when both comparisons are all `same`
rust=$1; tool=$2; bad=0
for o in "--stage lower --sections type,protocol,instance,fun,def,extern,error" "--stage lower --ast"; do
  r=$(compiler/tests/types/compare.sh -j 4 -o "$o" "$rust" "$tool" compiler/tests/types/top-cases/*.fib)
  echo "$o: $(echo "$r" | head -1)"
  echo "$r" | grep -q 'different 0' || { echo "$r" | tail -n +2; bad=1; }
done
exit $bad
