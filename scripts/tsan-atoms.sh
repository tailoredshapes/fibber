#!/bin/bash
# scripts/tsan-atoms.sh: the atom cases under ThreadSanitizer (P-count-a; docs/design/parallelism.md 2.8, proto/mktsan.sh is the pipeline).
# Each case is emitted as lIR by a stage 2, turned into LLVM IR by `lairf emit-llvm`, instrumented with `opt -passes=tsan`, compiled with
# `llc` and linked with `gcc -fsanitize=thread`; the program runs and its unique `SUMMARY` lines are printed (none = no report) with its
# exit status (the case's result: 0 = every check held). `fib.mt`, the thread flag the runtime reads and writes without atomics (a known
# race, P-race fixes it), is made atomic in the IR first so that its report does not hide the others (the `patch` of proto/mktsan.sh).
# usage: scripts/tsan-atoms.sh CASE.fib..          environment: FIBC (a stage 2; required), LAIRF (compiler/lairf.fib built; required),
#   TSAN_OUT (scratch; default ~/.cache/fibber-scratch/tsan-atoms), TSAN_SED (see below)
# What this CAN find: data races on non-atomic memory (a plain store of a scalar atom beside an atomic load, a lock that does not lock so
# that object-atom reads race the stores, a use after free of a Vec the swap released). What it CANNOT find: ordering bugs among atomics
# (a `seq_cst` weakened to `monotonic` is not a race): those are argued in the comments of compiler/emit/lower/atoms.fib and rt/atom.lir.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-}; LAIRF=${LAIRF:-}
[ -x "$FIBC" ] && [ -x "$LAIRF" ] || { echo "tsan-atoms: set FIBC (a stage 2) and LAIRF (lairf built with it)" >&2; exit 2; }
OUT=${TSAN_OUT:-$HOME/.cache/fibber-scratch/tsan-atoms}; mkdir -p "$OUT"
LLVM=${LLVM_BIN:-/usr/lib/llvm-21/bin}
unset LD_LIBRARY_PATH
export FIB_LIB=${FIB_LIB:-$R/lib} TMPDIR=$OUT
bad=0
for src in "$@"; do
  name=$(basename "$src" .fib); b=$OUT/$name
  ( cd "$R" && "$FIBC" emit -I compiler -I lib -I cases/stdlib/support "$src" ) > "$b.lir" 2> "$b.emit.err" || { echo "$name: emit failed"; head -3 "$b.emit.err"; bad=2; continue; }
  # TSAN_SED: a sed expression applied to the lIR (the planted-race self-test: TSAN_SED='s/(atomic-store seq_cst /(store /' makes the stores of scalar atoms plain, which must be reported)
  [ -n "${TSAN_SED:-}" ] && sed -i -E "$TSAN_SED" "$b.lir"
  "$LAIRF" emit-llvm "$b.lir" > "$b.ll" 2> "$b.ll.err" || { echo "$name: emit-llvm failed"; head -3 "$b.ll.err"; bad=2; continue; }
  sed -i -E 's/^(\s*%[0-9a-z.]+ = )load i32, ptr @fib\.mt, align 4/\1load atomic i32, ptr @fib.mt monotonic, align 4/; s/^(\s*)store i32 1, ptr @fib\.mt, align 4/\1store atomic i32 1, ptr @fib.mt monotonic, align 4/' "$b.ll"
  sed -E 's/^(define [^{]*) \{$/\1 sanitize_thread {/' "$b.ll" > "$b.t.ll"
  "$LLVM/opt" -passes=tsan "$b.t.ll" -S -o "$b.tsan.ll" && "$LLVM/llc" -O1 -relocation-model=pic -filetype=obj "$b.tsan.ll" -o "$b.tsan.o" \
    && gcc -fsanitize=thread "$b.tsan.o" -o "$b.exe" -lm || { echo "$name: instrumenting failed"; bad=2; continue; }
  ( ulimit -v unlimited; MALLOC_ARENA_MAX=2 timeout "${TSAN_TIMEOUT:-600}" "$b.exe" > "$b.run" 2>&1; echo "exit $?" >> "$b.run" )
  reports=$(grep -E 'SUMMARY' "$b.run" | sed 's/(BuildId.*//; s/ThreadSanitizer: //' | sort | uniq -c)
  echo "== $name: $(tail -1 "$b.run"), $(grep -c 'WARNING: ThreadSanitizer' "$b.run") reports"
  [ -n "$reports" ] && { echo "$reports"; bad=1; }
done
exit $bad
