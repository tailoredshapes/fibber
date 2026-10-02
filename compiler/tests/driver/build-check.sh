#!/bin/bash
# `build` of the compiler in fibber (compiler/fibc.fib, "stage 2") against the Rust compiler's ("stage 1"), on programs: for each FILE
# build it with both (`STAGE build FILE -o EXE`), and require the same compile status and the same text on standard error (a rejected program
# is rejected in the same words); then run both executables (no arguments, a timeout) and require the same standard output, the same
# standard error and the same exit status (a trap, a result, an audit failure alike). With -T the executables run again with FIB_TRACE=1,
# whose heap trace on standard error must be the same too (unless the Rust executable's own two runs differ: threads race).
# A case's header `;; roots: DIR..` is given as `-I DIR` for each, relative to the case's directory (what the harness does).
# usage: build-check.sh [-j N] [-T] [-r SECONDS] [-O N] STAGE1 STAGE2 FILE..
#   -j  files in parallel (default 4)      -T  also compare the heap trace   -r  timeout of each executable run (default 20)
#   -O  the optimisation level given to both (default: the compilers' own, 2)
# Output: a tally and the first ten differing files; the outputs of each difference are kept in $CMP_OUT (default
# $HOME/.cache/fibber-scratch/driver-build) as NAME.{out,err,st}{1,2} (compile) and NAME.run{out,err,st}{1,2}. Executables are made under
# $CMP_OUT/exe and removed. Run from the repository root with FIB_LIB=lib and LD_LIBRARY_PATH holding liblair.so in the environment.
# Exit status 0 only if every file was the same.
jobs=4; trace=0; secs=20; olevel=""
while getopts "j:Tr:O:" o; do case $o in j) jobs=$OPTARG;; T) trace=1;; r) secs=$OPTARG;; O) olevel="-O $OPTARG";; esac; done
shift $((OPTIND-1)); s1=$1; s2=$2; shift 2
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/driver-build}; rm -rf "$out"; mkdir -p "$out/exe"
roots_of() { # the `-I dir` words of the header `;; roots: DIR..` of a case (each relative to the case's directory), as the harness gives them
  for d in $(sed -n 's/^;; roots: *//p' "$1" | head -1); do case $d in /*) echo "-I $d";; *) echo "-I $(dirname "$1")/$d";; esac; done
}
runit() { # runit EXE PREFIX: stdout, stderr, status of one run
  (ulimit -c 0 -v 4000000; timeout "$secs" "$1" > "$2.out" 2> "$2.err" < /dev/null; echo "status $?" > "$2.st")
}
same3() { cmp -s "$1.out" "$2.out" && cmp -s "$1.err" "$2.err" && cmp -s "$1.st" "$2.st"; }
one() {
  f=$1; n=$(echo "$f" | tr '/' '_'); p="$out/$n"; why=""; r=$(roots_of "$f")
  for k in 1 2; do
    eval s=\$s$k
    (ulimit -c 0 -v 6000000; timeout 600 "$s" build $olevel $r "$f" -o "$out/exe/$n.$k" > "$p.out$k" 2> "$p.err$k"; echo "status $?" > "$p.st$k")
  done
  cmp -s "$p.out1" "$p.out2" && cmp -s "$p.err1" "$p.err2" && cmp -s "$p.st1" "$p.st2" || why="compile"
  if [ -z "$why" ] && [ -x "$out/exe/$n.1" ]; then
    runit "$out/exe/$n.1" "$p.run1"; runit "$out/exe/$n.2" "$p.run2"
    same3 "$p.run1" "$p.run2" || why="run"
    if [ -z "$why" ] && [ "$trace" = 1 ]; then
      # A program with threads or atoms races: its trace is only compared when the Rust executable repeats its own.
      export FIB_TRACE=1
      runit "$out/exe/$n.1" "$p.trace1"; runit "$out/exe/$n.1" "$p.again1"; runit "$out/exe/$n.2" "$p.trace2"
      unset FIB_TRACE
      if same3 "$p.trace1" "$p.again1"; then same3 "$p.trace1" "$p.trace2" || why="trace"; fi
    fi
  fi
  rm -f "$out/exe/$n".1 "$out/exe/$n".2
  if [ -z "$why" ]; then rm -f "$p".*; echo "same $f"; else echo "DIFF $f ($why)"; fi
}
export -f one runit same3 roots_of; export s1 s2 out trace secs olevel
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one {}' > "$out/tally.txt" 2> /dev/null
same=$(grep -c '^same' "$out/tally.txt"); diff=$(grep -c '^DIFF' "$out/tally.txt")
echo "files $((same+diff)): same $same, different $diff"
grep '^DIFF' "$out/tally.txt" | head -10
[ "$diff" -eq 0 ]
