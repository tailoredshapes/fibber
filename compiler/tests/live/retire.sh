#!/bin/bash
# compiler/tests/live/retire.sh [FIBC]: what running state holds into a retired generation (docs/design/live-incremental.md 7; serve.livereach,
# serve.liveretire, serve.livesplit): retire.py over a real socket against retire-app.fib. Closures made by a swapped body (in an atom, in a def,
# run by a spawned thread) answer after their generation's family is swapped again and retired; a thread parked at a pause point it reaches
# through a function value read from the heap, inside a body swapped while it is parked, returns out of that body after the retirement and
# the resume. About a minute. Exit 0 when every check holds. The mutant of scripts/mutant-live.sh (retire-dynamic-unfollowed) must make it
# fail: before the fix the retirement freed the parked worker's generation and the program died on the resume (`fatal signal 11`).
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: retire.sh FIBC (or FIBC in the environment)"; exit 2; }
fibc=$(cd "$(dirname "$fibc")" && pwd)/$(basename "$fibc")
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_WITH
cd "$root" || exit 2
python3 "$here/retire.py" "$fibc" "$root"
