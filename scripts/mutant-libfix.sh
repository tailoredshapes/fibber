#!/bin/bash
# scripts/mutant-libfix.sh: planted faults for package LIBFIX-1 (array-push! growth past 32, run-time keywords). Copies the tree to a scratch directory, breaks ONE
# rule in the copy, regenerates compiler/emit/runtime.fib from rt/, builds a stage 2 from the copy and runs the cases that pin the rule: every one must FAIL (a hung
# case is cut by the timeout and counts as failed). A case that still passes survived.
# usage: scripts/mutant-libfix.sh MODE
#   MODE          what is broken                                                                              cases that must fail
#   push-old      `fib.array-roomy?` says no from 32 elements on: every push copies again (quadratic)          8101-
#   push-shared   `fib.array-roomy?` does not test uniqueness: a shared array is pushed to in place            8101-
#   push-full     `fib.array-roomy?` says yes at a power of two: the push writes past the block                8101-
#   kw-no-literal `kw.intern` does not look at the keywords of the program: `(keyword "a")` is not `:a`       8104-
# environment: FIBC (a fibc that builds the mutant; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-libfix-MODE), MUT_TIMEOUT (seconds per
#   case, default 120). The cases run under `ulimit -v 16000000`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-libfix.sh push-old|push-shared|push-full|kw-no-literal}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-libfix-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-libfix: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
case $MODE in
  push-old|push-shared|push-full) CASES=(8101-) ;;
  kw-no-literal) CASES=(8104-) ;;
  *) echo "mutant-libfix: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-libfix: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  push-old)      mut rt/array.lir 's/\(block big\n    \(let \(\(m \(and n \(sub n \(i64 1\)\)\)\)\)\n      \(ret \(icmp ne m \(i64 0\)\)\)\)\)/(block big (ret (i1 0)))/' ;;
  push-shared)   mut rt/array.lir 's/\(br \(call \@fib\.unique\? a\) test no\)/(br (i1 1) test no)/' ;;
  push-full)     mut rt/array.lir 's/\(ret \(icmp ne m \(i64 0\)\)\)/(ret (i1 1))/' ;;
  kw-no-literal) mut compiler/emit/kwdyn.fib 's/\(icmp slt j \(i64 " ns "\)\) body miss/(icmp slt j (i64 0)) body miss/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-libfix[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-libfix: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
for p in "${CASES[@]}"; do
  res=$(timeout "${MUT_TIMEOUT:-120}" "$OUT/F" cases cases/stdlib --only "$p" 2>&1)
  rc=$?
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-220)
  if [ $rc = 124 ]; then echo "killed    $p (hung: cut by the ${MUT_TIMEOUT:-120} s timeout)"
  elif echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1
  else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-libfix[$MODE]: every case failed under the mutant"; else echo "mutant-libfix[$MODE]: a case survived: rewrite it"; fi
exit $survived
