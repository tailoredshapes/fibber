#!/bin/bash
# scripts/mutant-elem-get.sh: the mutation review of the borrowing element read (spec/types.md §6.3, "Element reads"; lever B of
# performance batch 4). Copies compiler/, lib/ and rt of this tree to a scratch directory, applies ONE mutant of a rule
# of the element read, builds a stage 2 from the copy (nothing in the tree changes) and runs the ownership cases that guard the rule
# against it: at least one must FAIL (a wrong answer, a trap, a crash, an audit failure or a build error all count). A case that
# still passes survived the mutant; a mutant under which every case passes means the rule has no case.
#
# usage: scripts/mutant-elem-get.sh [MUTANT..]        (default: all)
#   never-retain    the emitter takes no count at any `array-get`, as if every element read were `Derived`
#   always-retain   the emitter retains at every `array-get`, as if no read were `Derived` (a `Derived` read then holds two counts)
#   no-pend         own.lastuse: the operands of a call are not siblings, so a sibling operand may take the array at its last use
#   no-derived-read own.lastuse: reading a derived binding no longer reads its root, so the root may be moved or released early
#   keep-temp-part  own.walk.call: a read through a part of the step's own temporary stays `Derived` of it, though the temporary is
#                   released at the end of the step
# environment: FIBC (the fibc that builds the mutant; default the gate's F of this tree, else target/debug/fibc of the main
#   checkout), LAIR_DIR (the directory with liblair.so), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-elem-get),
#   MUT_J (cases at once, default 2), MUT_CASES.
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(never-retain always-retain no-pend no-derived-read keep-temp-part)
CASES=${MUT_CASES:-"270- 271- 272- 273- 274- 275- 276- 277- 278-"}
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-elem-get}
main_target=$(cd "$R" && git rev-parse --git-common-dir | sed 's|/\.git$||')/target/debug
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
if [ -z "${FIBC:-}" ]; then if [ -x "$gate_f" ]; then FIBC=$gate_f; else FIBC=$main_target/fibc; fi; fi
LAIR_DIR=${LAIR_DIR:-$main_target}
[ -x "$FIBC" ] || { echo "mutant-elem-get: no fibc to build with: set FIBC" >&2; exit 2; }
[ -e "$LAIR_DIR/liblair.so" ] || { echo "mutant-elem-get: no liblair.so in $LAIR_DIR: set LAIR_DIR" >&2; exit 2; }
export LD_LIBRARY_PATH=$LAIR_DIR

# mutate NAME TREE: edits the copy; a pattern that is not found is a setup error (the source changed under the mutant).
sub() { # sub FILE PERL-SUBSTITUTION
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-elem-get: pattern not found in $1: $2" >&2; exit 2; }
}
mutate() {
  local name=$1 t=$2 f
  case $name in
    never-retain)
      f=$t/compiler/emit/lower/builtins.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(some x\) \(match \(\. x mode\) \(\(MDerived _\) true\) \(_ false\)\)\)/((some x) true)/' ;;
    always-retain)
      f=$t/compiler/emit/lower/builtins.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(MDerived _\) true\) \(_ false\)\)\)\n    \(_ false\)\)\)\n\n;; An element of/((MDerived _) false) (_ false)))\n    (_ false)))\n\n;; An element of/' ;;
    no-pend)
      f=$t/compiler/own/lastuse.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(ECall _ _\) true\)\n    \(\(EConcat _\) true\)/((ECall _ _) false)\n    ((EConcat _) true)/' ;;
    no-derived-read)
      f=$t/compiler/own/lastuse.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(ids-of \[\(some \(vid x\)\) \(pid s\)\]\)\n\s+\(ids-of \[\(wid s\)\]\)\)\)/(ids-of [(some (vid x)) (pid s)])\n                    (set-empty)))/' ;;
    keep-temp-part)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(do \(set! res \(if own-temp \(MOwned false\) rm\)\)/(do (set! res rm)/' ;;
    *) echo "mutant-elem-get: no mutant $name" >&2; exit 2 ;;
  esac
  find "$t" -name '*.orig' -delete
}

killed=0; survived=0
for m in "${MUTANTS[@]}"; do
  T=$OUT/$m; rm -rf "$T"
  mkdir -p "$T/tree" "$T/tree/cases" "$T/tmp"
  export TMPDIR=$T/tmp
  cp -r "$R/compiler" "$R/lib" "$T/tree/"
  cp -r "$R/rt" "$T/tree/"
  cp -r "$R/cases/ownership" "$T/tree/cases/"
  mutate "$m" "$T/tree" || exit 2
  # `F cases` finds the implicit library through FIB_LIB, else the one beside the binary
  export FIB_LIB=$T/tree/lib
  if ! (cd "$T/tree" && ulimit -v 16000000 && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L "$LAIR_DIR" -l lair -o "$T/F") > "$T/build.log" 2>&1; then
    echo "mutant $m: KILLED (the mutant does not even build: $(tail -n 1 "$T/build.log"))"; killed=$((killed+1)); continue
  fi
  (cd "$T/tree" && ulimit -v 16000000 && "$T/F" cases cases/ownership --only $CASES -j "${MUT_J:-2}") > "$T/cases.log" 2>&1
  failing=$(grep -cE '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log")
  if [ "$failing" -gt 0 ]; then
    echo "mutant $m: KILLED by $(grep -E '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log" | awk '{print $1}' | cut -c1-4 | tr '\n' ' ')"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case passed; see $T/cases.log)"; survived=$((survived+1))
  fi
done
echo "mutant-elem-get: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
