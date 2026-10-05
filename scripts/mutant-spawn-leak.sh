#!/bin/bash
# scripts/mutant-spawn-leak.sh: planted faults for the thread lifecycle of rt/thread.lir (docs/design/exceptions.md F1: a finished thread is
# detached, main's return waits on a count of live threads). Copies the tree to a scratch directory, breaks ONE rule of the runtime in the copy,
# regenerates compiler/emit/runtime.fib from it, builds a stage 2 from the copy and runs the cases that pin the rule: every one must FAIL under
# the mutant (a wrong answer, a trap, an audit leak all count). A case that still passes survived the mutant and is a bad case.
# usage: scripts/mutant-spawn-leak.sh MODE
#   MODE      what is broken                                                         cases that must fail
#   detach    a thread is not detached when it starts: its resources stay            912- 913-
#   join-all  main's return does not wait for the threads still running              914-
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-spawn-leak-MODE), MUT_J (cases at once, default 1). The cases run under `ulimit -v 16000000`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by an
#   ordinary `F cases cases/stdlib --only 912- 913- 914-` of the tree's own stage 2; this script does not repeat it.
set -uo pipefail
MODE=${1:?usage: mutant-spawn-leak.sh detach|join-all}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-spawn-leak-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-spawn-leak: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  detach)   CASES=(912- 913-) ;;
  join-all) CASES=(914-) ;;
  *) echo "mutant-spawn-leak: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-spawn-leak: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  detach)   mut rt/thread.lir 's/    \(call \@pthread_detach \(load i64 handle\)\)\n//' ;;
  join-all) mut rt/thread.lir 's/\(icmp sgt \(load i32 \@fib\.threads-live\) \(i32 0\)\)/(icmp sgt (load i32 \@fib.threads-live) (i32 99999999))/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-spawn-leak[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-spawn-leak: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
for p in "${CASES[@]}"; do
  res=$("$OUT/F" cases cases/stdlib --only "$p" -j "${MUT_J:-1}" 2>&1)
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-200)
  if echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1; else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-spawn-leak[$MODE]: every case failed under the mutant"; else echo "mutant-spawn-leak[$MODE]: a case survived: rewrite it"; fi
exit $survived
