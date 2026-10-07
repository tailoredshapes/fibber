#!/bin/bash
# scripts/mutant-uninit-i8.sh: a planted fault in the builtin `array-uninit-i8` (ADR 0014: a builtin has a mutant): the lowering of the i8 form skips the length check that traps a negative or
# overflowing length. Copies compiler/ and lib/ to a scratch directory, applies the mutant, builds a stage 2 from the copy with FIBC (a stage 2 that has the builtin: default the gate's F) and
# runs cases 8506 to 8508: 8508 (trap on a negative length) must FAIL. About two minutes (a compiler build). usage: scripts/mutant-uninit-i8.sh
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-uninit-i8: no stage 2 fibc: set FIBC" >&2; exit 2; }
T=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-uninit-i8}
rm -rf "$T"; mkdir -p "$T/cases/stdlib"
cp -r "$R/compiler" "$T/compiler"; cp -r "$R/lib" "$T/lib"; cp -r "$R/cases/stdlib/support" "$T/cases/stdlib/"; cp "$R"/cases/stdlib/850[678]-*.fib "$T/cases/stdlib/"
f=$T/compiler/emit/lower/builtins.fib
perl -0pi -e 's/\("array-uninit-i8" \(Ok \(array-uninit cx r \(nth a 0\)\)\)\)/("array-uninit-i8" (Ok (fb-val (. cx b) (str-join ["(call \@fib.array-alloc " (v-text (nth a 0)) " (i32 " (str (. r tid)) ") (i64 " (str (. r esize)) "))"]) LirPtr)))/' "$f"
grep -q 'fib.array-alloc " (v-text (nth a 0))' "$f" || { echo "mutant-uninit-i8: pattern not found" >&2; exit 2; }
(cd "$T" && FIB_LIB=$T/lib "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$T/F" > "$T/build.log" 2>&1) || { echo "mutant build failed: $(tail -3 "$T/build.log")"; exit 2; }
(cd "$T" && FIB_LIB=$T/lib "$T/F" cases cases/stdlib > "$T/cases.log" 2>&1) && rc=0 || rc=$?
if [ $rc -ne 0 ] || ! grep -q "0 fail, 0 pending, 0 header error" "$T/cases.log"; then echo "killed   array-uninit-i8 without the length check   ($(grep -m1 ' FAIL ' "$T/cases.log" | cut -c1-120))"; rm -rf "$T/compiler" "$T/lib"; exit 0
else echo "SURVIVED array-uninit-i8 without the length check"; exit 1; fi
