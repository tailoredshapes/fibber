#!/bin/bash
# scripts/mutant-sched.sh: planted faults in the work-stealing pool (rt/sched.lir, rt/park.lir, rt/deque.lir, the pool runner of
# compiler/emit/lower/threads.fib; docs/design/parallelism.md 3.1, the acceptance rows of P-sched). The runtime is embedded in the
# compiler, so each mutant copies the tree to a scratch directory, breaks ONE thing in the copy, regenerates compiler/emit/runtime.fib,
# builds a stage 2 from the copy (about 90 s) and runs the cases and stress programs that pin the rule, under a timeout. Every one must
# FAIL under the mutant (a wrong answer, a trap, a crash, a hang killed by the timeout all count). A case that still passes survived.
# usage: scripts/mutant-sched.sh MODE|all|check
#   check          no mutant: the pool's own checks that are not cases: `ring-overflow` (with one worker a fork of 20 000 tasks overflows the
#                  ring of 4096 and the rest run inline: pool-inline-forks > 0), and the pool cases under FIB_THREADS=1, 2 and 3 (the results
#                  do not depend on the worker count; 7606's "more than one chunk at once" is skipped with one worker, which has none)
# environment: FIBC (the fibc that builds the mutated stage 2; required), MUT_OUT (scratch, default ~/.cache/fibber-scratch/mutant-sched-MODE),
#   LLVM_LIBDIR (default /usr/lib/llvm-21/lib). Builds and runs under `ulimit -v 16000000`.
# exit: 0 when at least one detector (a case or a stress run) failed under the mutant (or, for `check`, every check held), 1 otherwise, 2 for a setup error.
#
# The modes and what they plant (the design's rows in docs/design/parallelism.md 5, P-sched):
#   steal-no-cas       M1 of cl.lir: steal takes the item without the compare-and-swap on top: two thieves take one task (a task run twice:
#                      its count released twice, a wrong sum or a crash)
#   pop-last-no-race   M3: the owner's pop of the last item does not race the thieves for it: the owner and a thief both take it
#   pop-fence-release  M2: pop's seq_cst fence weakened to release. NOT detected on x86 in the design's experiment (the stores are ordered
#                      there); it is run and reported, and a survival is EXPECTED, not a failure (the mode exits 0 either way and says so)
#   lost-wakeup        the eventcount's re-check after the announcement is removed: a fork that saw no sleeper is never seen by the worker
#                      that then sleeps; with two workers a storm of forks from main hangs (the timeout kills it)
#   complete-no-fence  the seq_cst fence between a completion's store of the state and its load of the waiters is removed: a joiner that
#                      registered between the two is never woken (a storm from main hangs)
#   park-not-help      a join that neither runs the task itself nor helps with other work (the design's "deadlock" mutant): it parks; with every
#                      worker inside a join and the children in the deques, nothing runs them and the fork-join tree deadlocks. (Parking instead of
#                      helping alone does NOT deadlock: a join that runs its own unstarted child is deadlock-free by itself; measured, 2026-10-07.)
#   run-unclaimed      a thief runs a task without claiming its driver: the joiner and the thief both run it
#   complete-first     the pool runner (emit.lower.threads pool-entry) completes the task before it stores the result: a joiner reads zero
set -uo pipefail
MODE=${1:?usage: mutant-sched.sh MODE|all|check}
R=$(cd "$(dirname "$0")/.." && pwd)
if [ "$MODE" = all ]; then
  rc=0
  for m in steal-no-cas pop-last-no-race pop-fence-release lost-wakeup complete-no-fence park-not-help run-unclaimed complete-first check; do
    "$0" "$m" || rc=1
  done
  exit $rc
fi
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-sched-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-sched: set FIBC to an executable fibc" >&2; exit 2; }
LIBDIR=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
unset LD_LIBRARY_PATH
ulimit -v 16000000; export MALLOC_ARENA_MAX=2
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/lib" "$R/rt" "$R/compiler" "$R/scripts" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
T=$OUT/tree
mut() {  # mut FILE PERL-EXPRESSION: apply, and fail when nothing changed (the source moved)
  local f=$T/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-sched: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
}
expect_survive=0
case $MODE in
  steal-no-cas)      mut rt/deque.lir 's/\(r \(cmpxchg seq_cst monotonic w t \(add t \(i64 1\)\)\)\)\)\n      \(ret \(select \(extractvalue r 1\) x \(ptr null\)\)\)\)\)\)\n/)\n      (atomic-store monotonic (add t (i64 1)) w)\n      (ret x))))\n/' ;;
  pop-last-no-race)  mut rt/deque.lir 's/\(block last\n    \(let \(\(r \(cmpxchg seq_cst monotonic w t \(add t \(i64 1\)\)\)\)\)\n      \(atomic-store monotonic \(add b \(i64 1\)\) bp\)\n      \(ret \(select \(extractvalue r 1\) x \(ptr null\)\)\)\)\)\)/(block last\n    (atomic-store monotonic (add b (i64 1)) bp)\n    (ret x)))/' ;;
  pop-fence-release) mut rt/deque.lir 's/\(atomic-store monotonic b bp\)\n      \(fence seq_cst\)/(atomic-store monotonic b bp)\n      (fence release)/'; expect_survive=1 ;;
  lost-wakeup)       mut rt/sched.lir 's/\(br \(call \@fib\.sched-any-work\) withdraw sleep\)\)\)\n  \(block withdraw \(atomicrmw sub seq_cst \@fib\.sched-sleepers \(i32 1\)\) \(br out\)\)/(br sleep)))\n  (block withdraw (atomicrmw sub seq_cst \@fib.sched-sleepers (i32 1)) (br out))/' ;;
  complete-no-fence) mut rt/sched.lir 's/\(atomic-store release \(i32 2\) \(getelementptr %struct\.fib\.task task \(i32 0\) \(i32 3\)\)\)\n      \(fence seq_cst\)/(atomic-store release (i32 2) (getelementptr %struct.fib.task task (i32 0) (i32 3)))/' ;;
  park-not-help)     mut rt/park.lir 's/\(br \(icmp sge \(load i64 \(getelementptr i8 w \(i64 200\)\)\) \(load i64 \@fib\.sched-max-depth\)\) deep find\)/(br (icmp sge (i64 0) (i64 0)) deep find)/'; mut rt/park.lir 's/\(br \(call \@fib\.sched-claim-run task\) loop help\)/(br (icmp eq (i64 0) (i64 1)) loop help)/' ;;
  run-unclaimed)     mut rt/sched.lir 's/\(r \(cmpxchg acq_rel acquire dp \(i32 0\) \(i32 1\)\)\)\)\n      \(br \(extractvalue r 1\) claimed out\)\)\)\n  \(block claimed\n    \(br \(icmp ult/(r (cmpxchg acq_rel acquire dp (i32 0) (i32 1))))\n      (br claimed)))\n  (block claimed\n    (br (icmp ult/' ;;
  complete-first)    mut compiler/emit/lower/threads.fib 's/:else \(str-join \[head "      \(let \(" \(pool-call uw r\) "\)\\n" store finish "\)\)\)\)\\n"\]\)/:else (str-join [head "      (let (" (pool-call uw r) ")\\n      (call \@fib.sched-complete task prev)\\n" store "      (ret)))))\\n"])/' ;;
  check) ;;
  *) echo "mutant-sched: unknown MODE $MODE" >&2; exit 2 ;;
esac
# the stage 2 of the (mutated) copy
build() {
  (cd "$T" && "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" > "$OUT/gen.log" 2>&1) || { tail -5 "$OUT/gen.log"; return 2; }
  (cd "$T" && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib) || return 2
  (cd "$T" && nice "$FIBC" build compiler/fibc.fib -I compiler -I lib -L "$LIBDIR" -l LLVM-21 -o "$OUT/F" > "$OUT/build.log" 2>&1) || { tail -5 "$OUT/build.log"; return 2; }
}
if [ "$MODE" != check ] || [ ! -x "$OUT/F" ]; then build || { echo "mutant-sched: the stage 2 of the copy did not build" >&2; exit 2; }; fi
F=$OUT/F
export FIB_LIB=$T/lib
cd "$T" || exit 2
"$F" build scripts/tsan/sched.fib -o "$OUT/sched-k" > "$OUT/k.log" 2>&1 || { tail -3 "$OUT/k.log"; exit 2; }
rc=0
killed=0; survived=0
# cases CASE-PREFIX.. : each must fail (or time out) under the mutant
cases() {
  for p in "$@"; do
    out=$(timeout 120 "$F" cases cases/stdlib --only "$p" 2>&1); code=$?
    line=$(echo "$out" | grep -E '^[0-9]+ cases:')
    if [ $code -eq 124 ]; then killed=$((killed+1)); echo "mutant $MODE: case $p hung and was killed, as it must"
    elif echo "$line" | grep -q ' 0 fail' && echo "$line" | grep -q ' 0 header'; then survived=$((survived+1)); echo "mutant $MODE: case $p SURVIVED: $line"
    else killed=$((killed+1)); echo "mutant $MODE: case $p failed as it must: $(echo "$out" | grep -E 'FAIL|HEADER' | cut -c1-160 | head -1)"; fi
  done
}
# stress KIND N TIMEOUT [ENV..]: the kernel must not answer 0 (a wrong answer, a crash or the timeout)
stress() {
  local kind=$1 n=$2 tmo=$3; shift 3
  env "$@" timeout "$tmo" "$OUT/sched-k" "$kind" "$n" > /dev/null 2>&1; local code=$?
  if [ $code -eq 0 ]; then survived=$((survived+1)); echo "mutant $MODE: stress $kind $n ($*) SURVIVED (answered right)"
  elif [ $code -eq 124 ]; then killed=$((killed+1)); echo "mutant $MODE: stress $kind $n ($*) hung and was killed, as it must"
  else killed=$((killed+1)); echo "mutant $MODE: stress $kind $n ($*) failed as it must (exit $code)"; fi
}
case $MODE in
  steal-no-cas|pop-last-no-race|run-unclaimed) cases 8640- 8645-; stress tree 18 120; stress wide 20000 120 ;;
  pop-fence-release) cases 8640- 8645-; stress tree 18 120 ;;
  lost-wakeup)       stress storm 200000 60 FIB_THREADS=2; stress storm 200000 60 FIB_THREADS=1 ;;
  complete-no-fence) stress storm 300000 60 FIB_THREADS=2; stress storm 300000 60 FIB_THREADS=4 ;;
  park-not-help)     cases 8640-; stress tree 16 60 FIB_THREADS=4 ;;
  complete-first)    cases 8647- 8640- ;;
  check)
    inline=$(FIB_THREADS=1 "$F" run scripts/tsan/sched.fib wide 20000 2>&1 | tail -1)
    # the kernel answers 0; the inline count is read by a one-line program
    cat > "$OUT/inline.fib" <<'EOF'
(ns main (:use fib.core fib.parallel))
(defun wide (n: i64) -> i64
  (let [ts (loop [i 0 out []] (if (< i n) (recur (+ i 1) (conj out (fork-task (fn () (* i 2))))) out))]
    (reduce (fn (acc: i64 t: (Task i64)) (+ acc (join t))) 0 ts)))
(defun main () -> i64 (do (join (fork-task (fn () (wide 20000)))) (if (> (pool-inline-forks) 0) 0 1)))
EOF
    if FIB_THREADS=1 "$F" run "$OUT/inline.fib" > /dev/null 2>&1; then echo "check ring-overflow: with one worker a wide fork of 20 000 ran some inline (ok)"
    else echo "check ring-overflow: FAILED (no inline fork with one worker)"; rc=1; fi
    for n in 1 2 3; do
      out=$(FIB_THREADS=$n timeout 600 "$F" cases cases/stdlib --only 864 760 761 2>&1)
      bad=$(echo "$out" | grep -E 'FAIL|HEADER' | grep -v '^7606-' | head -3)
      if [ -n "$bad" ]; then echo "check FIB_THREADS=$n: FAILED: $bad"; rc=1; else echo "check FIB_THREADS=$n: $(echo "$out" | grep -E '^[0-9]+ cases:')"; fi
    done ;;
esac
if [ "$MODE" != check ]; then
  if [ $killed -gt 0 ]; then echo "mutant $MODE: KILLED ($killed detectors failed, $survived did not)"; else echo "mutant $MODE: NOT KILLED by any detector"; rc=1; fi
fi
if [ "$expect_survive" = 1 ] && [ $rc -ne 0 ]; then echo "mutant $MODE: survived, as the design's experiment found on x86 (a weakened fence needs a model checker)"; rc=0; fi
exit $rc
