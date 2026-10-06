#!/bin/bash
# scripts/mutant-catch.sh: planted faults for stage 2 of exceptions (docs/design/exceptions.md 4.2 and 9, docs/adr/0009): try, catch,
# finally and throw over a failure returned next to every function's value. Copies the tree to a scratch directory, breaks ONE rule in
# the copy, regenerates compiler/emit/runtime.fib from rt/, builds a stage 2 from the copy and runs the cases that pin the rule: every one
# must FAIL under the mutant (a wrong answer, an aborted run, an audit leak or error all count). A case that still passes survived and is
# a bad case.
# usage: scripts/mutant-catch.sh MODE
#   MODE                 what is broken                                                                  cases that must fail
#   no-release           an unwinding frame does not release what the plan says it holds                 7870- 7874- 7882- 7885-
#   no-pending           the evaluated arguments of a call being made are not released                   7870-
#   double-release       an unwinding frame releases what the plan says it holds twice                   7871-
#   finally-catchable    a finally clause run while unwinding is an ordinary region                      7872-
#   no-catch             catch-run answers 0 (returned) for a failure                                    7873- 7874- 7879- 7884- 7887- 7888- 7891-
#   catch-everywhere     a trap unwinds even where no catch is active on its thread                      7875- 7883-
#   take-unwinds         a function with an array-take! passes failures on                               7876-
#   update-unwinds       a failure of cell-update!'s function is passed on                                7877-
#   no-write-back        the & arguments of a failed call are not written back                           7878-
#   swap-keeps-snapshot  swap!'s snapshot is not released when its function fails                       7880-
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-catch-MODE), MUT_J (cases at once, default 1). The cases run under `ulimit -v 16000000`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by an
#   ordinary `F cases cases/stdlib --only 787 788 789` of the tree's own stage 2; this script does not repeat it.
set -uo pipefail
MODE=${1:?usage: mutant-catch.sh no-release|no-pending|double-release|finally-catchable|no-catch|catch-everywhere|take-unwinds|update-unwinds|no-write-back|swap-keeps-snapshot}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-catch-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-catch: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  no-release)          CASES=(7870- 7874- 7882- 7885-) ;;
  no-pending)          CASES=(7870-) ;;
  double-release)      CASES=(7871-) ;;
  finally-catchable)   CASES=(7872-) ;;
  no-catch)            CASES=(7873- 7874- 7879- 7884- 7887- 7888- 7891-) ;;
  catch-everywhere)    CASES=(7875- 7883-) ;;
  take-unwinds)        CASES=(7876-) ;;
  update-unwinds)      CASES=(7877-) ;;
  no-write-back)       CASES=(7878-) ;;
  swap-keeps-snapshot) CASES=(7880-) ;;
  *) echo "mutant-catch: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-catch: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  no-release)          mut compiler/emit/lower/call.fib 's/\(lcx-run-ops cx ops\)\)\)\)/(Ok ()))))/' ;;
  no-pending)          mut compiler/emit/lower/call.fib 's/\(doseq \[v \(reverse \@\(\. cx pending\)\)\] \(lcx-release cx v\)\)/()/' ;;
  double-release)      mut compiler/emit/lower/call.fib 's/\(lcx-run-ops cx ops\)\)\)\)/(do (lcx-run-ops cx ops) (lcx-run-ops cx ops)))))/' ;;
  finally-catchable)   mut rt/core.lir 's/\(br \(call \@fib\.in-finally\) fin task\)/(br (i1 0) fin task)/; s/      \(store \(i64 0\) dp\)\n      \(store \(add \(load i64 fp\)/      (store (add (load i64 fp)/' ;;
  no-catch)            mut rt/core.lir 's/\(br \(icmp eq ex \(ptr null\)\) ok caught\)/(br (i1 1) ok caught)/' ;;
  catch-everywhere)    mut rt/core.lir 's/\(block no \(ret \(i1 0\)\)\)/(block no (ret (i1 1)))/; s/\(br \(call \@fib\.catching\) unwind plain\)/(br (i1 1) unwind plain)/g' ;;
  take-unwinds)        mut compiler/emit/lower/mod.fib 's/\(takes-slots\? p \(body-expr g \(\. inst key\)\)\)/false/; s/\(takes-slots\? p \(\. f body\)\)/false/' ;;
  update-unwinds)      mut compiler/emit/lower/cells.fib 's/\(fatal-while cx \(fn \(\) \(\(\. \(\. cx hub\) emit-call\) cx \(TgValue f\) \[old\] content false\)\)\)/((. (. cx hub) emit-call) cx (TgValue f) [old] content false)/' ;;
  no-write-back)       mut compiler/emit/lower/call.fib 's/\(\(some w\) \(call-write-backs cx \(\. w fst\) \(\. \(\. w snd\) fst\) \(\. \(\. w snd\) snd\)\)\)/((some w) (Ok ()))/' ;;
  swap-keeps-snapshot) mut compiler/emit/lower/cells.fib 's/\(set! \(\. cx pending\) \(conj \@\(\. cx pending\) old\)\)/(set! (. cx pending) \@(. cx pending))/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-catch[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-catch: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
for p in "${CASES[@]}"; do
  res=$("$OUT/F" cases cases/stdlib --only "$p" -j "${MUT_J:-1}" 2>&1)
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-220)
  if echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1; else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-catch[$MODE]: every case failed under the mutant"; else echo "mutant-catch[$MODE]: a case survived: rewrite it"; fi
exit $survived
