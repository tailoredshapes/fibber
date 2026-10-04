#!/bin/bash
# scripts/mutant-amp-param.sh: the mutation review of the batch 6 ownership rules (spec/types.md §6.6 "Taking the content" for an `&` parameter,
# §6.4 "Declared owned"). Copies compiler/ and lib/ of this tree to a scratch directory, breaks ONE rule of the checker in the copy, builds a
# stage 2 from it and runs the cases that pin the rule: every one of them must FAIL under the mutant (a wrong answer, an allocation count, an audit
# error or a crash all count). A case that still passes survived the mutant and is a bad case. Nothing in the tree changes.
#
# usage: scripts/mutant-amp-param.sh MODE
#   MODE      what is broken                                                              cases that must fail
#   mention   a taken `&b` ignores the siblings and the closure captures of `b`           ownership/301- (and 262-)
#   param     an `&` parameter is taken whatever the last-use pass says (any mention)     ownership/301-
#   release   the write-back of a taken `&b` releases the old content, which was moved    ownership/300- 302- 303-
#   writeback a taken `&b` has no write-back: the callee's result is lost                 ownership/303- 304-
#   owned     `:owned` on a `defun` parameter is parsed and ignored                       compiler/tests/own/owned-param.sh (the `explain` golden)
#   lastuse   an owned argument is handed over whether or not it is the variable's last use   ownership/313- 315- 316- (and others)
#   inplace   `array-with` writes in place whether or not its operand is unique           ownership/313- 314- 317- 318-
#   leak      the copying branch of `array-with` does not release the operand it consumed     ownership/314- 317-
# environment: FIBC (a stage 2 or the seed that builds the mutant; default the gate's F of this tree), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-amp-param-MODE), MUT_J (cases at once, default 2).
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by an
#   ordinary `F cases cases/ownership --only 300- ..` of the tree's own stage 2; this script does not repeat it.
set -uo pipefail
MODE=${1:?usage: mutant-amp-param.sh mention|param|release|writeback|owned|lastuse|inplace|leak}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-amp-param-$MODE}
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
FIBC=${FIBC:-$gate_f}
[ -x "$FIBC" ] || { echo "mutant-amp-param: no fibc to build with: set FIBC" >&2; exit 2; }
unset LD_LIBRARY_PATH

case $MODE in
  mention)   CASES=(301- 262-) ;;
  param)     CASES=(301-) ;;
  release)   CASES=(300- 302- 303-) ;;
  writeback) CASES=(303- 304-) ;;
  owned)     CASES=() ;;
  lastuse)   CASES=(313- 315- 316-) ;;
  inplace)   CASES=(313- 314- 317- 318-) ;;
  leak)      CASES=(314- 317-) ;;
  *) echo "mutant-amp-param: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"
cp -r "$R/cases/ownership" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-amp-param: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  mention)
    mut compiler/own/lastuse.fib 's/\(when \(and \(not \(contains\? sibs \(\* b 8\)\)\) \(not \(contains\? \(\. \(\. u cx\) pinned\) b\)\)\)/(when true/' ;;
  param)
    mut compiler/own/walk/call.fib 's/\(and \(contains\? \(\. \(\. w cx\) amp\) \(\+ \(\* \(\. cs call\) 64\) i\)\)\s*\(or \(w-movable w \(SiteBind b\)\) \(forwards\? w b\)\)\)/(or (and (contains? (. (. w cx) amp) (+ (* (. cs call) 64) i)) (w-movable w (SiteBind b))) (forwards? w b))/' ;;
  release)
    mut compiler/emit/lower/call.fib 's/\(when \(not took\) \(lcx-release cx old\)\)/(lcx-release cx old)/' ;;
  writeback)
    mut compiler/own/walk/call.fib 's/\(or \(= pass PsAcquire\) \(= pass PsMoveIn\)\) \(conj wbs/(= pass PsAcquire) (conj wbs/' ;;
  owned)
    mut compiler/own/top.fib 's/\(and \(\. d owned\) \(not \(\. d amp\)\)/(and false (. d owned) (not (. d amp))/' ;;
  inplace)
    mut compiler/emit/lower/builtins.fib 's/\(unique \(fb-val \(\. cx b\) \(str-join \["\(call \@fib\.unique\? " \(v-text \(nth a 0\)\) "\)"\]\) LirI1\)\)\n\s*\(li \(fb-label \(\. cx b\) "wi-inplace"\)\)/(unique (fb-val (. cx b) "(icmp eq (i64 0) (i64 0))" LirI1))\n        (li (fb-label (. cx b) "wi-inplace"))/' ;;
  leak)
    mut compiler/emit/lower/builtins.fib 's/\(lcx-release cx \(nth a 0\)\)\n\s*\(fb-term b \(str-join \["\(br " lj "\)"\]\)\)\)\)\n\s*\(fb-open b lj\)/(fb-term b (str-join ["(br " lj ")"]))))\n        (fb-open b lj)/' ;;
  lastuse)
    mut compiler/own/walk/call.fib 's/\(\(MBorrowed s\) :when \(w-last-use\? w x s\)/((MBorrowed s) :when (w-movable w s)/' ;;
esac

cd "$OUT/tree" || exit 2
echo "mutant-amp-param[$MODE]: building the mutant stage 2 with $FIBC"
export FIB_LIB=$OUT/tree/lib
"$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F" > "$OUT/build.log" 2>&1 \
  || { echo "mutant-amp-param: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
if [ "$MODE" = owned ]; then
  if "$R/compiler/tests/own/owned-param.sh" "$OUT/F" > "$OUT/golden.log" 2>&1; then echo "SURVIVED  owned-param.sh passes under the mutant"; survived=1
  else echo "killed    owned-param.sh: $(grep DIFF "$OUT/golden.log" | tr '\n' ' ')"; fi
fi
for p in "${CASES[@]}"; do
  res=$("$OUT/F" cases cases/ownership --only "$p" -j "${MUT_J:-2}" 2>&1)
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-170)
  if echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1; else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-amp-param[$MODE]: every case failed under the mutant"; else echo "mutant-amp-param[$MODE]: a case survived: rewrite it"; fi
exit $survived
