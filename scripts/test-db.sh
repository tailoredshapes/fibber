#!/bin/bash
# fib.db checks against the system SQLite (docs/design/observability-and-databases.md 6): the Driver contract and the SQLite feature under
# `fibc test` (libsqlite3 preloaded: the JIT resolves externs against the process), then the same spec built and linked as an executable.
# Usage: FIBC=/path/to/stage2 scripts/test-db.sh. Needs libsqlite3.so.0 (no development package: the link names the runtime library).
set -euo pipefail
cd "$(dirname "$0")/.."
fibc=${FIBC:?set FIBC to a stage 2 fibc}
lib=$(ldconfig -p | awk '/libsqlite3\.so\.0 .*x86-64|libsqlite3\.so\.0 .*AArch64/ { if (!f) { print $NF; f = 1 } }')
[ -n "$lib" ] || { echo "test-db: libsqlite3.so.0 not found (ldconfig -p)"; exit 2; }
scratch=$(mktemp -d "${TMPDIR:-/tmp}/fib-db.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
echo "== fibc test (LD_PRELOAD=$lib)"
LD_PRELOAD="$lib" "$fibc" test scripts/tests/db --seed 1
echo "== built and linked with -l:libsqlite3.so.0"
"$fibc" build scripts/tests/db/sqlite-spec.fib -I lib -I scripts/tests/db -l:libsqlite3.so.0 -o "$scratch/sqlite-spec"
"$scratch/sqlite-spec" --seed 1 | tail -n 1
