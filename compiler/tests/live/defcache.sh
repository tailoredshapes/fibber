#!/bin/bash
# compiler/tests/live/defcache.sh [FIBC]: the checks of the constant defs' values kept across the programs of a live session (package
# LE1 of docs/design/live-incremental.md 2.2 and 4; emit.defs.cache, emit.program `DefCache`): defcache.py over a real socket against
# defcache-app.fib, three apps side by side (the cache on, FIB_LIVE_EMIT=verify, FIB_LIVE_EMIT=off; its docstring lists the checks: the defs read
# through three generations of the handler and through expressions, a required module edited on disk read anew, the three apps
# agreeing byte for byte, the emit of a swap bounded), about two minutes. Exit 0 when every check holds. The mutants of
# scripts/mutant-live.sh (defcache-stale, defcache-no-items, defcache-verify-blind) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: defcache.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_EMIT
cd "$root" || exit 2
# unbuffered: a run the stamp's timeout kills still shows its checks (the CI log of 37dabc68 was empty)
python3 -u "$here/defcache.py" "$fibc" "$root"
