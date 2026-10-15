#!/bin/bash
# compiler/tests/live/watch.sh [FIBC]: the checks of `watch` of a live session (package LT5 of docs/design/live-meshql.md 3.8;
# serve.livewatch, serve.live `watch-step`, serve.liveclient `:watch`): watch.py over a real socket against watch-app.fib with
# watch-handler.fib as src/handler.fib and a feature file, edited on disk while the connection watches (its docstring lists the checks),
# about a minute. Exit 0 when every check holds. The mutants of scripts/mutant-live.sh (watch-no-load, watch-reruns-on-refusal) must make
# it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: watch.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS
cd "$root" || exit 2
python3 "$here/watch.py" "$fibc" "$root"
