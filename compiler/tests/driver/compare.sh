#!/bin/bash
# Compare the compiler in fibber (compiler/fibc.fib, "stage 2") with the Rust one (`fibc`, "stage 1") on a command, one process
# pair per program: the standard output, the standard error and the exit status of `STAGE1 COMMAND [OPTIONS] FILE` and `STAGE2 COMMAND
# [OPTIONS] FILE` must be the same, byte for byte (the stage-2 compiler emits what stage 1 emits; a program it rejects is rejected with the same text).
# usage: compare.sh [-j N] [-c COMMAND] [-o "OPTIONS"] [-t SECONDS] STAGE1 STAGE2 FILE..
#   -c  the command: emit (the default), explain, run, or emit-dump (with -o "--sections fns" and so on)
#   -o  words between the command and the file, given to both
#   A case's header `;; roots: DIR..` is given as `-I DIR` for each, relative to the case's directory (what the harness does).
#   -t  the timeout of each process, default 300
# Output: a tally and the first ten differing files; the outputs of each difference are kept in $CMP_OUT (default
# $HOME/.cache/fibber-scratch/driver-COMMAND) as NAME.out1, NAME.err1, NAME.out2, NAME.err2 and NAME.status.
# Run from the repository root, with FIB_LIB=lib and LD_LIBRARY_PATH holding liblair.so (target/debug) in the environment. Each process runs
# under ulimit -v 6000000 (no core files). A `run` that does not end is cut by the timeout in both (status 124 in both is the same).
jobs=4; opts=""; cmd=emit; secs=300
export FIB_TARGET_CPU=${FIB_TARGET_CPU:-x86-64}   # the golden output does not depend on the machine
while getopts "j:o:c:t:" o; do case $o in j) jobs=$OPTARG;; o) opts=$OPTARG;; c) cmd=$OPTARG;; t) secs=$OPTARG;; esac; done
shift $((OPTIND-1)); s1=$1; s2=$2; shift 2
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/driver-$cmd}; rm -rf "$out"; mkdir -p "$out"
roots_of() { # the `-I dir` words of the header `;; roots: DIR..` of a case (each relative to the case's directory), as the harness gives them
  for d in $(sed -n 's/^;; roots: *//p' "$1" | head -1); do case $d in /*) echo "-I $d";; *) echo "-I $(dirname "$1")/$d";; esac; done
}
one() {
  f=$1; n=$(echo "$f" | tr '/' '_'); r=$(roots_of "$f")
  (ulimit -c 0 -v 6000000; timeout "$secs" "$s1" $cmd $r $opts "$f" < /dev/null > "$out/$n.out1" 2> "$out/$n.err1"; echo "status $?" > "$out/$n.st1")
  (ulimit -c 0 -v 6000000; timeout "$secs" "$s2" $cmd $r $opts "$f" < /dev/null > "$out/$n.out2" 2> "$out/$n.err2"; echo "status $?" > "$out/$n.st2")
  # stage 2's emit starts with the module's (target ..) form (SIMD P3, FIB_TARGET_CPU pinned above), which the Rust compiler does not know
  if [ "$cmd" = emit ]; then sed -i '1{/^(target /d}' "$out/$n.out2"; fi
  if cmp -s "$out/$n.out1" "$out/$n.out2" && cmp -s "$out/$n.err1" "$out/$n.err2" && cmp -s "$out/$n.st1" "$out/$n.st2"; then
    rm -f "$out/$n".*; echo "same $f"
  else
    cat "$out/$n.st1" "$out/$n.st2" | tr '\n' ' ' > "$out/$n.status"; echo "DIFF $f $(cat "$out/$n.status")"
  fi
}
export -f one roots_of; export s1 s2 opts out cmd secs
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one {}' > "$out/tally.txt" 2> /dev/null
same=$(grep -c '^same' "$out/tally.txt"); diff=$(grep -c '^DIFF' "$out/tally.txt")
echo "files $((same+diff)): same $same, different $diff"
grep '^DIFF' "$out/tally.txt" | head -10
[ "$diff" -eq 0 ]
