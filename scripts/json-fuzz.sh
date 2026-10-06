#!/bin/bash
# Mutation fuzz of fib.json's readers (scripts/json-fuzz.fib): N mutated documents (default 100000), seed S (default 7). A trap, or a disagreement between the DOM parser and
# the tape reader, fails the run. FIBC = a stage 2 (default `fibc`).
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$here/..
FIBC=${FIBC:-fibc}; work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
FIB_LIB=$root/lib "$FIBC" build "$here/json-fuzz.fib" -I "$root/lib" -o "$work/fuzz" || exit 2
"$work/fuzz" "${1:-100000}" "${2:-7}"
