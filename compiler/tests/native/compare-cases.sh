#!/bin/bash
# The JIT and the case harness: for each DIRECTORY of cases (default: each directory directly under cases/lir), `lair cases DIR` against
# `lairf cases DIR`: the verdict table, line by line, with the exit status. A unit is a directory, not a file. See lib.sh.
# usage: compare-cases.sh [-j N] [DIR..]       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-cases
. "$(dirname "$0")/lib.sh"
units() {
  if [ $# -eq 0 ]; then set -- $(find cases/lir -mindepth 1 -maxdepth 1 -type d | sort); fi
  local d
  # a directory whose every case says `;; stage: 2` is for the fibber lair alone (see lib.sh `skip_stage2`)
  for d in "$@"; do
    [ -d "$d" ] || continue
    if [ -n "$(find "$d" -name '*.lir' | xargs grep -L '^;; stage: *2' 2>/dev/null)" ]; then echo "$d"; fi
  done
}
rust_run() { capture "$2" "$LAIR" cases "$1"; }
fib_run() { capture "$2" "$LAIRF" cases "$1"; }
compare_main "$@"
