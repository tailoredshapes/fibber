#!/bin/bash
# compiler/tests/harness-proto/run.sh: the prototype of fib.test checked against itself (docs/design/test-harness.md §7). It builds
# map-spec.fib and judges what the harness reports, so a harness that stopped reporting a failure, stopped isolating a trap or lost the
# Map contract's reach into an implementation fails here:
#   1. the spec as written: 21 scenarios, 18 pass, 2 fail, 1 trap, and the run reaches the scenarios AFTER the trap (isolation);
#   2. the Map contract passes against both implementations (12 of 12);
#   3. a planted fault in the association list (assoc appends a duplicate key) fails exactly that scenario of that implementation and
#      nothing of the library's Map: the fault is shown, then the tree's alist.fib is untouched (the plant is made in a copy);
#   4. replay: the same seed gives the same report, another seed gives another counterexample draw.
# usage: compiler/tests/harness-proto/run.sh        environment: FIBC (a stage 2 with `unchecked-add`; default the tree's gate F), FIB_LIB
#   (default this tree's lib), SCRATCH (default ~/.cache/fibber-scratch/harness-proto). Exit 0 when every check held.
set -uo pipefail
R=$(cd "$(dirname "$0")/../../.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "harness-proto: no fibc: set FIBC" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$R/lib}
S=${SCRATCH:-$HOME/.cache/fibber-scratch/harness-proto}
mkdir -p "$S/plant" "$S/tmp"; export TMPDIR=$S/tmp
ulimit -v 16000000
fails=0
check() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fails=$((fails+1)); fi; }

"$FIBC" build "$R/compiler/tests/harness-proto/map-spec.fib" -o "$S/spec" || exit 2

out=$("$S/spec"); st=$?
check "1 exit status of a run with failures" "$st" 1
check "1 counts" "$(echo "$out" | grep -E '^[0-9]+ scenarios:')" "21 scenarios: 18 pass, 2 fail, 1 trap, 0 timeout (seed 1)"
check "1 the trap is a row" "$(echo "$out" | grep -c '^  TRAP')" 1
check "1 the scenarios after the trap ran" "$(echo "$out" | grep -c -E '^  (pass|FAIL)  (reverse twice|every vector)')" 2

out=$("$S/spec" --only MapContract); st=$?
check "2 contract against both implementations: exit" "$st" 0
check "2 contract counts" "$(echo "$out" | grep -E '^[0-9]+ scenarios:')" "12 scenarios: 12 pass, 0 fail, 0 trap, 0 timeout (seed 1)"

cp "$R"/compiler/tests/harness-proto/*.fib "$S/plant/"
sed -i '/(assoc (self k v)/,/(Dissoc i64)/ s/(if (< i 0)/(if (< i 1000)/' "$S/plant/alist.fib"
check "3 the plant is in the copy only" "$(diff "$R/compiler/tests/harness-proto/alist.fib" "$S/plant/alist.fib" | grep -c '^>')" 1
"$FIBC" build "$S/plant/map-spec.fib" -o "$S/spec-plant" || exit 2
out=$("$S/spec-plant" --only MapContract --format json); st=$?
check "3 planted fault: exit" "$st" 1
check "3 planted fault: one failure" "$(echo "$out" | grep -o '"summary":{[^}]*}')" '"summary":{"total":12,"pass":11,"fail":1,"trap":0,"timeout":0}'
check "3 planted fault: in the AList, in the replace scenario" \
  "$(echo "$out" | grep -o '"id":"[^"]*","text":"[^"]*","covers":\["assoc"\],"status":"fail"')" \
  '"id":"MapContract[AList]/assoc-on-an-existing-key-replaces-its-value","text":"assoc on an existing key replaces its value","covers":["assoc"],"status":"fail"'

a=$("$S/spec" --only every-vector --seed 7); b=$("$S/spec" --only every-vector --seed 7); c=$("$S/spec" --only every-vector --seed 8)
check "4 same seed, same report" "$a" "$b"
check "4 another seed, another draw" "$([ "$a" != "$c" ] && echo differ || echo same)" differ

echo "harness-proto: $fails failed"
exit $((fails > 0))
