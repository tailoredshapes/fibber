#!/bin/bash
# scripts/mutant-parallel.sh: planted faults for fib.parallel (docs/design/parallelism.md 3.2, P-struct). Library only: copies lib/ and the
# stdlib cases to a scratch directory, breaks ONE rule of lib/fib/parallel* in the copy, and runs the cases that pin the rule with a fibc
# (`fibc cases` over the copy, the library found through the header's `roots`). Every one must FAIL under the mutant (a wrong answer, a
# trap, a hang killed by the timeout all count). A case that still passes survived and is a bad case.
# usage: scripts/mutant-parallel.sh MODE|all
# environment: FIBC (a fibc; required; the seed is enough, nothing in compiler/ or rt/ changes), MUT_OUT (scratch, default
#   ~/.cache/fibber-scratch/mutant-parallel-MODE). The cases run under `ulimit -v 16000000` and MALLOC_ARENA_MAX=2.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-parallel.sh MODE|all}
R=$(cd "$(dirname "$0")/.." && pwd)
if [ "$MODE" = all ]; then
  rc=0
  for m in $(sed -n 's/^  \([a-z0-9-]*\)) *CASES=.*/\1/p' "$0"); do "$0" "$m" || rc=1; done
  exit $rc
fi
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-parallel-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-parallel: set FIBC to an executable fibc" >&2; exit 2; }
unset LD_LIBRARY_PATH
# MODE) CASES=(prefixes that must fail)     the edit is made in the function `mutate` below
case $MODE in
  cpu-ignores-affinity) CASES=(7531-) ;;
  scope-no-join)        CASES=(7532-) ;;
  scope-first-wins)     CASES=(7532-) ;;
  scope-stops-at-trap)  CASES=(7532-) ;;
  scope-swallows-trap)  CASES=(7532- 7533-) ;;
  *) echo "mutant-parallel: unknown MODE $MODE" >&2; exit 2 ;;
esac
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/lib" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
mut() {  # mut FILE PERL-EXPRESSION: apply, and fail when nothing changed (the source moved)
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-parallel: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
}
mutate() {
  case $MODE in
    cpu-ignores-affinity) mut lib/fib/parallel/cpu.fib 's/\(lesser-known \(lesser-known \(if \(> online 0\) online \(max-workers\)\) affinity\) quota\)/(if (> online 0) online (max-workers))/' ;;
    scope-no-join)        mut lib/fib/parallel/scope.fib 's/\(let \[ts \@\(\. s tasks\)\]/(let [ts []]/' ;;
    scope-first-wins)     mut lib/fib/parallel/scope.fib 's/\(if \(some\? first-message\) first-message \(some \(\. trap message\)\)\)/(some (. trap message))/' ;;
    scope-stops-at-trap)  mut lib/fib/parallel/scope.fib 's/\(if \(< i \(vec-count ts\)\)/(if (and (< i (vec-count ts)) (not (some? first-message)))/' ;;
    scope-swallows-trap)  mut lib/fib/parallel/scope.fib 's/\(\(Err trap\) \(if \(some\? first-message\) first-message \(some \(\. trap message\)\)\)\)/((Err trap) first-message)/' ;;
  esac
}
mutate
cd "$OUT/tree" || exit 2
ulimit -v 16000000; export MALLOC_ARENA_MAX=2
rc=0
for p in "${CASES[@]}"; do
  out=$(timeout 300 "$FIBC" cases cases/stdlib --only "$p" 2>&1); line=$(echo "$out" | grep -E '^[0-9]+ cases:')
  if echo "$line" | grep -q ' 0 fail' && echo "$line" | grep -q ' 0 header'; then echo "mutant $MODE: case $p SURVIVED: $line"; rc=1
  else echo "mutant $MODE: case $p failed as it must: $(echo "$out" | grep -E 'FAIL|HEADER' | cut -c1-160 | head -1)"; fi
done
exit $rc
