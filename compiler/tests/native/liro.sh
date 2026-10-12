#!/bin/bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root" || exit 2
exec python3 compiler/tests/native/liro/check.py "$@"
