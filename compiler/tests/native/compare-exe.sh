#!/bin/bash
# Executables (package A): for each FILE, `lair build FILE -o EXE -O 2` against `lairf build ...` (the way `lair cases` builds the AOT path of a case),
# then the executable is run: what is compared is the build's standard error and exit status, and when an executable was made its sha256, its exit
# status, its standard output and its standard error. A case the build refuses compares the diagnostics. See lib.sh for the rest.
# usage: compare-exe.sh [-j N] FILE-or-DIR..       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-exe
. "$(dirname "$0")/lib.sh"
# exe_run TOOL UNIT OUT: the capture of the build, then, if an executable was written, its hash and the capture of its run
exe_run() {
  local tool=$1 f=$2 out=$3
  rm -f "$out.exe"
  capture "$out" "$tool" build "$f" -o "$out.exe" -O 2
  if [ -f "$out.exe" ]; then
    { echo "--exe $(sha256sum < "$out.exe")"; } >> "$out"
    capture "$out.run" "$out.exe"
    { echo "--run"; cat "$out.run"; } >> "$out"
    rm -f "$out.exe" "$out.run" "$out.run.err" "$out.run.status"
  fi
}
rust_run() { exe_run "$LAIR" "$1" "$2"; }
fib_run() { exe_run "$LAIRF" "$1" "$2"; }
compare_main "$@"
