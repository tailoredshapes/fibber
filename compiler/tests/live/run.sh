#!/bin/bash
# compiler/tests/live/run.sh: the checks of the live-session prototype (compiler/live, docs/design/live-repl.md, compiler/live/FINDINGS.md).
#   compiler/tests/live/run.sh [FIBC] [--update]     FIBC is stage 2 (default $FIBC); --update rewrites the transcripts (read the diff)
# Builds the tool `live` from this tree with FIBC, then:
#   1. the session transcripts: live run NAME.in must print NAME.transcript (the `; N ms` lines are normalised): session.in is the
#      resident session (a def that persists, a redefinition swapped, a signature change refused two ways, a trap caught, a def after
#      boot refused); values.in is the shared numbering (new types, keywords, a record, a closure kept in an atom, strings)
#   2. the swap under load at the lIR level (live load): 8 threads x 1000 swaps PASS at -O 0 and -O 2; a mismatched body refused
#   3. the session under load: 8 workers spawned in the session call handler while it is redefined 40 times; no bad answer
#   4. the planted faults, each of which must turn a check red (a FAIL line, a refusal that is missing, a signal):
#      free-first no-wait two-step no-count (live.swap, live.stub) and skip-check (live.swap, live.session), skip-program-check (live.session)
# Exit 0 when every check holds.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
update=
fibc=${FIBC:-}
for a in "$@"; do case $a in --update) update=1 ;; *) fibc=$a ;; esac; done
[ -n "$fibc" ] || { echo "usage: run.sh FIBC [--update] (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
out=${TMPDIR:-/tmp}/live-test-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
LLVMLIB=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
if ! "$fibc" build compiler/live.fib -I compiler -I lib -L "$LLVMLIB" -l LLVM-21 -o "$out/live" > "$out/build.log" 2>&1; then
  echo "FAIL: live does not build"; tail -5 "$out/build.log"; exit 1
fi
live=$out/live
bad=0
norm() { sed -E 's/^; [0-9]+ ms$/; N ms/; s/^; session total [0-9]+ ms$/; session total N ms/' "$1"; }

# 1. transcripts
for n in session values; do
  timeout 600 "$live" run "$here/$n.in" > "$out/$n.raw" 2>&1; rc=$?
  norm "$out/$n.raw" > "$out/$n.out"
  if [ -n "$update" ]; then cp "$out/$n.out" "$here/$n.transcript"; echo "updated $n.transcript"; continue; fi
  if [ "$rc" != 0 ]; then echo "FAIL $n: exit $rc"; bad=1; fi
  if diff -u "$here/$n.transcript" "$out/$n.out" > "$out/$n.diff"; then echo "ok $n transcript"; else echo "FAIL $n: differs from the transcript"; head -30 "$out/$n.diff"; bad=1; fi
done

# 2. the lIR-level load
load() { timeout 300 "$live" load "$@" > "$out/load.log" 2>&1; echo $?; }
for opt in 0 2; do
  rc=$(load --threads 8 --swaps 1000 --opt "$opt")
  if [ "$rc" = 0 ] && grep -q "^load PASS" "$out/load.log" && grep -q "bad 0," "$out/load.log"; then echo "ok load -O $opt: $(grep ^calls "$out/load.log")"; else echo "FAIL load -O $opt (rc $rc)"; cat "$out/load.log"; bad=1; fi
done
rc=$(load --threads 8 --swaps 10 --mismatch)
if [ "$rc" = 1 ] && grep -q "refused" "$out/load.log"; then echo "ok load mismatch refused"; else echo "FAIL load mismatch not refused (rc $rc)"; cat "$out/load.log"; bad=1; fi

# 3. the session under load
python3 "$here/gen-load.py" 8 40 > "$out/load.in"
timeout 600 "$live" run "$out/load.in" > "$out/loadsess.raw" 2>&1; rc=$?
if [ "$rc" = 0 ] && [ "$(grep -c "swapped f.handler" "$out/loadsess.raw")" = 40 ] && grep -q "^; bad 0$" "$out/loadsess.raw" && ! grep -q "refused\|did not run" "$out/loadsess.raw"; then
  echo "ok session under load: $(grep '^; calls' "$out/loadsess.raw")"
else echo "FAIL session under load (rc $rc)"; grep -v "^fib> (defun handler\|^; redefined handler\|^; [0-9]* ms$" "$out/loadsess.raw" | tail -12; bad=1; fi

# 4. the planted faults
fault_load() {  # name, args..: red when the status is not 0 or the log says FAIL
  f=$1; shift; timeout 300 "$live" load "$@" --fault "$f" > "$out/fault.log" 2>&1; rc=$?
  if [ "$rc" != 0 ] || grep -q "FAIL" "$out/fault.log"; then echo "ok fault $f is red (rc $rc: $(grep -o 'bad [0-9]*' "$out/fault.log" | head -1; grep -o 'signal [0-9]*' "$out/fault.log" | head -1))"; else echo "FAIL fault $f is not red"; cat "$out/fault.log"; bad=1; fi; }
fault_load free-first --threads 8 --swaps 1000
fault_load no-wait --threads 8 --swaps 300 --work 100000
fault_load two-step --threads 16 --swaps 5000 --work 1
fault_load no-count --threads 8 --swaps 300 --work 100000
fault_load skip-check --threads 8 --swaps 1000 --mismatch
for f in skip-check skip-program-check; do  # red when the session's transcript is not the expected one (a refusal missing, garbage, a signal)
  timeout 600 "$live" run "$here/session.in" --fault "$f" > "$out/fs.raw" 2>&1; rc=$?
  norm "$out/fs.raw" > "$out/fs.out"
  if [ "$rc" != 0 ] || ! diff -q "$here/session.transcript" "$out/fs.out" > /dev/null; then echo "ok fault $f is red (rc $rc, $(diff "$here/session.transcript" "$out/fs.out" | grep -c '^>') lines differ)"; else echo "FAIL fault $f is not red"; bad=1; fi
done
[ "$bad" = 0 ] && echo "live checks: ok" || echo "live checks: FAILED"
exit "$bad"
