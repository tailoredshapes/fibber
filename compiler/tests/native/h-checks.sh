#!/bin/bash
# Package H's checks: the unit tests of the header (h-header.fib) and of exec (h-exec.fib), then, with LAIR (the Rust lair) and LAIRF (lairf)
# set, the case harness on the fixtures of h-cases/ (every verdict rule, every header error, every path; compare-cases.sh) and the CLI (h-cli.sh).
# usage: h-checks.sh [OUTDIR]     FIBC names the seed fibc, FIB_LIB the library; run from anywhere. Exit 0 when everything passes.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
out=${1:-${TMPDIR:-/tmp}/h-checks-$$}
mkdir -p "$out"
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
rc=0
for t in h-header h-exec; do
  "$fibc" build compiler/tests/native/$t.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$out/$t" || { echo "h-checks: $t FAILED to build"; rc=1; continue; }
  LD_LIBRARY_PATH=/usr/lib/llvm-21/lib "$out/$t" > "$out/$t.out"; s=$?
  echo "$t: $(grep -c '^ok' "$out/$t.out") ok, $(grep -c '^FAIL' "$out/$t.out") FAIL (status $s)"
  [ "$s" -eq 0 ] || { grep '^FAIL' "$out/$t.out"; rc=1; }
done
if [ -n "${LAIR:-}" ] && [ -n "${LAIRF:-}" ]; then
  compiler/tests/native/compare-cases.sh compiler/tests/native/h-cases/sub-a compiler/tests/native/h-cases/sub-b || rc=1
  compiler/tests/native/h-cli.sh | tail -n 1; [ "${PIPESTATUS[0]}" -eq 0 ] || rc=1
fi
exit $rc
