#!/bin/bash
# compiler/tests/live/testlive.sh FIBC: `fibc test --live` and `fibc test --connect` (driver.testlive; package LT6 of docs/design/live-meshql.md
# section 6). The specs of compiler/tests/driver/test-cmd.sh (a holding, a failing, a trapping and a skipping spec, one at any depth, one that
# needs a module under a second -I) are made resident in ONE live session, and the rows of test-cmd.sh are reproduced through it: each
# report (plain, json, tap, --list, --only, -j) is compared byte for byte with `fibc test`'s, and the exit statuses with test-cmd.sh's.
# Then the reason the command exists: the second run of one scenario through the session is timed against `fibc test` of the same
# scenario (the bound is generous, for a loaded machine: under 5 s and under half the spawned run; the design's numbers are 20 s and
# under 1 s for meshql-fib, 1.3), and the session's `status` must show that the run was its own (`tests.last`: 1 scenario). Refusals:
# --timeout and --isolate fork (exit 2), a spec the session did not load (ERROR, exit 2), no session (exit 2), --live with an option.
# A spec that does not compile and a second spec of one base name are given to --live: the session serves the others, and a run of either
# is that file's ERROR (`does not compile` with its diagnostic, `cannot be run` with the reason), as `fibc test` reports a spec that fails.
# The mutants `child-spawned-anyway` (--connect runs `fibc test` as before) and `unheld-held` of scripts/mutant-live.sh must make it fail.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
s2=${1:-${FIBC:-}}
[ -x "$s2" ] || { echo "usage: testlive.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_WITH
ulimit -v 16000000
mkdir -p "$HOME/.cache/fibber-scratch"
t=$(mktemp -d "$HOME/.cache/fibber-scratch/testlive.XXXXXX")
pid=
cleanup() { [ -n "$pid" ] && kill "$pid" 2> /dev/null; rm -rf "$t"; }
trap cleanup EXIT
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
ms() { echo $(( $(date +%s%N) / 1000000 )); }
mkdir -p "$t/specs/sub" "$t/skips" "$t/extra/helper"
spec() { printf '(ns main (:use fib.test.core fib.test.run)%s)\n(defspecs specs\n  (feature %s\n%s))\n(defun main () -> i64 (run-main (specs)))\n' "$3" "$2" "$4" > "$t/$1"; }
spec specs/good-spec.fib '"Good"' '' '    (scenario "one holds" (given [a 1]) (then (expect = 1 a)))
    (scenario "two holds" (given [a 2]) (then (expect = 2 a)))'
spec specs/sub/bad-spec.fib '"Bad"' '' '    (scenario "a wrong claim" (given [a 1]) (then (expect = 2 a)))
    (scenario "a right one" (given [a 1]) (then (expect = 1 a)))'
spec trap-spec.fib '"Trap"' '' '    (scenario "traps" (given [v [1 2]]) (then (expect = 9 (nth v 7))))
    (scenario "after" (then (expect = 1 1)))'
spec skips/s-spec.fib '"Skips"' '' '    (scenario "needs a thing that is not here" (skip-unless (= 1 2) "no such thing") (given [v [1 2]]) (then (expect = 9 (nth v 7))))
    (scenario "holds" (skip-unless (= 1 1) "never") (given [a 1]) (then (expect = 1 a)))'
spec uses-spec.fib '"Roots"' ' (:require [helper.greet :as g])' '    (scenario "a module from the second -I" (then (expect = "hello" (g/greeting))))'
spec other-spec.fib '"Other"' '' '    (scenario "never loaded" (then (expect = 1 1)))'
spec broken-spec.fib '"Broken"' '' '    (scenario "does not compile" (given [a 1]) (then (expect = 1 (+ a "x"))))'
mkdir -p "$t/dup"; spec dup/good-spec.fib '"Dup"' '' '    (scenario "same base name" (then (expect = 1 1)))'
printf '(ns helper.greet)\n(defun greeting () -> str "hello")\n' > "$t/extra/helper/greet.fib"
cd "$t" || exit 2

$s2 test --live --only x specs > /dev/null 2> "$t/e0"; ck "--live with an option: exit 2" $? 2
$s2 test --connect 999999999 specs/good-spec.fib > /dev/null 2> "$t/e0"; ck "--connect with no session: exit 2" $? 2

t0=$(ms)
$s2 test -I "$t/extra" --live specs trap-spec.fib skips uses-spec.fib broken-spec.fib dup > "$t/live.out" 2> "$t/live.err" &
pid=$!
for _ in $(seq 1200); do grep -q 'live at' "$t/live.err" && break; kill -0 "$pid" 2> /dev/null || break; sleep 0.25; done
ck "--live prints the socket" "$(grep -c "fibc: live at .*/$pid.sock" "$t/live.err")" 1
ck "--live names the command" "$(grep -c "fibc test --connect $pid" "$t/live.err")" 1
echo "      the session is up after $(( $(ms) - t0 )) ms"

# the rows of test-cmd.sh: `fibc test ARGS` and `fibc test --connect PID ARGS`, the same report and the same status
same() { # same NAME STATUS ARGS..
  local name=$1 want=$2; shift 2
  $s2 test "$@" > "$t/n.out" 2> /dev/null; local rn=$?
  $s2 test --connect "$pid" "$@" > "$t/c.out" 2> "$t/c.err"; local rc=$?
  ck "$name: exit $want (spawned $rn, live $rc)" "$rn $rc" "$want $want"
  if cmp -s "$t/n.out" "$t/c.out"; then echo "ok    $name: the same report"; else echo "FAIL  $name: the reports differ"; diff "$t/n.out" "$t/c.out" | head -10 || true; fail=1; fi  # pipe-ok: a short diff listing
}
same "a holding spec" 0 specs/good-spec.fib
ck "a holding spec: the total line" "$(grep '^total:' "$t/c.out")" "total: 2 scenarios: 2 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)"
same "a failing spec" 1 specs/sub/bad-spec.fib
ck "a failing spec: the location of the expect is printed" "$(grep -c 'at .*bad-spec.fib:4:' "$t/c.out")" 1
same "a trapping scenario" 1 trap-spec.fib
ck "a trapping scenario: one trap, the one after it ran" "$(grep '^total:' "$t/c.out")" "total: 2 scenarios: 1 pass, 0 fail, 1 trap, 0 timeout in 1 spec files (seed 1)"
same "a directory" 1 specs
ck "a directory: the total over both files" "$(grep '^total:' "$t/c.out")" "total: 4 scenarios: 3 pass, 1 fail, 0 trap, 0 timeout in 2 spec files (seed 1)"
same "-j 2 over two files" 1 specs -j 2
same "--only across files" 0 specs --only one-holds
ck "--only across files: one scenario" "$(grep '^total:' "$t/c.out")" "total: 1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)"
same "--only matching nothing anywhere" 2 specs --only nothing-matches-this
ck "--only matching nothing: said" "$(grep -c 'nothing to run is not a pass' "$t/c.err")" 1
same "json" 1 specs --format json
ck "json: summary" "$(grep -o '"summary":{[^}]*}' "$t/c.out")" '"summary":{"total":4,"pass":3,"fail":1,"trap":0,"timeout":0}'
same "tap" 1 specs --format tap
ck "tap: one plan for both files, numbered in order" "$(grep -E '^(1\.\.|ok |not ok )' "$t/c.out" | sed -E 's/ - .*//' | tr '\n' '|')" "1..4|ok 1|ok 2|not ok 3|ok 4|"
same "--list" 0 specs --list
same "skip" 0 skips
same "skip: json" 0 skips --format json
same "skip: tap" 0 skips --format tap
same "--seed 7" 1 specs --seed 7
$s2 test -I "$t/extra" --connect "$pid" uses-spec.fib > "$t/c.out" 2>&1; ck "a module under a second -I of --live: exit 0" $? 0

# a spec that does not compile and one of a base name another has: the session serves the others; each is its file's ERROR
ck "the session names the spec that does not compile" "$(grep -c 'broken-spec.fib is not held' "$t/live.err")" 1
$s2 test broken-spec.fib > "$t/n.out" 2> /dev/null; rn=$?
$s2 test --connect "$pid" broken-spec.fib > "$t/c.out" 2> /dev/null; rc=$?
ck "a spec that does not compile: exit 2 (spawned $rn, live $rc)" "$rn $rc" "2 2"
ck "a spec that does not compile: the same ERROR line" "$(grep 'ERROR' "$t/c.out")" "$(grep 'ERROR' "$t/n.out")"
ck "a spec that does not compile: the same diagnostic" "$(grep -c 'broken-spec.fib:4:' "$t/c.out") $(grep 'broken-spec.fib:4:' "$t/c.out")" "1 $(grep 'broken-spec.fib:4:' "$t/n.out")"
$s2 test --connect "$pid" specs/good-spec.fib broken-spec.fib > "$t/c.out" 2> /dev/null; rc=$?
ck "with a good spec: exit 2, the good one ran" "$rc $(grep -c 'ERROR: does not compile' "$t/c.out") $(grep '^total:' "$t/c.out")" "2 1 total: 2 scenarios: 2 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)"
$s2 test --connect "$pid" dup/good-spec.fib > "$t/c.out" 2> /dev/null; rc=$?
ck "a second spec of one base name: exit 2, the reason" "$rc $(grep -c "ERROR: cannot be run" "$t/c.out") $(grep -c 'its module spec.good-spec is specs/good-spec.fib.s too' "$t/c.out")" "2 1 1"

# refusals
$s2 test --connect "$pid" trap-spec.fib --timeout 1 > /dev/null 2>&1; ck "--timeout with --connect: exit 2" $? 2
$s2 test --connect "$pid" trap-spec.fib --isolate fork > /dev/null 2>&1; ck "--isolate fork with --connect: exit 2" $? 2
$s2 test --connect "$pid" other-spec.fib > "$t/c.out" 2> /dev/null; ck "a spec the session did not load: exit 2" $? 2
ck "a spec the session did not load: ERROR" "$(grep -c 'ERROR: cannot be run' "$t/c.out")" 1

# the second run of one scenario: the session's own, and fast
$s2 test --connect "$pid" specs/good-spec.fib --only one-holds > /dev/null 2>&1
a=$(ms); $s2 test --connect "$pid" specs/good-spec.fib --only one-holds > "$t/c.out" 2>&1; rc=$?; live=$(( $(ms) - a ))
a=$(ms); $s2 test specs/good-spec.fib --only one-holds > /dev/null 2>&1; spawned=$(( $(ms) - a ))
ck "the second run of one scenario: exit 0" $rc 0
echo "      one scenario: through the session $live ms, spawned $spawned ms"
ck "the second run is under 5 s and under half the spawned run" "$([ "$live" -lt 5000 ] && [ $(( live * 2 )) -lt "$spawned" ] && echo yes || echo "no ($live ms, spawned $spawned ms)")" yes
echo '{"op":"status"}' | $s2 repl --connect "$pid" --json > "$t/status" 2>&1
ck "the session ran it (status: the last run, 1 scenario)" "$(grep -o '"last":{"scenarios":[0-9]*' "$t/status")" '"last":{"scenarios":1'

echo '{"op":"eval","text":"(reset! live-test-stop true)"}' | $s2 repl --connect "$pid" --json > /dev/null 2>&1
wait "$pid"; ck "the host ends when told: exit 0" $? 0
pid=
sock=$(sed -n 's/^fibc: live at //p' "$t/live.err")
ck "the socket is gone" "$([ -n "$sock" ] && [ ! -e "$sock" ] && echo yes)" yes
echo "testlive: $([ $fail = 0 ] && echo ok || echo FAILED)"
exit $fail
