#!/bin/bash
# The judge of the lowering core (package E4) before the whole tool prints `fns`: for each FILE, the functions
# `fibc emit-dump --sections fns` (the Rust, the oracle) prints against what compiler/tests/emit/ctrl-tool.fib prints, byte for
# byte. A file whose lowering reaches a stub of another package (the tool traps, exit 134) is PENDING, never a pass; a file the
# Rust refuses or does not lower (`unsupported`, a rejected program) is SKIPPED. Run from the repository root.
# usage: ctrl.sh FIBC TOOL FILE..     (TOOL: `fibc build compiler/tests/emit/ctrl-tool.fib -I compiler -I lib -o TOOL`)
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
same=0; diff=0; pending=0; skipped=0; T=${TMPDIR:-$HOME/.cache/fibber-scratch}/ctrl-sh.$$; mkdir -p "$T"
for f in "$@"; do
  timeout 120 "$FIBC" emit-dump --sections fns "$f" > "$T/rust" 2> "$T/rust.err"; rc=$?
  if [ $rc -ne 0 ]; then skipped=$((skipped+1)); continue; fi
  (ulimit -c 0 -v 4000000; timeout 120 "$TOOL" "$f" > "$T/fib" 2> "$T/fib.err"); rc=$?
  if [ $rc -eq 134 ] || grep -q '^error: todo' "$T/fib"; then pending=$((pending+1)); echo "pending $f: $(grep -h -o 'todo: [A-Za-z0-9]* [a-z0-9-]*' "$T/fib" "$T/fib.err" | head -1)"; continue; fi
  if cmp -s "$T/rust" "$T/fib"; then same=$((same+1)); else diff=$((diff+1)); echo "DIFFERENT $f"; fi
done
rm -rf "$T"
echo "same $same, different $diff, pending $pending, skipped $skipped"
[ $diff -eq 0 ]
