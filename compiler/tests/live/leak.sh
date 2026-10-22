#!/bin/bash
# compiler/tests/live/leak.sh [FIBC]: the resident size of a live session does not grow with its swaps (docs/design/live-incremental.md 8;
# serve.incr `retain-keys`, own.chain `chain-retain`, macros.runner `session-retain`). leak.py: an 11-module program, the last module
# but one swapped 200 times by `load`; the growth of the last 100 swaps is bounded. Before the fix each swap kept the checker's state and the
# expander's entry for the old text (8 MB each in this program). Exit 0 when every check holds. The mutant `retain-keys-off` of
# scripts/mutant-live.sh (retain-keys does nothing) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: leak.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS
cd "$root" || exit 2
python3 "$here/leak.py" "$fibc" "$root"
