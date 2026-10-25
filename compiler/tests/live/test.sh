#!/bin/bash
# compiler/tests/live/test.sh [FIBC]: the checks of the `test` op of a live session (package LT2 of docs/design/live-meshql.md section 6;
# serve.livetest, serve.livewords, serve.live, serve.liveclient, `--with` of expand.modules).
#   1. unit-livetest.fib: the pure half (the selection and its refusals, the words, the entry's text, the rendering of the document, the
#      status line), built with FIBC without LLVM (serve.livewords needs none)
#   2. test.py: everything over a real socket against test-app.fib with test-spec.fib loaded by --with (its docstring lists the checks),
#      about two minutes
# Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (stale-never, rerun-old-registry, test-on-attach-thread,
# status-waits-for-run, sink-shared-run) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: test.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_WITH
out=${TMPDIR:-/tmp}/live-test-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
if (ulimit -v 16000000; "$fibc" build "$here/unit-livetest.fib" -I compiler -I lib -o "$out/unit") > "$out/unit.build" 2>&1 && "$out/unit" > "$out/unit.log" 2>&1; then
  echo "ok unit-livetest: $(tail -n 1 "$out/unit.log")"
else echo "FAIL unit-livetest"; tail -5 "$out/unit.build" "$out/unit.log" 2>/dev/null; bad=1; fi
python3 "$here/test.py" "$fibc" "$root" || bad=1
exit "$bad"
