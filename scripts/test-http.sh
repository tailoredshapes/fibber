#!/bin/bash
# Native HTTP checks; dependencies: stage 2 and Python (the differential suite, scripts/tests/http/integration.py). No libcurl, no openssl.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
if [ -x ./F ]; then default_fibc=./F; else default_fibc=fibc; fi
fibc=${HTTP_FIBC:-${FIBC:-$default_fibc}}
if [ -n "${HTTP_TEST_OUT:-}" ]; then scratch=$HTTP_TEST_OUT; else
  scratch=$(mktemp -d /tmp/fibber-http-tests.XXXXXX)
  trap 'rm -rf -- "$scratch"' EXIT
fi
mkdir -p "$scratch"
exec 9>/tmp/fibsuite.lock
flock 9
"$fibc" cases cases/stdlib --only 7300 7301 7302 7303 7304 7305 7306 7307 7308 -j 9
"$fibc" build scripts/tests/http/server.fib -I lib -o "$scratch/server"
"$fibc" build scripts/tests/http/client.fib -I lib -o "$scratch/client"
PYTHONDONTWRITEBYTECODE=1 python3 scripts/tests/http/integration.py --server "$scratch/server" --client "$scratch/client"
