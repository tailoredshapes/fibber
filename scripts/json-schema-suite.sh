#!/bin/bash
# fib.json.schema over the JSON-Schema-Test-Suite's draft 7 tests (fetched by scripts/fetch-json-schema-suite.sh): the required tests
# (tests/draft7/*.json) and the optional ones (optional/, optional/format/), every file of remotes/ served at http://localhost:1234/.
# Prints the counts; every failure must be listed in specs/json-schema-suite.expected (what is known not to pass, each with a reason there),
# and every listed one must still fail: exits 1 otherwise.
#   FIBC=path to a stage 2 fibc (default: fibc on PATH), DEST=where the suite is (default ~/.cache/fibber-scratch/tools/JSON-Schema-Test-Suite)
#   scripts/json-schema-suite.sh --record   rewrites the failure list (keeping no reasons: write them back)
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$here/..
dest=${DEST:-$HOME/.cache/fibber-scratch/tools/JSON-Schema-Test-Suite}
FIBC=${FIBC:-fibc}
"$here/fetch-json-schema-suite.sh" "$dest" > /dev/null || exit 2
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
FIB_LIB=$root/lib "$FIBC" build "$here/json-schema-suite.fib" -I "$root/lib" -o "$work/runner" || exit 2
remotes=()
while IFS= read -r f; do remotes+=(--remote "http://localhost:1234/${f#"$dest/remotes/"}" "$f"); done < <(find "$dest/remotes" -name '*.json' | LC_ALL=C sort)
(cd "$dest/tests/draft7" && "$work/runner" "${remotes[@]}" *.json optional/*.json optional/format/*.json) > "$work/out.txt"; rc=$?
total=$(wc -l < "$work/out.txt"); required=$(grep -cv '^optional/' "$work/out.txt")
failed_required=$(grep -v '^optional/' "$work/out.txt" | grep -c $'\tFAIL\t')
failed_optional=$(grep '^optional/' "$work/out.txt" | grep -c $'\tFAIL\t')
grep $'\tFAIL\t' "$work/out.txt" | cut -f1-3,5 | sed 's/ \[compile: .*\]$//' | LC_ALL=C sort > "$work/fails.txt"
expected=$root/specs/json-schema-suite.expected
bad=0
if [ "${1:-}" = --record ]; then cp "$work/fails.txt" "$expected"; echo "recorded $(wc -l < "$work/fails.txt") failures in $expected"
else
  grep -v '^#' "$expected" | cut -f1-4 | LC_ALL=C sort > "$work/want.txt"
  if ! diff -u "$work/want.txt" "$work/fails.txt"; then echo "FAIL: the failures differ from specs/json-schema-suite.expected (- now passes, + newly fails)"; bad=1; fi
fi
echo "JSON-Schema-Test-Suite draft 7: $total tests; required $required, $failed_required failing; optional $((total - required)), $failed_optional failing; runner exit $rc"
[ "$bad" = 0 ] && [ "$rc" = 0 ]
