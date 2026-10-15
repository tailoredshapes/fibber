#!/bin/bash
# compiler/tests/live/load.sh [FIBC]: the checks of `load` and `reload` of a live session (package LM1 of docs/design/live-incremental.md 3;
# serve.liveload, serve.incr `Job.texts`, expand.modules `try-load-texts`, emit.compile `BodyRequest.module`, serve.liveclient `:load`):
# load.py over a real socket against load-app.fib with load-routes.fib as src/routes.fib, edited on disk between loads (its docstring lists
# the checks), about two minutes. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (partial-load, load-reads-disk,
# prefix-dropped, removed-ignored, rules-at-file-start) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: load.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS
cd "$root" || exit 2
python3 "$here/load.py" "$fibc" "$root"
