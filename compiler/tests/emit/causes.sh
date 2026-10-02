#!/bin/bash
# The causes of the differences compare.sh kept (its $CMP_OUT, default $HOME/.cache/fibber-scratch/compare-emit-dump): for every
# DIFF in tally.txt, what the fibber side says: a stub reached (`todo: PACKAGE NAME`, printed on stderr, status 70, or `trap: todo:`
# for one that still traps), the expander's `MacroNeedsEvaluator` (no macro runner in stage 2, package E11), or OTHER, which is a
# real difference to read first. Pending is never a pass: the histogram says which package to run next.
# usage: causes.sh [CMP_OUT]
out=${1:-${CMP_OUT:-$HOME/.cache/fibber-scratch/compare-emit-dump}}
for f in $(grep '^DIFF' "$out/tally.txt" | awk '{print $2}'); do
  n=$(echo "$f" | tr '/' '_')
  c=$(grep -m1 -o 'MacroNeedsEvaluator\|todo: [A-Za-z0-9 .-]*' "$out/$n.fib" | head -1)
  echo "${c:-OTHER}"
done | sort | uniq -c | sort -rn
