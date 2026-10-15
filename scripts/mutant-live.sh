#!/bin/bash
# scripts/mutant-live.sh: planted faults in live mode (docs/design/live-repl.md section 6: the mutants of each package). Copies compiler/ and lib/ to a
# scratch directory, breaks ONE rule in the copy, builds a fibc from the copy and runs the test that pins the rule against it: the test must FAIL. A test
# that still passes under its mutant is a bad test. The pattern of scripts/mutant-serve.sh.
# usage: scripts/mutant-live.sh MODE
#   MODE                 what is broken                                                              test that must fail
#   stub-no-tailcall     the stub calls the body and returns instead of tail-jumping: every call of  compiler/tests/live/stubs.sh (deep.fib's
#                        a program function adds a frame, and a recursion a hundred million deep     recursion traps `stack overflow`; hello's
#                        traps `stack overflow` (LV0). A million deep (ownership cases 53, 164) is   golden names the tail jump)
#                        NOT enough: main's stack (rt/thread.lir) holds a million stub frames
#   stub-plain-load      the stub reads the slot with a plain load, not an acquire load: a caller     compiler/tests/live/stubs.sh (hello's
#                        may jump into code the swap has not finished writing (LV0). Until LV2's    golden names the acquire load; LV2's
#                        swap stress exists this is killed by the text of hello's golden only, not  TSAN run is the kill that matters)
#                        by a run
#   deps-stubbed-by-default  in a project the name rule is kept: the libraries' bodies are stubbed   compiler/tests/live/project.sh (the
#                        too (LT3, docs/design/live-meshql.md 4)                                      golden's `default:` line gains acme.*)
#   project-paths-not-stubbed  in a project no module of the project's own is live (only the main    compiler/tests/live/project.sh (the
#                        module): the project's own bodies are direct calls (LT3)                     golden's lines lose app.core)
#   sink-ignored         fib.test.run's `report` prints the document instead of writing `out`, the   cases/stdlib 8423 (the buffer is empty:
#                        sink `run-main-into` was given (LT1, docs/design/live-meshql.md 3.2)         result 2); compiler/tests/live/hooks.sh
#   override-ignored     `run-main-into` parses `(args)` and not the words it was given (LT1)         cases/stdlib 8423 (`--only x` not applied:
#                                                                                                     three plain rows in the buffer: result 2)
#   value-reevaluated    a session `def` is not made resident: its slot is left in the input's      compiler/tests/live/attach.sh (the atom
#                        module, so every later input gets a slot of its own, never initialised,    counter of the design's transcript does not
#                        and the value of a `def` does not persist from one input to the next (LV1) answer 1 then 2: the second read is not of
#                                                                                                   the same atom)
#   trap-not-caught      an expression's entry runs without `live-guard` (the `try` of fib.ex):     compiler/tests/live/attach.sh (`(nth [1 2] 5)`
#                        a trap in an input aborts the program instead of answering `failed` (LV1)  ends the program; the next input has nobody
#                                                                                                   to answer it)
#   patch-before-jit     a swap stores the slots (with 0: the address is not known yet) BEFORE the   compiler/tests/live/swap.sh (the worker thread
#                        new bodies are looked up, so a call during ORC's compilation of the new     calling `work` during one of the 200 swaps
#                        generation jumps to address 0 (LV2; docs/design/live-repl.md 3.1 step 4)   jumps to 0: the app dies, the next check
#                                                                                                   has nobody to answer it)
#   old-tracker-removed  a swap removes the previous generation's tracker right after the patch:    compiler/tests/live/swap.sh (the worker is
#                        a thread in the old body runs freed code (LV2; 3.3: generations are kept)   inside the old `work` when its code is freed:
#                                                                                                   the app dies)
#   summary-ignored      the summary rule is not applied: a body whose parameter was borrowed and   compiler/tests/live/swap.sh (transcript 5's
#                        is now owned (stored) is accepted; its callers hand nothing over (LV2, 3.4) second refusal does not come; `handle` then
#                                                                                                   stores a request its caller releases)
#   signature-ignored    the signature rule is not applied: a function redefined with another       compiler/tests/live/swap.sh (transcript 5's
#                        scheme (an added parameter) is accepted; its callers' calls are typed lIR  first refusal does not come; the server's
#                        of the old one (LV2, 3.4)                                                  calls of `handle` read a register for `db`)
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-live-MODE),
#              MUT_QUICK=1 runs stubs.sh --quick (hello's golden and the deep recursion; no case tables: kills both mutants in under a minute after the build).
# A mutant of lib/ alone (sink-ignored, override-ignored) builds nothing: the case's `roots:` header names the copy's lib, and FIBC runs it.
# exit: 0 when the test failed under the mutant, 1 when it passed (the mutant survived), 2 for a setup error. That the tests pass UNmutated is shown by running them.
set -uo pipefail
MODE=${1:?usage: mutant-live.sh stub-no-tailcall|stub-plain-load|deps-stubbed-by-default|project-paths-not-stubbed|sink-ignored|override-ignored|value-reevaluated|trap-not-caught|patch-before-jit|old-tracker-removed|summary-ignored|signature-ignored}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-live-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-live: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
rm -rf "$OUT"; mkdir -p "$OUT"
cp -r "$R/compiler" "$R/lib" "$OUT/"
patch() { # patch FILE OLD NEW: the text must be there exactly once
  python3 - "$OUT/$1" "$2" "$3" <<'EOF' || { echo "mutant-live: the text to change is not in $1 (the source moved: update the mutant)" >&2; exit 2; }
import sys
p, old, new = sys.argv[1:4]
s = open(p).read()
if s.count(old) != 1: sys.exit(1)
open(p, 'w').write(s.replace(old, new))
EOF
}
case $MODE in
  stub-no-tailcall) patch compiler/emit/live.fib '  (if (tail-jump? ty)
      [(n/indirect-call (n/local "at") ty args true)]' '  (if false
      [(n/indirect-call (n/local "at") ty args true)]' ;;
  stub-plain-load)  patch compiler/emit/live.fib '(n/node (la/KAtomicLoad la/ScSystem la/OAcquire lt/ty-ptr (n/global slot)))' '(n/load lt/ty-ptr (n/global slot) nil)' ;;
  deps-stubbed-by-default) patch compiler/emit/live.fib '((some names) (or (= m @(. g main)) (contains? names (g-module-name g m))))' '((some names) (mods/sees-implicit (g-module-name g m) (mods/implicit-lib)))' ;;
  project-paths-not-stubbed) patch compiler/emit/live.fib '((some names) (or (= m @(. g main)) (contains? names (g-module-name g m))))' '((some names) (= m @(. g main)))' ;;
  sink-ignored) patch lib/fib/test/run.fib '(do (out (cond (= (. o format) "json")' '(do (print (cond (= (. o format) "json")' ;;
  override-ignored) patch lib/fib/test/run.fib '  (match (parse-args argv)' '  (match (parse-args (vec (args)))' ;;
  value-reevaluated) patch compiler/serve/livesess.fib '      (if (empty? slots)
          (Ok ())' '      (if true
          (Ok ())' ;;
  trap-not-caught)  patch compiler/serve/livesess.fib '-> i64 (live-guard live-fd (fn () -> i64 (do "' '-> i64 ((fn () -> i64 (do "' ;;
  patch-before-jit) patch compiler/serve/liveswap.fib '                (addrs (ti/try-map (fn (nm: str) (ls/address-of sx nm)) coded))' '                (_ (publish sx names (mapv (fn (nm: str) 0) names)))
                (addrs (ti/try-map (fn (nm: str) (ls/address-of sx nm)) coded))' ;;
  old-tracker-removed) patch compiler/serve/liveswap.fib '                (_ (publish sx names addrs)))' '                (_ (publish sx names addrs))
                (_ (let ((gens (. (unwrap-or (get (. @(. sx table) families) family) (lt/Family 0 nil nil [])) gens)))
                     (if (empty? gens) (Ok ()) (lj/remove (ls/jit-of sx) (. (nth gens (- (count gens) 1)) rt) (. (nth gens (- (count gens) 1)) names))))))'
                       patch compiler/serve/liveswap.fib '[own.driver :as od] [types.init :as ti]' '[own.driver :as od] [types.init :as ti] [live.jit :as lj]' ;;
  summary-ignored)  patch compiler/serve/liverules.fib '       (nil (summary-rule c f fid))))' '       (nil nil)))' ;;
  signature-ignored) patch compiler/serve/liverules.fib '     (match (signature-rule c f fid session-defs)' '     (match (if true nil (signature-rule c f fid session-defs))' ;;
  *) echo "mutant-live: unknown mode $MODE" >&2; exit 2 ;;
esac
case $MODE in
  sink-ignored|override-ignored) F=$FIBC ;;  # a mutant of lib/ alone: nothing to build
  *)
    cd "$OUT" || exit 2
    echo "mutant-live: building the $MODE mutant ..."
    ( ulimit -v 16000000; "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F" ) > "$OUT/build.log" 2>&1 \
      || { echo "mutant-live: the mutant does not build (see $OUT/build.log)"; tail -5 "$OUT/build.log"; exit 2; }
    F=$OUT/F ;;
esac
cd "$R" || exit 2
quick=(); [ "${MUT_QUICK:-}" = 1 ] && quick=(--quick)
case $MODE in
  stub-*) what="the stubs test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/stubs.sh" "$F" "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$? ;;
  value-reevaluated|trap-not-caught) what="the attach test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/attach.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  patch-before-jit|old-tracker-removed|summary-ignored|signature-ignored) what="the swap test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/swap.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  deps-stubbed-by-default|project-paths-not-stubbed) what="the project test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/project.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  *) what="case 8423"  # the case's `roots:` header names ../../lib: run from a copy of cases/stdlib beside the mutated lib
     mkdir -p "$OUT/cases/stdlib"; cp -r "$R"/cases/stdlib/8423-*.fib "$R/cases/stdlib/support" "$OUT/cases/stdlib/"
     ( cd "$OUT" && ulimit -v 16000000 && FIB_LIB="$OUT/lib" "$F" cases cases/stdlib --only 8423- ) > "$OUT/test.log" 2>&1; rc=$? ;;
esac
grep -E '^FAIL|FAIL ' "$OUT/test.log" | head -5 || true  # pipe-ok: a short failure listing
if [ $rc -ne 0 ]; then echo "mutant-live: $MODE KILLED by $what (exit $rc)"; exit 0; fi
echo "mutant-live: $MODE SURVIVED: $what passed under the mutant"; exit 1
