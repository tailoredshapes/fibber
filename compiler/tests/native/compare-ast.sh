#!/bin/bash
# The reader: for each .lir FILE (or each one under a DIRECTORY), `lair dump-ast` (the Rust, the oracle) against `lairf dump-ast`, byte for
# byte over standard output, standard error and exit status (a file that does not parse compares its diagnostic line). Format:
# docs/design/lair-ast-dump.md. See lib.sh for the environment, the verdicts and the exit status.
# usage: compare-ast.sh [-j N] FILE-or-DIR..       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-ast
. "$(dirname "$0")/lib.sh"
rust_run() { capture "$2" "$LAIR" dump-ast "$1"; }
fib_run() { capture "$2" "$LAIRF" dump-ast "$1"; }
compare_main "$@"
