#!/bin/bash
# Builds and runs compiler/tests/gen/rng-check.fib and diffs its output with compiler/tests/golden/gen/rng.tsv (comment lines dropped).
# A port of SplitMix64 that gets one bit, one unsigned remainder or one draw order wrong fails here. Prints ok or the diff.
# usage: rng-check.sh   FIBC names the fibc (a stage 2 from the tree: `unchecked-add` is newer than the v0.1.5 seed); FIB_LIB the library.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
export FIB_LIB=${FIB_LIB:-$root/lib}
out=${TMPDIR:-/tmp}/gen-rng-check-$$
cd "$root" || exit 2
"$fibc" build compiler/tests/gen/rng-check.fib -I compiler -I lib -o "$out" || { echo "rng-check: FAILED to build" >&2; exit 1; }
"$out" > "$out.txt"; rc=$?
grep -v '^#' compiler/tests/golden/gen/rng.tsv > "$out.want"
if [ "$rc" = 0 ] && diff "$out.txt" "$out.want" > "$out.diff"; then rm -f "$out" "$out.txt" "$out.want" "$out.diff"; echo ok; exit 0; fi
cat "$out.diff" >&2; rm -f "$out" "$out.txt" "$out.want" "$out.diff"; echo "rng-check: FAILED" >&2; exit 1
