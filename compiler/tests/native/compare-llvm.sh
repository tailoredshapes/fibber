#!/bin/bash
# The lowering: `lair emit-llvm FILE` (the verified, unoptimised module as LLVM prints it, for FIB_TARGET_CPU=x86-64-v2) against
# `lairf emit-llvm FILE`: the whole text, and the diagnostics of a module the checker refuses, with the exit status. See lib.sh.
# usage: compare-llvm.sh [-j N] FILE-or-DIR..       run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
NAME=compare-llvm
. "$(dirname "$0")/lib.sh"
rust_run() { capture "$2" "$LAIR" emit-llvm "$1"; }
fib_run() { capture "$2" "$LAIRF" emit-llvm "$1"; }
compare_main "$@"
