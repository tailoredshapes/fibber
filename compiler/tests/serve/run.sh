#!/bin/bash
# compiler/tests/serve/run.sh [FIBC]: the tests of `fibc serve` (run.py). Offline, about 30 s, a server on a socket in a scratch directory.
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
exec python3 "$here/run.py" "${1:-${FIBC:-fibc}}" "$root"
