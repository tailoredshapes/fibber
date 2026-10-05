#!/bin/bash
# scripts/mutant-task-trap.sh: planted faults for stage 1 of exceptions (docs/design/exceptions.md 4.1): a trap on a spawned task's thread
# ends the task (rt/core.lir fib.fail-task), `try-join` returns it, `join` and `@t` trap in the joiner with its message, what the task
# owned is abandoned and counted by `audit: abandoned` / `leaks: N`. Copies the tree to a scratch directory, breaks ONE rule in the copy,
# regenerates compiler/emit/runtime.fib from rt/, builds a stage 2 from the copy and runs the cases that pin the rule: every one must FAIL
# under the mutant (a wrong answer, an aborted run, an audit leak or error all count). A case that still passes survived and is a bad case.
# usage: scripts/mutant-task-trap.sh MODE
#   MODE            what is broken                                                              cases that must fail
#   no-isolate      a trap on a task's thread aborts the process as before                      911- 915- 918- 919- 920- 923-
#   join-ignores    join and @t of a failed task read its empty result and go on                916- 917- 923-
#   ok-failed       try-join of a failed task says Ok (task-failure answers nil)                911- 915- 918- 919- 920- 923-
#   no-release      the thread keeps its count of a failed task (the task leaks)                911- 918- 923-
#   double-release  the thread releases its count of a failed task twice                         911- 918- 923-
#   leak-message    the failure message is not released with the task                            911- 915- 919-
#   main-isolated   the thread-entry test is dropped: a trap on any thread is isolated           916- 921-
#   oom-isolated    out of memory is an ordinary trap (isolated in a task)                        922-
#   ex-cause        ex-cause always answers nil                                                  924-
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-task-trap-MODE), MUT_J (cases at once, default 1). The cases run under `ulimit -v 16000000`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by an
#   ordinary `F cases cases/stdlib --only 911- ..` of the tree's own stage 2; this script does not repeat it.
set -uo pipefail
MODE=${1:?usage: mutant-task-trap.sh no-isolate|join-ignores|ok-failed|no-release|double-release|leak-message|main-isolated|oom-isolated|ex-cause}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-task-trap-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-task-trap: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  no-isolate)     CASES=(911- 915- 918- 919- 920- 923-) ;;
  join-ignores)   CASES=(916- 917- 923-) ;;
  ok-failed)      CASES=(911- 915- 918- 919- 920- 923-) ;;
  no-release)     CASES=(911- 918- 923-) ;;
  double-release) CASES=(911- 918- 923-) ;;
  leak-message)   CASES=(911- 915- 919-) ;;
  main-isolated)  CASES=(916- 921-) ;;
  oom-isolated)   CASES=(922-) ;;
  ex-cause)       CASES=(924-) ;;
  *) echo "mutant-task-trap: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-task-trap: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  no-isolate)     mut rt/core.lir 's/\(br \(icmp ne tk \(ptr null\)\) in-task plain\)/(br (icmp ne tk (ptr null)) plain plain)/' ;;
  join-ignores)   mut rt/task.lir 's/\(br \(icmp ne msg \(ptr null\)\) failed done\)/(br (icmp ne msg (ptr null)) done done)/' ;;
  ok-failed)      mut rt/task.lir 's/\(ret \(atomic-load acquire ptr \(getelementptr %struct\.fib\.task task \(i32 0\) \(i32 10\)\)\)\)/(ret (ptr null))/' ;;
  no-release)     mut rt/core.lir 's/      \(call \@fib\.release task\)\n      \(call \@fib\.thread-done\)/      (call \@fib.thread-done)/' ;;
  double-release) mut rt/core.lir 's/      \(call \@fib\.release task\)\n      \(call \@fib\.thread-done\)/      (call \@fib.release task)\n      (call \@fib.release task)\n      (call \@fib.thread-done)/' ;;
  leak-message)   mut compiler/emit/objects/walk.fib 's/\(conj \(conj \(conj \(counted \(some-of caps\) slot-task-capture0\) slot-task-result\)\s*slot-task-waiters\)\s*slot-task-failure\)/(conj (conj (counted (some-of caps) slot-task-capture0) slot-task-result) slot-task-waiters)/' ;;
  main-isolated)  mut rt/core.lir 's/\(br \(icmp ne tk \(ptr null\)\) in-task plain\)/(br (i1 1) in-task plain)/' ;;
  oom-isolated)   mut rt/core.lir 's/fib\.fatal-c \(string "out of memory"\)/fib.trap-c (string "out of memory")/g' ;;
  ex-cause)       mut lib/fib/ex.fib 's/\(\. e cause\)\)\s*$/nil)/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-task-trap[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-task-trap: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
for p in "${CASES[@]}"; do
  res=$("$OUT/F" cases cases/stdlib --only "$p" -j "${MUT_J:-1}" 2>&1)
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-220)
  if echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1; else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-task-trap[$MODE]: every case failed under the mutant"; else echo "mutant-task-trap[$MODE]: a case survived: rewrite it"; fi
exit $survived
