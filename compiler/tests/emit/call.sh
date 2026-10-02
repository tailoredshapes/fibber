#!/bin/bash
# The judge of calls and arithmetic (package E5): for each FILE, the functions `fibc emit-dump --sections fns` (the Rust, the oracle)
# prints against what the emitter tool (compiler/emit.fib) prints with the same options, byte for byte. A file whose lowering reaches
# a stub of another package (the tool exits 70, or traps with 134) is PENDING, never a pass; a file the Rust refuses or does not lower
# is SKIPPED. Exit 0 only when every file was judged and the same: 1 for a difference, 70 when some file is pending.
# usage: call.sh FIBC TOOL FILE..     (TOOL: `fibc build compiler/emit.fib -I compiler -I lib -o TOOL`; run from the repository root,
# with FIB_LIB set to the library the Rust and the tool should read)
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
same=0; diff=0; pending=0; skipped=0; T=${TMPDIR:-$HOME/.cache/fibber-scratch}/call-sh.$$; mkdir -p "$T"
for f in "$@"; do
  timeout 120 "$FIBC" emit-dump --sections fns "$f" > "$T/rust" 2> "$T/rust.err"; rc=$?
  if [ $rc -ne 0 ] || grep -q '^unsupported\|^rejected\|^error' "$T/rust"; then skipped=$((skipped+1)); echo "skipped $f"; continue; fi
  (ulimit -c 0 -v 4000000; timeout 120 "$TOOL" --sections fns "$f" > "$T/fib" 2> "$T/fib.err"); rc=$?
  if [ $rc -eq 70 ] || [ $rc -eq 134 ] || grep -q 'todo:' "$T/fib.err"; then
    pending=$((pending+1)); echo "pending $f: $(grep -h -o 'todo: [A-Za-z0-9]* [a-z0-9-]*' "$T/fib.err" | head -1)"; continue
  fi
  if cmp -s "$T/rust" "$T/fib"; then same=$((same+1)); else diff=$((diff+1)); echo "DIFFERENT $f"; diff "$T/rust" "$T/fib" | head -6; fi
done
rm -rf "$T"
echo "same $same, different $diff, pending $pending, skipped $skipped"
[ $diff -eq 0 ] || exit 1
[ $pending -eq 0 ] || exit 70
[ $same -gt 0 ]
