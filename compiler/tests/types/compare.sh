#!/bin/bash
# Compare the fibber type checker with `fibref types` on a list of programs, one process per file.
# usage: compare.sh [-j N] [-o "OPTIONS"] FIBREF TOOL FILE..
#   FIBREF  the Rust tool (target/debug/fibref), TOOL the built compiler/types.fib binary
#   -o      options given to both tools, e.g. "--stage lower --sections type,error" (see spec/bootstrap.md section 6)
# Output: a tally and the first ten differing files; the two outputs of each difference are kept in
# $CMP_OUT (default $HOME/.cache/fibber-scratch/compare-types) as NAME.rust and NAME.fib.
# Run from the repository root. Each process runs under ulimit -v 4000000 and a 120 s timeout.
jobs=4; opts=""
while getopts "j:o:" o; do case $o in j) jobs=$OPTARG;; o) opts=$OPTARG;; esac; done
shift $((OPTIND-1)); rust=$1; tool=$2; shift 2
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/compare-types}; rm -rf "$out"; mkdir -p "$out"
one() {
  f=$1; n=$(echo "$f" | tr '/' '_')
  (ulimit -v 4000000; timeout 120 "$rust" types $opts "$f" > "$out/$n.rust" 2>&1; echo "status $?" >> "$out/$n.rust")
  (ulimit -v 4000000; timeout 120 "$tool" $opts "$f" > "$out/$n.fib" 2>&1; echo "status $?" >> "$out/$n.fib")
  if cmp -s "$out/$n.rust" "$out/$n.fib"; then rm -f "$out/$n.rust" "$out/$n.fib"; echo "same $f"; else echo "DIFF $f"; fi
}
export -f one; export rust tool opts out
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one {}' > "$out/tally.txt"
same=$(grep -c '^same' "$out/tally.txt"); diff=$(grep -c '^DIFF' "$out/tally.txt")
echo "files $((same+diff)): same $same, different $diff"
grep '^DIFF' "$out/tally.txt" | head -10
