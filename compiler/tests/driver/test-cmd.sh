#!/bin/bash
# compiler/tests/driver/test-cmd.sh: `fibc test` (compiler/driver/test.fib, docs/design/test-harness.md 6) as a program: the exit status for a
# holding, a failing, a trapping, a hanging and a non-compiling spec, directory discovery of `*-spec.fib`, `--only` across files, the three formats
# and `--jobs`. Every check can fail: the specs it runs are the planted faults (a wrong claim, a trap, a hang, a syntax error), and the
# status and the counts are what is compared.
# usage: test-cmd.sh STAGE2        (run from the repository root; FIB_LIB defaults to the tree's lib)
set -uo pipefail
s2=${1:?usage: test-cmd.sh STAGE2}
root=$PWD
export FIB_LIB=${FIB_LIB:-$root/lib}
ulimit -v 16000000
t=$(mktemp -d "$HOME/.cache/fibber-scratch/test-cmd.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
mkdir -p "$t/specs/sub" "$t/empty"
cat > "$t/specs/good-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "Good"
    (scenario "one holds" (given [a 1]) (then (expect = 1 a)))
    (scenario "two holds" (given [a 2]) (then (expect = 2 a)))))
(defun main () -> i64 (run-main (specs)))
EOF
cat > "$t/specs/sub/bad-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "Bad"
    (scenario "a wrong claim" (given [a 1]) (then (expect = 2 a)))
    (scenario "a right one" (given [a 1]) (then (expect = 1 a)))))
(defun main () -> i64 (run-main (specs)))
EOF
cat > "$t/trap-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "Trap"
    (scenario "traps" (given [v [1 2]]) (then (expect = 9 (nth v 7))))
    (scenario "after" (then (expect = 1 1)))))
(defun main () -> i64 (run-main (specs)))
EOF
cat > "$t/hang-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "Hang"
    (scenario "loops" (given [n 0]) (then (expect = 1 (loop ((i 0)) (if (< i 0) 1 (recur (+ i 1)))))))
    (scenario "after" (then (expect = 1 1)))))
(defun main () -> i64 (run-main (specs)))
EOF
echo '(ns main (:use fib.test.core) (defspecs' > "$t/broken-spec.fib"
# skip-unless: a scenario whose condition is false is reported as skipped (no pass, no failure) and none of its steps runs (its given would trap);
# one whose condition holds runs, and fails when its claim is wrong.
mkdir -p "$t/skips" "$t/skipfail"
cat > "$t/skips/s-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "Skips"
    (scenario "needs a thing that is not here"
      (skip-unless (= 1 2) "no such thing")
      (given [v [1 2]])
      (then (expect = 9 (nth v 7))))
    (scenario "holds" (skip-unless (= 1 1) "never") (given [a 1]) (then (expect = 1 a)))))
(defun main () -> i64 (run-main (specs)))
EOF
cat > "$t/skipfail/f-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run))
(defspecs specs
  (feature "NotSkipped"
    (scenario "the condition holds, so the wrong claim is judged" (skip-unless (= 1 1) "never") (given [a 1]) (then (expect = 2 a)))))
(defun main () -> i64 (run-main (specs)))
EOF
cd "$t" || exit 2

$s2 test specs/good-spec.fib > "$t/o1" 2> "$t/e1"; ck "a holding spec: exit 0" $? 0
ck "a holding spec: the total line" "$(grep '^total:' "$t/o1")" "total: 2 scenarios: 2 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)"
$s2 test specs/sub/bad-spec.fib > "$t/o2" 2> "$t/e2"; ck "a failing spec: exit 1" $? 1
ck "a failing spec: the location of the expect is printed" "$(grep -c 'at .*bad-spec.fib:4:' "$t/o2")" 1
$s2 test trap-spec.fib > "$t/o3" 2> "$t/e3"; ck "a trapping scenario: exit 1" $? 1
ck "a trapping scenario: one trap, the one after it ran" "$(grep '^total:' "$t/o3")" "total: 2 scenarios: 1 pass, 0 fail, 1 trap, 0 timeout in 1 spec files (seed 1)"
$s2 test hang-spec.fib --timeout 1 > "$t/o4" 2> "$t/e4"; ck "a hanging scenario with --timeout: exit 1" $? 1
ck "a hanging scenario: one timeout, the one after it ran" "$(grep '^total:' "$t/o4")" "total: 2 scenarios: 1 pass, 0 fail, 0 trap, 1 timeout in 1 spec files (seed 1)"
$s2 test broken-spec.fib > "$t/o5" 2> "$t/e5"; ck "a spec that does not compile: exit 2" $? 2
ck "a spec that does not compile: said" "$(grep -c 'ERROR: does not compile' "$t/o5")" 1

$s2 test specs > "$t/o6" 2> "$t/e6"; ck "a directory (both specs, one at any depth): exit 1" $? 1
ck "a directory: the total over both files" "$(grep '^total:' "$t/o6")" "total: 4 scenarios: 3 pass, 1 fail, 0 trap, 0 timeout in 2 spec files (seed 1)"
$s2 test specs -j 2 > "$t/o7" 2> /dev/null; ck "-j 2 over two files: exit 1" $? 1
ck "-j 2: the same total" "$(grep '^total:' "$t/o7")" "total: 4 scenarios: 3 pass, 1 fail, 0 trap, 0 timeout in 2 spec files (seed 1)"
$s2 test specs --only one-holds > "$t/o8" 2> "$t/e8"; ck "--only matching in one file only is not an error: exit 0" $? 0
ck "--only across files: one scenario" "$(grep '^total:' "$t/o8")" "total: 1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)"
$s2 test specs --only nothing-matches-this > /dev/null 2> "$t/e9"; ck "--only matching nothing anywhere: exit 2" $? 2
ck "--only matching nothing: said" "$(grep -c 'nothing to run is not a pass' "$t/e9")" 1

$s2 test specs --format json > "$t/o10" 2> /dev/null; ck "json: exit 1" $? 1
ck "json: summary" "$(grep -o '"summary":{[^}]*}' "$t/o10")" '"summary":{"total":4,"pass":3,"fail":1,"trap":0,"timeout":0}'
ck "json: two files, four results" "$(grep -o '"file":' "$t/o10" | wc -l | tr -d ' ') $(grep -o '"suite":' "$t/o10" | wc -l | tr -d ' ')" "2 4"
ck "json: the broken step has its position" "$(grep -c '"status":"broke","expected":"2","actual":"1","at":"[^"]*bad-spec.fib:4:' "$t/o10")" 1
$s2 test specs --format tap > "$t/o11" 2> /dev/null; ck "tap: exit 1" $? 1
ck "tap: one plan for both files, numbered in order" "$(grep -E '^(1\.\.|ok |not ok )' "$t/o11" | sed -E 's/ - .*//' | tr '\n' '|')" "1..4|ok 1|ok 2|not ok 3|ok 4|"
$s2 test specs --list > "$t/o12" 2> /dev/null; ck "--list: exit 0" $? 0
ck "--list: the ids, nothing run" "$(grep -c '^Good/' "$t/o12") $(grep -c '^Bad/' "$t/o12")" "2 2"

$s2 test skips > "$t/o20" 2> /dev/null; ck "skip: a skipped scenario is no failure: exit 0" $? 0
ck "skip: the total says skipped, and the skipped one is no pass" "$(grep '^total:' "$t/o20")" "total: 2 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout, 1 skipped in 1 spec files (seed 1)"
ck "skip: the row says skip and why" "$(grep -c 'skip  needs a thing' "$t/o20") $(grep -c 'skipped: no such thing' "$t/o20")" "1 1"
$s2 test skips --format json > "$t/o21" 2> /dev/null; ck "skip: json exit 0" $? 0
ck "skip: json summary" "$(grep -o '"summary":{[^}]*}' "$t/o21")" '"summary":{"total":2,"pass":1,"fail":0,"trap":0,"timeout":0,"skip":1}'
$s2 test skips --format tap > "$t/o22" 2> /dev/null; ck "skip: tap exit 0" $? 0
ck "skip: tap marks it SKIP with the reason" "$(grep -c '^ok 1 - .* # SKIP no such thing' "$t/o22")" 1
$s2 test skipfail > /dev/null 2>&1; ck "skip: a condition that holds runs the scenario: its wrong claim fails, exit 1" $? 1

$s2 test --bogus > /dev/null 2>&1; ck "an unknown option: exit 2" $? 2
$s2 test --format yaml specs > /dev/null 2>&1; ck "a bad format: exit 2" $? 2
$s2 test empty > /dev/null 2>&1; ck "a directory without specs: exit 2" $? 2
$s2 test nope-spec.fib > /dev/null 2>&1; ck "a path that is not there: exit 2" $? 2
# -I (LIBFIX-1): `test` passes each -I DIR to its children as module roots, and a -I DIR that holds a prelude.fib is also the library (FIB_LIB's meaning, taken when
# FIB_LIB is unset), so a spec runs from any directory with `-I /path/to/lib`; before, the prelude was read from `lib/` of the working directory and a spec outside
# the repository failed with "cannot read lib/prelude.fib". The planted faults: the same run without -I (must fail, status 2), and a module root that is not given (the
# spec requires a module that lives only under the second -I).
mkdir -p "$t/extra/helper" "$t/away"
printf '(ns helper.greet)\n(defun greeting () -> str "hello")\n' > "$t/extra/helper/greet.fib"
cat > "$t/away/uses-spec.fib" <<'EOF'
(ns main (:use fib.test.core fib.test.run) (:require [helper.greet :as g]))
(defspecs specs
  (feature "Roots"
    (scenario "a module from the second -I" (then (expect = "hello" (g/greeting))))))
(defun main () -> i64 (run-main (specs)))
EOF
(unset FIB_LIB; cd "$t/away" && $s2 test -I "$root/lib" -I "$t/extra" uses-spec.fib > "$t/away.out" 2>&1); ck "-I lib and -I DIR from another directory, FIB_LIB unset: exit 0" $? 0
(unset FIB_LIB; cd "$t/away" && $s2 test uses-spec.fib > /dev/null 2>&1); ck "the same without -I: exit 2 (no library, no module root)" $? 2
(unset FIB_LIB; cd "$t/away" && $s2 test -I "$root/lib" uses-spec.fib > /dev/null 2>&1); ck "with the library but without the module root: exit 2" $? 2
echo "test-cmd: $([ $fail = 0 ] && echo ok || echo FAILED)"
exit $fail
