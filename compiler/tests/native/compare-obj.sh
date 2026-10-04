#!/bin/bash
# Optimisation and the target: for each FILE at each -O 0..3 (a unit is FILE@N), `lair build FILE -o OBJ --emit obj -O N` against
# `lairf build ...` with FIB_TARGET_CPU=x86-64-v2 on both: the bytes of the object file, plus standard error and exit status. A file the
# checker refuses compares its diagnostics. LEVELS="0 1 2 3" by default. See lib.sh for the rest.
# usage: compare-obj.sh [-j N] FILE-or-DIR..       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-obj
. "$(dirname "$0")/lib.sh"
units() {
  local f l
  for f in $(for a in "$@"; do if [ -d "$a" ]; then find "$a" -name '*.lir' | sort; else echo "$a"; fi; done); do
    for l in ${LEVELS:-0 1 2 3}; do echo "$f@$l"; done
  done
}
# obj_run TOOL UNIT OUT: the capture, then the object (if one was written) after the text
obj_run() {
  local tool=$1 f=${2%@*} l=${2##*@} out=$3
  rm -f "$out.obj"
  capture "$out" "$tool" build "$f" -o "$out.obj" --emit obj -O "$l"
  if [ -f "$out.obj" ]; then { echo "--obj"; cat "$out.obj"; } >> "$out"; rm -f "$out.obj"; fi
}
rust_run() { obj_run "$LAIR" "$1" "$2"; }
fib_run() { obj_run "$LAIRF" "$1" "$2"; }
compare_main "$@"
