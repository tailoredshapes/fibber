#!/bin/bash
# scripts/mutant-fork.sh: planted faults for `fib.os.process` fork-run (lib/fib/os/process.fib; cases 7531, 7532). The library is compiled
# with every program, so a mutant needs no new stage 2: the script copies lib/ and the stdlib cases to a scratch directory, breaks ONE
# rule in the copy and runs the cases that pin it with the F you name; every one must FAIL (a wrong answer, an audit leak or a hang
# killed by the harness all count). A case that still passes survived and is a bad case.
# usage: FIBC=F scripts/mutant-fork.sh MODE
#   MODE        what is broken                                                         cases that must fail
#   no-guard    the thread guard gives up after 50 ms and forks anyway                 7532-
#   no-signal   the status decode drops the signal (a trap reads as exit 0)            7531-
#   no-redirect the child does not redirect its standard output                        7531-
#   no-cleanup  the result files are not removed after a child is collected            7531-
#   no-exit     the child returns into the caller instead of `_exit`                   7531-
# environment: FIBC (a stage 2; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-fork-MODE). Cases run under ulimit -v 16000000.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-fork.sh no-guard|no-signal|no-redirect|no-cleanup|no-exit}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-fork-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-fork: no fibc: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  no-guard) CASES=(7532-) ;; no-signal) CASES=(7531-) ;; no-redirect) CASES=(7531-) ;; no-cleanup) CASES=(7531-) ;; no-exit) CASES=(7531-) ;;
  *) echo "mutant-fork: unknown MODE $MODE" >&2; exit 2 ;;
esac
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
export FIB_LIB=$OUT/tree/lib
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-fork: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
P=lib/fib/os/process.fib
case $MODE in
  no-guard)    mut $P 's/\(>= tries 50\) true/(>= tries 50) false/' ;;
  no-signal)   mut $P 's/\(\+ 128 \(bit-and w 127\)\)/(bit-and w 127)/' ;;
  no-redirect) mut $P 's/\(dup2 fo 1i32\) //' ;;
  no-cleanup)  mut $P 's/\(doseq \[ext \[".res" ".out" ".err"\]\] \(files\/remove-file \(str \(\. c base\) ext\)\)\)/()/' ;;
  no-exit)     mut $P 's/\(_exit 0i32\)\n/()\n/' ;;
esac
ulimit -v 16000000
survived=0
for c in "${CASES[@]}"; do
  all=$("$FIBC" cases "$OUT/tree/cases/stdlib" --only "$c" 2>&1) || true   # into a variable first: a pipe into `head -1` kills fibc with SIGPIPE, which pipefail reports
  line=$(printf '%s\n' "$all" | grep -E "^$c" | head -1)
  case $line in
    *" pass"*) echo "SURVIVED  $MODE: $line"; survived=1 ;;
    *) echo "killed    $MODE: ${line:0:200}" ;;
  esac
done
exit $survived
