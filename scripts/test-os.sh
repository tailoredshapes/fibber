#!/bin/bash
# Native OS cases, C-header ABI conformance, and Darwin backend emission.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
if [ -x ./F ]; then default_fibc=./F; else default_fibc=fibc; fi
fibc=${OS_FIBC:-${FIBC:-$default_fibc}}
if [ -n "${OS_TEST_OUT:-}" ]; then scratch=$OS_TEST_OUT; else
  scratch=$(mktemp -d /tmp/fibber-os-tests.XXXXXX)
  trap 'rm -rf -- "$scratch"' EXIT
fi
mkdir -p "$scratch"
# macOS need not install flock to run this script.
if command -v flock >/dev/null 2>&1; then
  exec 9>/tmp/fibsuite.lock
  flock 9
fi
"$fibc" cases cases/stdlib --only 7400 7401 7402 7403 7404 7405 7406 7407 7408 -j 9
"$fibc" build scripts/tests/os/abi.fib -I lib -o "$scratch/abi-fibber"
"${CC:-cc}" -std=c11 -Wall -Wextra -Werror scripts/tests/os/abi.c -o "$scratch/abi-c"
"$scratch/abi-fibber" > "$scratch/abi-fibber.txt"
"$scratch/abi-c" > "$scratch/abi-c.txt"
diff -u "$scratch/abi-c.txt" "$scratch/abi-fibber.txt"
echo "OS ABI agrees with the host C headers"
"$fibc" emit scripts/tests/os/darwin-backend.fib -I lib/platform/darwin -I lib > "$scratch/darwin-backend.lir"
# This checks source typing/emission, not linking against a Darwin SDK or execution.
if ! grep -Fq '__error' "$scratch/darwin-backend.lir"; then
  echo "Darwin backend was not selected" >&2
  exit 1
fi
echo "Darwin backend emitted successfully (native macOS execution requires a macOS host)"
