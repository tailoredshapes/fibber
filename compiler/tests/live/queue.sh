#!/bin/bash
# compiler/tests/live/queue.sh [FIBC]: the checks of the queue mode (package LE3 of docs/design/live-incremental.md 2.3;
# emit.program `resident`/`program-request`, emit.typetext, serve.liveswap `requests-of`): queue.py over a real socket against
# queue-app.fib, three times (the queue mode on, FIB_LIVE_QUEUE=0 for a whole emit, FIB_LIVE_EMIT=verify; its docstring lists the
# checks: a handler swapped twice, a generic swapped through a resident caller, a new specialisation, the three apps agreeing, the
# swap's emit under 60 ms), about a minute. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (resident-by-family,
# skip-new-specialisation, literal-counter-per-program, specialisation-not-requested) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: queue.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_EMIT FIB_LIVE_QUEUE
cd "$root" || exit 2
python3 "$here/queue.py" "$fibc" "$root"
