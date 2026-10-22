#!/bin/bash
# compiler/tests/live/mt.sh [FIBC]: the attach thread and the program's thread both run program code (docs/design/host-threads.md): mt.py.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: mt.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
cd "$root" || exit 2
python3 "$here/mt.py" "$fibc" "$root"
