#!/bin/bash
# The records of the programs that do not reach the ownership pass: compare the output of
# compiler/own.fib, built, with `fibref ORACLE` on every file that `fibref types` rejects or cannot
# read (exit 1 or 2), byte for byte with the exit status (the unreadable line, the expander's record,
# the checker's `error` records). The files `fibref types` accepts are not compared: they reach
# `analyse`, which is a stub (`trap: todo: analyse`) until package O7; the tally counts them.
# usage: records.sh [-c ORACLE] [-j N] FIBREF TOOL "OPTIONS" FILE..
#   -c  the Rust subcommand compared with (default `own`; `types` compares the type checker's records)
#   FIBREF the Rust tool, TOOL the built compiler/own.fib; OPTIONS are given to all (e.g. "--sections error")
# Output: a tally and the first five differing files; both outputs of a difference are kept in
# $CMP_OUT (default $HOME/.cache/fibber-scratch/compare-records) as NAME.rust and NAME.fib.
# Run from the repository root. Each process runs under ulimit -v 4000000 and a 120 s timeout.
oracle=own; jobs=4
while getopts "c:j:" o; do case $o in c) oracle=$OPTARG;; j) jobs=$OPTARG;; esac; done
shift $((OPTIND-1)); rust=$1; tool=$2; opts=$3; shift 3
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/compare-records}; rm -rf "$out"; mkdir -p "$out"
one() {
  f=$1; n=$(echo "$f" | tr '/' '_')
  (ulimit -c 0; ulimit -v 4000000; timeout 120 "$rust" types $opts "$f" > "$out/$n.types" 2>&1; echo "status $?" >> "$out/$n.types")
  if [ "$(tail -1 "$out/$n.types")" = "status 0" ]; then rm -f "$out/$n.types"; echo "accepted $f"; return; fi
  rm -f "$out/$n.types"
  (ulimit -c 0; ulimit -v 4000000; timeout 120 "$rust" "$oracle" $opts "$f" > "$out/$n.rust" 2>&1; echo "status $?" >> "$out/$n.rust")
  (ulimit -c 0; ulimit -v 4000000; timeout 120 "$tool" $opts "$f" > "$out/$n.fib" 2>&1; echo "status $?" >> "$out/$n.fib")
  if cmp -s "$out/$n.rust" "$out/$n.fib"; then rm -f "$out/$n.rust" "$out/$n.fib"; echo "same $f"; else echo "DIFF $f"; fi
}
export -f one; export rust tool opts out oracle
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one {}' > "$out/tally.txt"
same=$(grep -c '^same' "$out/tally.txt"); diff=$(grep -c '^DIFF' "$out/tally.txt"); acc=$(grep -c '^accepted' "$out/tally.txt")
echo "files $((same+diff+acc)): rejected or unreadable $((same+diff)): same $same, different $diff; accepted, not compared $acc"
grep '^DIFF' "$out/tally.txt" | head -5
