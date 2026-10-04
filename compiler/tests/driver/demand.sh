#!/bin/bash
# The library is checked on demand by `emit`, `run` and `build` (types.infer.demand: only the units of the library the program can reach are
# inferred and analysed) and in full by `emit-dump` (every unit): the two must give the same lIR. This compares `FIBC emit FILE` with the
# sections of `FIBC emit-dump FILE`, minus its marker lines (`== FILE` and `;; == section NAME`), and with no file takes a sample of the cases.
# A unit the demand leaves out that the emitter then asks for shows as a rejection or a difference here. It exits 1 on any difference.
# usage: demand.sh FIBC [FILE..]    run from the repository root, FIB_LIB=lib and LD_LIBRARY_PATH holding liblair.so in the environment
fibc=${1:?usage: demand.sh FIBC [FILE..]}; shift
if [ $# -eq 0 ]; then
  set -- scripts/bench/hello.fib cases/ownership/01-return-part-of-argument.fib cases/stdlib/001-*.fib cases/stdlib/002-*.fib \
    cases/modules/004-diamond-loads-once/main.fib
fi
roots_of() {
  for d in $(sed -n 's/^;; roots: *//p' "$1" | head -1); do
    case $d in /*) echo "-I $d";; *) echo "-I $(dirname "$1")/$d";; esac
  done
}
bad=0; n=0
for f in "$@"; do
  r=$(roots_of "$f"); n=$((n + 1))
  # stage 2's emit starts with the module's (target ..) form (SIMD P3); emit-dump prints the sections only
  a=$( "$fibc" emit $r "$f" 2>&1 | sed '1{/^(target /d}'; echo "status ${PIPESTATUS[0]}" )
  b=$( "$fibc" emit-dump $r "$f" 2>&1 | grep -v '^== \|^;; == section '; echo "status ${PIPESTATUS[0]}" )
  if [ "$a" = "$b" ]; then echo "same $f"; else echo "DIFF $f"; bad=1; fi
done
echo "demand.sh: $n files, $([ $bad = 0 ] && echo all the same || echo DIFFERENCES)"
exit $bad
