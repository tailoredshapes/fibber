#!/bin/bash
# compiler/tests/live/statics.sh [FIBC]: closure constants and vtables named by content in live mode (package LE2 of
# docs/design/live-incremental.md 2.2 item 3; emit.statics, serve.livesess, serve.livesplit): statics.py over a real socket against
# statics-app.fib (a def holding functions as values and a body swapped to call through it; a method swap; a new instance in an input),
# under FIB_LIVE_DEBUG=cut, about ten seconds. Plain emits are unchanged (stubs.sh's goldens and the golden checks of the passes).
# Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (closure-renumbered, vtable-copied) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: statics.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_WITH
cd "$root" || exit 2
python3 "$here/statics.py" "$fibc" "$root"
