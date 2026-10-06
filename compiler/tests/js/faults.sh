#!/bin/bash
# Planted faults that the differential harness (diff.sh) must catch (method.md: a test that cannot fail is worse than none). Each fault
# is one exact text replacement in a copy of the translator (compiler/js, rebuilt with $FIBC) or of the runtime (compiler/js/rt); the
# harness then runs over cases/lir and compiler/tests/js/cases with the faulty backend and must report at least one `differ`.
#   compiler/tests/js/faults.sh [-j N] [NAME..]      (default: every fault)
# Environment: FIBC (builds the faulty lir2js; the seed will do), LAIRF; as diff.sh. Exit 0 when every fault was caught.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
jobs=4; names=()
while [ $# -gt 0 ]; do case $1 in -j) jobs=$2; shift ;; *) names+=("$1") ;; esac; shift; done
: "${FIBC:?}" "${LAIRF:?}"
# name | file under compiler/js | the text | its replacement
faults=(
  "i64-add-wrap|scalar.fib|((BAdd) (big (str a \"+\" b)))|((BAdd) (p (str a \"+\" b)))"
  "overflow-trap|rt/int.js|return [w, w === r ? 0 : 1];|return [w, 0];"
  "switch-branch|func.fib|\"default:{\" (goto fx fl from d)|\"default:{\" (goto fx fl from (. (nth cases 0) label))"
  "tail-stack|func.fib|:else (str \"{const a=[\" (args-js fx args) \"];\" (restore-sp fx) \"\$tf=\" (fn-name n) \";\$ta=a;return \$TAIL;}\")))|:else (str \"return \" (fn-name n) \"(\" (args-js fx args) \");\")))"
  "f32-fround|scalar.fib|((TFloat) (str \"Math.fround\" (float-bin op a b)))|((TFloat) (float-bin op a b))"
)
[ ${#names[@]} -eq 0 ] && for f in "${faults[@]}"; do names+=("${f%%|*}"); done
work=$(mktemp -d "${TMPDIR:-/tmp}/jsfaults.XXXXXX"); trap 'rm -rf "$work"' EXIT
caught=0; missed=0
for want in "${names[@]}"; do
  line=""; for f in "${faults[@]}"; do [ "${f%%|*}" = "$want" ] && line=$f; done
  [ -z "$line" ] && { echo "unknown fault $want"; exit 2; }
  IFS='|' read -r name file from to <<< "$line"
  t=$work/$name; mkdir -p "$t/compiler"
  for e in "$root"/compiler/*; do [ "$(basename "$e")" = js ] || ln -s "$e" "$t/compiler/"; done
  cp -r "$root/compiler/js" "$t/compiler/js"
  FROM=$from TO=$to python3 -c '
import os, sys
p = sys.argv[1]; s = open(p).read(); a = os.environ["FROM"]; b = os.environ["TO"]
if s.count(a) != 1: sys.exit("the text of the fault occurs %d times in %s" % (s.count(a), p))
open(p, "w").write(s.replace(a, b))' "$t/compiler/js/$file" || { echo "FAULT NOT PLANTED $name"; missed=$((missed + 1)); continue; }
  if [ "${file#rt/}" = "$file" ]; then
    (cd "$root" && "$FIBC" build "$t/compiler/lir2js.fib" -I "$t/compiler" -I "$root/lib" -o "$t/lir2js") > "$t/build.log" 2>&1 \
      || { echo "FAULT NOT BUILT $name (log $t/build.log)"; cat "$t/build.log"; missed=$((missed + 1)); continue; }
    lir2js=$t/lir2js
  else
    lir2js=${LIR2JS:?set LIR2JS for a runtime fault}
  fi
  LIR2JS=$lir2js RT=$t/compiler/js/rt "$here/diff.sh" -j "$jobs" "$root/cases/lir" "$root/compiler/tests/js/cases" > "$t/rows" 2>&1
  n=$(grep -c '^differ ' "$t/rows")
  if [ "$n" -gt 0 ]; then caught=$((caught + 1)); echo "caught $name: $n differ, e.g. $(grep -m1 '^differ ' "$t/rows" | cut -c1-150)"
  else missed=$((missed + 1)); echo "MISSED $name: $(tail -1 "$t/rows")"; fi
done
echo "faults: $caught caught, $missed missed"
[ "$missed" -eq 0 ]
