#!/bin/bash
# Compare the fibber expander with `fibref expand` on a list of programs, one process per file.
# usage: compare.sh [-j N] [-o "OPTIONS"] FIBREF TOOL FILE..
#   FIBREF  the Rust tool (fibref), TOOL the built compiler/expand.fib binary
#   -o      options given to both tools (default "--no-runner"; spec/bootstrap.md section 5.1),
#           e.g. -o "--no-runner --context" or -o "--no-runner --implicit-lib ''"
# Output: a tally and the first ten differing files; the two outputs of each difference are kept in
# $CMP_OUT (default $HOME/.cache/fibber-scratch/compare-expand) as NAME.rust and NAME.fib, each
# ending in a `status N` line, so that bytes and exit status are compared together.
# Run from the repository root (the tools read lib/prelude.fib from the working directory) with
# FIB_LIB set to the repository's lib. Each process runs under ulimit -v 4000000 and a 120 s timeout.
jobs=4; opts="--no-runner"
while getopts "j:o:" o; do case $o in j) jobs=$OPTARG;; o) opts=$OPTARG;; esac; done
shift $((OPTIND-1)); rust=$1; tool=$2; shift 2
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/compare-expand}; rm -rf "$out"; mkdir -p "$out"
one() {
  f=$1; n=$(echo "$f" | tr '/' '_')
  (ulimit -v 4000000; eval "timeout 120 \"$rust\" expand $opts -- \"$f\"" > "$out/$n.rust" 2>&1; echo "status $?" >> "$out/$n.rust")
  (ulimit -v 4000000; eval "timeout 120 \"$tool\" $opts -- \"$f\"" > "$out/$n.fib" 2>&1; echo "status $?" >> "$out/$n.fib")
  if cmp -s "$out/$n.rust" "$out/$n.fib"; then rm -f "$out/$n.rust" "$out/$n.fib"; echo "same $f"; else echo "DIFF $f"; fi
}
export -f one; export rust tool opts out
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one "$1"' _ {} > "$out/tally.txt"
same=$(grep -c '^same' "$out/tally.txt"); diff=$(grep -c '^DIFF' "$out/tally.txt")
echo "files $((same+diff)): same $same, different $diff"
grep '^DIFF' "$out/tally.txt" | head -10
