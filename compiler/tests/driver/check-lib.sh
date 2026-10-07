#!/bin/bash
# `fibc check MODULE..` (LIBFIX-1): the full check of every module of the library, called or not. run, build and emit check a library definition only when the
# program reaches it (types.infer.demand), so a type or ownership error in a function nobody calls surfaced at its first caller (TLS, HTTP, JSON agents). Two parts:
#  1. the library: every module under lib/fib/** (the names its `(ns ..)` forms give) is checked in one `check`: it must exit 0. This is the gate's check of
#     uncalled library code (scripts/tools.sh, sh-driver-check-lib); a module that fails is named.
#  2. the planted faults: in a copy of the tree, an uncalled function with a type error (`planted-bad-type`) and, replacing it, one with an ownership error
#     (`planted-bad-own`: one cell passed to two `&` parameters) is appended to fib.char. `run` of a program that requires the module and calls neither must still SUCCEED (the gap this closes:
#     it is why the check exists); `check fib.char` must FAIL (exit 3) and say which; with several modules `check` must name the failing one (`FAIL fib.char`)
#     and not the others. A check that passed on the planted tree would be a check that cannot fail.
# usage: check-lib.sh FIBC     (run from anywhere; it changes into the repository root; scratch under TMPDIR)
set -uo pipefail
FIBC=${1:?usage: check-lib.sh FIBC}; R=$(cd "$(dirname "$0")/../../.." && pwd); cd "$R" || exit 2
S=$(mktemp -d "${TMPDIR:-/tmp}/check-lib.XXXXXX"); trap 'rm -rf "$S"' EXIT
mods=$(find lib/fib -name '*.fib' | sort | xargs sed -n 's/^(ns \([^ )]*\).*/\1/p' | sort -u | tr '\n' ' ')
n=$(echo $mods | wc -w)
t0=$(date +%s)
# shellcheck disable=SC2086
out=$("$FIBC" check $mods 2>&1); rc=$?
echo "library: $n modules, exit $rc, $(( $(date +%s) - t0 )) s: $(echo "$out" | head -3)"
[ $rc -eq 0 ] || { echo "FAIL: the library does not check"; echo "$out" | head -20; exit 1; }
# part 2
mkdir -p "$S/tree"; cp -r lib "$S/tree/lib"
{
  echo
  echo '(defun planted-bad-type (x: i64) -> str x)'
} >> "$S/tree/lib/fib/char.fib"
printf '(ns main (:require [fib.char :as c]))\n(defun main () -> i64 0)\n' > "$S/tree/main.fib"
cd "$S/tree" || exit 2
r=$("$FIBC" run main.fib 2>&1); rr=$?
echo "run on the planted tree (nothing calls the faults): exit $rr: $(echo "$r" | head -1)"
[ $rr -eq 0 ] || { echo "FAIL: run rejected an uncalled function (the demand check changed: update this script)"; exit 1; }
c1=$("$FIBC" check fib.char 2>&1); rc1=$?
echo "check fib.char on the planted tree: exit $rc1: $(echo "$c1" | sed -n 2p)"
if [ $rc1 -ne 3 ] || ! echo "$c1" | grep -q "cannot unify"; then echo "FAIL: check did not reject the planted type error"; echo "$c1"; exit 1; fi
c2=$("$FIBC" check fib.core fib.char fib.seq 2>&1); rc2=$?
echo "check fib.core fib.char fib.seq: exit $rc2: $(echo "$c2" | grep '^FAIL' | cut -c1-80)"
if [ $rc2 -ne 3 ] || ! echo "$c2" | grep -q "^FAIL fib.char" || echo "$c2" | grep -q "^FAIL fib.core"; then echo "FAIL: check did not name only fib.char"; echo "$c2"; exit 1; fi
# the ownership fault alone (the demand check finds this one too, being syntactic: `run` rejects it; the type error above is the gap)
sed -i '/planted-bad-type/d' lib/fib/char.fib
{
  echo '(defun planted-two (&a: (Array i64) &b: (Array i64)) -> unit (do (array-push! &a 1) (array-push! &b 2)))'
  echo '(defun planted-bad-own () -> i64 (let [x (cell (array 1 0))] (do (planted-two &x &x) (array-len @x))))'
} >> lib/fib/char.fib
c3=$("$FIBC" check fib.char 2>&1); rc3=$?
echo "check fib.char, ownership fault only: exit $rc3: $(echo "$c3" | sed -n 2p | cut -c1-120)"
if [ $rc3 -ne 3 ] || ! echo "$c3" | grep -q "more than one"; then echo "FAIL: check did not reject the planted ownership fault"; echo "$c3"; exit 1; fi
echo "check-lib: ok"
