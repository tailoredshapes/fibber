#!/bin/bash
# The checker: `lair check FILE` against `lairf check FILE`: standard error (every diagnostic, in order, with its position) and exit
# status, byte for byte. See lib.sh for the environment, the verdicts and the exit status.
# usage: compare-check.sh [-j N] FILE-or-DIR..       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-check
. "$(dirname "$0")/lib.sh"
rust_run() { capture "$2" "$LAIR" check "$1"; }
fib_run() { capture "$2" "$LAIRF" check "$1"; }
compare_main "$@"
