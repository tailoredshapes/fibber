#!/bin/bash
# Runs fib.json over the whole JSONTestSuite (fetched by scripts/fetch-json-testsuite.sh): y_ files must parse, n_ files must be rejected, i_ files are
# implementation-defined and are compared with specs/json-testsuite-i.expected (what we do, recorded). Prints the counts; exits 1 on any y_/n_ failure,
# any i_ that differs from the record, or any disagreement between the DOM parser and the tape reader.
#   FIBC=path to a stage 2 fibc (default: fibc on PATH), DEST=where the suite is (default ~/.cache/fibber-scratch/tools/JSONTestSuite)
#   scripts/json-testsuite.sh --record   rewrites specs/json-testsuite-i.expected
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$here/..
dest=${DEST:-$HOME/.cache/fibber-scratch/tools/JSONTestSuite}
FIBC=${FIBC:-fibc}
"$here/fetch-json-testsuite.sh" "$dest" > /dev/null || exit 2
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
FIB_LIB=$root/lib "$FIBC" build "$here/json-testsuite.fib" -I "$root/lib" -o "$work/runner" || exit 2
"$work/runner" "$dest"/test_parsing/*.json > "$work/out.txt"; rc=$?
y=0; n=0; i=0; bad=0
: > "$work/i.txt"
while read -r name verdict code rest; do
  case $name in
    DISAGREE) echo "FAIL: $name $verdict $code $rest"; bad=$((bad+1)) ;;
    y_*) y=$((y+1)); [ "$verdict" = accept ] || { echo "FAIL (must parse): $name code $code"; bad=$((bad+1)); } ;;
    n_*) n=$((n+1)); [ "$verdict" = reject ] || { echo "FAIL (must reject): $name"; bad=$((bad+1)); } ;;
    i_*) i=$((i+1)); echo "$name $verdict" >> "$work/i.txt" ;;
  esac
done < "$work/out.txt"
LC_ALL=C sort "$work/i.txt" -o "$work/i.txt"
if [ "${1:-}" = --record ]; then cp "$work/i.txt" "$root/specs/json-testsuite-i.expected"; echo "recorded $(wc -l < "$work/i.txt") i_ results"
elif ! diff -u "$root/specs/json-testsuite-i.expected" "$work/i.txt"; then echo "FAIL: i_ results differ from specs/json-testsuite-i.expected"; bad=$((bad+1)); fi
echo "JSONTestSuite: y_ $y (must parse), n_ $n (must reject), i_ $i (implementation-defined; accepted $(grep -c ' accept' "$work/i.txt"), rejected $(grep -c ' reject' "$work/i.txt")); failures $bad; runner exit $rc"
[ "$bad" = 0 ] && [ "$rc" = 0 ]
