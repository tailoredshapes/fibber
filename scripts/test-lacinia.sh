#!/bin/bash
# Native query-engine cases plus a standalone example and JSON envelope check.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
if [ -x ./F ]; then default_fibc=./F; else default_fibc=fibc; fi
fibc=${LACINIA_FIBC:-${FIBC:-$default_fibc}}
if [ -n "${LACINIA_TEST_OUT:-}" ]; then scratch=$LACINIA_TEST_OUT; else
  scratch=$(mktemp -d /tmp/fibber-lacinia-tests.XXXXXX)
  trap 'rm -rf -- "$scratch"' EXIT
fi
mkdir -p "$scratch"
exec 9>/tmp/fibsuite.lock
if command -v flock >/dev/null 2>&1; then flock 9; fi
"$fibc" cases cases/stdlib --only 7500 7501 7502 7503 7504 7505 7506 7507 7508 7509 7510 7511 7512 7513 7514 7515 7516 7517 7518 7519 7520 7521 7522 -j 8
"$fibc" build examples/lacinia.fib -I lib -o "$scratch/example"
"$scratch/example" > "$scratch/response.json"
PYTHONDONTWRITEBYTECODE=1 python3 - "$scratch/response.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response:
    actual = json.load(response)
expected = {"data": {"greeting": "Hello, Fibber", "person": {
    "name": "Ada", "id": "7", "__typename": "Person"}}}
if actual != expected:
    raise SystemExit(f"Unexpected GraphQL response: {actual!r}")
if list(actual["data"]) != ["greeting", "person"]:
    raise SystemExit("Root response fields are out of selection order")
if list(actual["data"]["person"]) != ["name", "id", "__typename"]:
    raise SystemExit("Nested response fields are out of selection order")
print("PASS standalone example, JSON envelope, and selection order")
PY
if [ -n "${LACINIA_ORACLE_PYTHON:-}" ]; then
  "$fibc" build scripts/tests/lacinia/queries.fib -I lib -I cases/stdlib/support -o "$scratch/queries"
  PYTHONDONTWRITEBYTECODE=1 "$LACINIA_ORACLE_PYTHON" scripts/tests/lacinia/compare.py "$scratch/queries"
fi
