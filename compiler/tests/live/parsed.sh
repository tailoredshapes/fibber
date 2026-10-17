#!/bin/bash
# compiler/tests/live/parsed.sh [FIBC]: the loader's memo of parsed files in a live session (expand.parsed, serve.incr `Stats.parsed`;
# docs/design/live-incremental.md 2.6): parsed.py over a real socket against load-app.fib with load-routes.fib as src/routes.fib, edited
# on disk and loaded (its docstring lists the checks), about a minute. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh
# (parse-memo-off, parse-memo-by-name, parse-memo-by-length) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: parsed.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS
cd "$root" || exit 2
python3 "$here/parsed.py" "$fibc" "$root"
