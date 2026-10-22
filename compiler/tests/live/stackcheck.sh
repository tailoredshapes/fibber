#!/bin/bash
# compiler/tests/live/stackcheck.sh [FIBC]: a stack overflow in a live session is an error reply, not the end of the session (docs/design/stack-check.md): stackcheck.py.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: stackcheck.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
cd "$root" || exit 2
python3 "$here/stackcheck.py" "$fibc" "$root"
