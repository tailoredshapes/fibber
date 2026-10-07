#!/bin/bash
# scripts/mutant-blit.sh: the mutation review of `array-blit!` (spec/types.md 2.13.3, docs/adr/0014). Copies compiler/, lib/ and rt/ of this tree to
# a scratch directory, plants one fault at a time in the lowering (compiler/emit/lower/builtins.fib) or the runtime (rt/array.lir and its generated copy
# compiler/emit/runtime.fib), builds a stage 2 from the copy (nothing in the tree changes) and runs cases 8140 to 8142 against it: each mutant must make one
# of them FAIL. A case that still passes survived and is a bad case.
#   bounds   the range check never fires (8141 and 8142 expect the trap)
#   short    one element too few is copied (8140)
#   shared   the unique test of `array-own` always says yes: a shared array is written (8140, the array another variable holds)
#   esize    the element size is taken as 1 (8140, the i64 array)
# usage: FIBC=<a stage 2 of this tree> scripts/mutant-blit.sh [MUTANT..]      (default: all four)   MUT_OUT: scratch (default ~/.cache/fibber-scratch/mutant-blit)
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by `F cases cases/stdlib --only 8140`.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-blit}
FIBC=${FIBC:?set FIBC to a stage 2 of this tree}
[ -x "$FIBC" ] || { echo "mutant-blit: no fibc to build with: $FIBC" >&2; exit 2; }
MUTANTS=("$@"); [ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(bounds short shared esize)
ulimit -v 16000000 || true
unset LD_LIBRARY_PATH
plant() { # plant FILE PERL-SUBSTITUTION: the file must change
  local f=$1 before; before=$(cksum < "$f"); perl -0pi -e "$2" "$f"
  [ "$before" != "$(cksum < "$f")" ] || { echo "mutant-blit: the plant did not change $f: the source moved" >&2; exit 2; }
}
survived=0
for m in "${MUTANTS[@]}"; do
  T=$OUT/$m; rm -rf "$T"; mkdir -p "$T/tree/cases/stdlib" "$T/tmp"; export TMPDIR=$T/tmp
  cp -r "$R/compiler" "$R/lib" "$R/rt" "$T/tree/"
  cp -r "$R/cases/stdlib/support" "$T/tree/cases/stdlib/"; cp "$R"/cases/stdlib/814[0-2]-*.fib "$T/tree/cases/stdlib/"
  C=$T/tree/compiler/emit
  case $m in
    bounds) for f in "$T/tree/rt/array.lir" "$C/runtime.fib"; do plant "$f" 's/(fib\.array-blit void.*?)\(br bad range ok\)/$1(br (i1 0) range ok)/s'; done;;
    short)  for f in "$T/tree/rt/array.lir" "$C/runtime.fib"; do plant "$f" 's/\(call \@memmove d s \(mul n esize\)\)/(call \@memmove d s (mul (sub n (i64 1)) esize))/'; done;;
    shared) plant "$C/lower/builtins.fib" 's/\(do \(fb-term b \(str-join \["\(br " \(v-text ok\) " " lj/(do (fb-term b (str-join ["(br " "(i1 1)" " " lj/';;
    esize)  plant "$C/lower/builtins.fib" 's/(\(v-text \(nth a 4\)\) " \(i64 " )\(str \(\. r esize\)\)/$1(str 1)/';;
    *) echo "mutant-blit: unknown mutant $m" >&2; exit 2;;
  esac
  ( cd "$T/tree" && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$T/F" ) || { echo "mutant-blit[$m]: the mutant did not build" >&2; exit 2; }
  out=$( cd "$T/tree" && FIB_LIB=$T/tree/lib "$T/F" cases cases/stdlib --only 8140 8141 8142 2>&1 ); echo "$out" | tail -n 6
  if echo "$out" | grep -q ' 0 fail'; then echo "mutant-blit[$m]: SURVIVED"; survived=1; else echo "mutant-blit[$m]: killed"; fi
done
exit $survived
