#!/bin/bash
# compiler/tests/live/attach.sh [FIBC]: the checks of the attach socket of `fibc run --live` (package LV1 of docs/design/live-repl.md
# section 6; serve.live, serve.livesess, serve.livesock, serve.liveclient).
#   1. unit-livesock.fib: the peer-uid rule with planted uids (built with FIBC without LLVM: serve.livesock needs none)
#   2. attach.py: everything over a real socket against app.fib (its docstring lists the checks), about a minute
# Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (value-reevaluated, trap-not-caught) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: attach.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
out=${TMPDIR:-/tmp}/live-attach-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
if (ulimit -v 16000000; "$fibc" build "$here/unit-livesock.fib" -I compiler -I lib -o "$out/unit") > "$out/unit.build" 2>&1 && "$out/unit" > "$out/unit.log" 2>&1; then
  echo "ok unit-livesock: $(tail -n 1 "$out/unit.log")"
else echo "FAIL unit-livesock"; tail -5 "$out/unit.build" "$out/unit.log" 2>/dev/null; bad=1; fi
python3 "$here/attach.py" "$fibc" "$root" || bad=1
exit "$bad"
