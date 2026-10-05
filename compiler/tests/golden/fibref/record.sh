#!/usr/bin/env bash
# Records the reference outputs of the Rust `fibref` and `fibc itrace` at tag seed-1 for the port of the interpreter
# (docs/design/fibref-port.md section 9). Needs a Rust build of seed-1 that lives OUTSIDE the tree:
#   git archive seed-1 -o seed1.tar; mkdir src; tar -xf seed1.tar -C src
#   (cd src && CARGO_TARGET_DIR=$SCRATCH/target cargo build --release -p fibref)                     # about 15 s
#   (cd src && LLVM_SYS_211_PREFIX=/usr/lib/llvm-21 CARGO_TARGET_DIR=$SCRATCH/target cargo build --release -p fibc)   # for itrace
# usage: record.sh SRC BINDIR OUTDIR     (SRC the extracted seed-1 tree, BINDIR the target/release, OUTDIR this directory)
# One file per command, each program a block "=== NAME" then the output lines then "--- exit N".
set -u
# MAXLINES (default 400) caps the lines kept per program: a trace of a long run is millions of lines
SRC=$1; BIN=$2; OUT=$3
ulimit -v 12000000
export LC_ALL=C
cd "$SRC" || exit 2
block() { # block FILE NAME CMD ARGS..  (appends one block)
  local f=$1 n=$2; shift 2
  { echo "=== $n"; nice -n 10 timeout 60 "$@" 2>&1 | head -n "${MAXLINES:-400}"; echo "--- exit ${PIPESTATUS[0]}"; } >> "$f"
}
: > "$OUT/run-ownership.txt"; : > "$OUT/explain-ownership.txt"; : > "$OUT/itrace-ownership.txt"
for c in cases/ownership/*.fib; do
  n=$(basename "$c" .fib)
  block "$OUT/run-ownership.txt" "$n" "$BIN/fibref" run "$c"
  block "$OUT/explain-ownership.txt" "$n" "$BIN/fibref" explain "$c"
  [ -x "$BIN/fibc" ] && block "$OUT/itrace-ownership.txt" "$n" "$BIN/fibc" itrace "$c"
done
# modules: programs of several modules (the main.fib of each directory)
: > "$OUT/run-modules.txt"
for d in cases/modules/*/; do
  [ -f "$d/main.fib" ] && block "$OUT/run-modules.txt" "$(basename "$d")" "$BIN/fibref" run "$d/main.fib"
done
# the language server: transcripts of the cases in lsp-cases.txt are made by lsp-record.js
