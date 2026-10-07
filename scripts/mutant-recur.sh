#!/bin/bash
# scripts/mutant-recur.sh: the mutation review of the `recur` argument rule (spec/types.md §6.3, the `(recur ..)` row; package BUG-1). Copies compiler/,
# lib/ and rt of this tree to a scratch directory, applies ONE mutant, builds a stage 2 from the copy (nothing in the tree changes) and runs the ownership
# cases that guard the rule: at least one must FAIL (a wrong answer, a crash, an audit failure or a build error all count).
#
# usage: scripts/mutant-recur.sh [MUTANT..]        (default: all)
#   handed-over   a recur argument that reads a loop binding moves it though a later argument already handed it over     (case 384: a double free)
# environment: FIBC (the stage 2 that builds the mutant; default the gate's F of this tree), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-recur), MUT_J (cases at once, default 2).
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(handed-over)
CASES="384- 385- 386- 387- 388- 389- 390-"
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-recur}
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
FIBC=${FIBC:-$gate_f}
[ -x "$FIBC" ] || { echo "mutant-recur: no fibc to build with: set FIBC" >&2; exit 2; }
root=$R; GATE_OUT=$OUT; mkdir -p "$GATE_OUT"
. "$R/scripts/lib/stage2.sh"; llvm_link_args || exit 2   # LLVM_ARGS: how a stage 2 links LLVM (LLVM_LINK, LLVM_LIBDIR)
unset LD_LIBRARY_PATH

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the mutant)
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-recur: pattern not found in $1: $2" >&2; exit 2; }
}
mutate() {
  local name=$1 t=$2 f
  case $name in
    handed-over)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/ \(not \(w-env-moved\? w s\)\)\)\n/)\n/' ;;
    *) echo "mutant-recur: no mutant $name" >&2; exit 2 ;;
  esac
  find "$t" -name '*.orig' -delete
}

killed=0; survived=0
for m in "${MUTANTS[@]}"; do
  T=$OUT/$m; rm -rf "$T"
  mkdir -p "$T/tree" "$T/tree/cases" "$T/tree/compiler/tests" "$T/tmp"
  export TMPDIR=$T/tmp
  cp -r "$R/compiler" "$R/lib" "$T/tree/"
  cp -r "$R/rt" "$T/tree/"
  cp -r "$R/cases/ownership" "$T/tree/cases/"
  mutate "$m" "$T/tree" || exit 2
  export FIB_LIB=$T/tree/lib
  if ! (cd "$T/tree" && ulimit -v 16000000 && "$FIBC" build compiler/fibc.fib -I compiler -I lib "${LLVM_ARGS[@]}" -o "$T/F") > "$T/build.log" 2>&1; then
    echo "mutant $m: KILLED (the mutant does not even build: $(tail -n 1 "$T/build.log"))"; killed=$((killed+1)); continue
  fi
  (cd "$T/tree" && ulimit -v 16000000 && "$T/F" cases cases/ownership --only $CASES -j "${MUT_J:-2}") > "$T/cases.log" 2>&1
  failing=$(grep -E '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log" | awk '{print $1}' | cut -c1-3 | tr '\n' ' ')
  if [ -n "$failing" ]; then
    echo "mutant $m: KILLED by cases $failing"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case passed; see $T/cases.log)"; survived=$((survived+1))
  fi
done
echo "mutant-recur: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
