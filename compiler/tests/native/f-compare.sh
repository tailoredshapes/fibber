#!/bin/bash
# Package F's differential gate: `lair fuzz` (the Rust, LAIR) against `lairf fuzz` (LAIRF) on the same seeds. For each seed in FROM..TO the mutants
# written with --keep must be byte-identical (cmp of every file), and the summary line (counts of rejected, ran, findings) the same; the command
# line errors and exit statuses must be the same too. A mutant that runs may crash at run time by its own undefined behaviour, so a summary that
# differs only in `ran` against `crashed at run time` is reported as FLAKY, not DIFF (run it again to see the Rust vary too).
# usage: f-compare.sh [FROM TO COUNT [DIR]]       (defaults 0 199 25 cases/lir; LAIR and LAIRF name the tools; run from the repository root)
set -u
from=${1:-0}; to=${2:-199}; count=${3:-25}; dir=${4:-cases/lir}
: "${LAIR:?set LAIR to the Rust lair}" "${LAIRF:?set LAIRF to lairf}"
work=${TMPDIR:-/tmp}/f-compare-$$
mkdir -p "$work"
same=0; diff=0; flaky=0
for s in $(seq "$from" "$to"); do
  for t in rust fib; do
    if [ $t = rust ]; then tool=$LAIR; else tool=$LAIRF; fi
    rm -rf "$work/$t"; mkdir -p "$work/$t"
    $tool fuzz --seed "$s" --count "$count" --keep "$work/$t/keep" -o "$work/$t/find" "$dir" > "$work/$t/out" 2>&1
    echo "exit $?" >> "$work/$t/out"
  done
  if ! diff -rq "$work/rust/keep" "$work/fib/keep" > /dev/null; then echo "DIFF   seed $s: the kept mutants differ"; diff=$((diff+1)); continue; fi
  if cmp -s "$work/rust/out" "$work/fib/out"; then same=$((same+1)); continue; fi
  a=$(sed 's/[0-9]* ran, [0-9]* crashed at run time/RAN/' "$work/rust/out"); b=$(sed 's/[0-9]* ran, [0-9]* crashed at run time/RAN/' "$work/fib/out")
  if [ "$a" = "$b" ]; then echo "FLAKY  seed $s: ran/crashed at run time differ"; flaky=$((flaky+1)); else echo "DIFF   seed $s:"; diff "$work/rust/out" "$work/fib/out" | head -5; diff=$((diff+1)); fi
done
echo "mutants and summaries: $same same, $flaky flaky, $diff different, of $((to-from+1)) seeds"
bad=0
cmd() { # ARGS..: the same output (the tool's own name aside) and status from both
  a=$($LAIR "$@" 2>&1); sa=$?; b=$($LAIRF "$@" 2>&1); sb=$?
  a=${a//$LAIR/lair}; b=${b//$LAIRF/lair}
  if [ "$a" = "$b" ] && [ $sa -eq $sb ]; then echo "same   lair $*"; else echo "DIFF   lair $*: [$a] $sa  vs  [$b] $sb"; bad=1; fi
}
cmd fuzz
cmd fuzz --seed x "$dir"
cmd fuzz --count -1 "$dir"
cmd fuzz --seed
cmd fuzz --bogus "$dir"
cmd fuzz -O 9 "$dir"
cmd fuzz "$work/nowhere"
cmd fuzz --seed 5 --print 3 "$dir"
cmd fuzz --seed 5 --print 3 -O 2 "$dir"
rm -rf "$work"
[ "$diff" -eq 0 ] && [ "$bad" -eq 0 ]
