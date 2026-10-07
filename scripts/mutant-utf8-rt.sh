#!/bin/bash
# scripts/mutant-utf8-rt.sh: planted faults in the runtime's UTF-8 validator (rt/str.lir, embedded in the compiler as compiler/emit/runtime.fib; docs/design/json.md 7).
# Each fault is put into a copy of compiler/ and lib/, a compiler is built from the copy (about a minute and a half each), and case 8070 (every UTF-8 sequence at every offset of a text) is run with it:
# it must FAIL. A fault under which 8070 passes means the validator has a part no test can fail.
#   word-mask         the ASCII word test ignores the top byte of the word (0x0080808080808080 for 0x8080808080808080)
#   three-byte-cont   the second continuation byte of a three-byte sequence is not checked
#   two-byte-range    C0 and C1 leads are taken as two-byte sequences
# usage: scripts/mutant-utf8-rt.sh [MUTANT..]     environment: FIBC (a stage 2 or a release that can build compiler/fibc.fib), MUT_OUT (scratch). exit 0 when every fault was killed.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-utf8-rt: no fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-utf8-rt}
MUTANTS=("$@"); [ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(word-mask three-byte-cont two-byte-range)
survived=0
for m in "${MUTANTS[@]}"; do
  rm -rf "$OUT"; mkdir -p "$OUT"; cp -r "$R/compiler" "$R/lib" "$OUT/"
  f=$OUT/compiler/emit/runtime.fib
  case $m in
    word-mask)       perl -0pi -e 's/\(and w \(i64 -9187201950435737472\)\)/(and w (i64 36170086419038336))/' "$f" ;;
    three-byte-cont) perl -0pi -e 's/\(icmp eq \(and d2 \(i32 192\)\) \(i32 128\)\)/(i1 1)/' "$f" ;;
    two-byte-range)  perl -0pi -e 's/\(icmp uge b0 \(i32 194\)\)/(icmp uge b0 (i32 192))/' "$f" ;;
    *) echo "mutant-utf8-rt: unknown mutant $m" >&2; exit 2 ;;
  esac
  cmp -s "$f" "$R/compiler/emit/runtime.fib" && { echo "SETUP ERROR $m: pattern not found" >&2; exit 2; }
  (cd "$OUT" && FIB_LIB=$OUT/lib "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F9" > "$OUT/build.log" 2>&1) || { echo "SETUP ERROR $m: build failed"; exit 2; }
  out=$(cd "$R" && FIB_LIB=$R/lib "$OUT/F9" cases cases/stdlib --only 8070- 2>&1 || true)
  if printf '%s' "$out" | grep -q "FAIL"; then echo "killed   $m"; else echo "SURVIVED $m"; survived=1; fi
done
rm -rf "$OUT"
[ $survived = 0 ]
