#!/bin/bash
# Public unit-test API checks; deliberate failures must be reported, never accepted.
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
out=${1:-${TMPDIR:-/tmp}/unit-api-$$}
mkdir -p "$out"
cd "$root"
"${FIBC:?set FIBC}" build compiler/tests/unit/fixture.fib -I lib -o "$out/fixture" > "$out/build.log" 2>&1 || { tail -n 20 "$out/build.log"; exit 1; }
python3 compiler/tests/unit/check.py "$out/fixture"
