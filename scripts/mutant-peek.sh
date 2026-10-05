#!/bin/bash
# scripts/mutant-peek.sh: the mutation review of cell peeks (spec/types.md §6.3, "Cell peeks"; performance batch 6, P6). Copies compiler/, lib/
# and rt of this tree to a scratch directory, applies ONE mutant of a rule of the peek, builds a stage 2 from the copy (nothing in the
# tree changes) and runs the ownership cases that guard the rule (and compiler/tests/own/peek.sh for the mutants that only lose the elision):
# at least one must FAIL (a wrong answer, a trap, a crash, an audit failure or a build error all count). A mutant under which every case passes
# means the rule has no case.
#
# usage: scripts/mutant-peek.sh [MUTANT..]        (default: all)
#   no-sibling      P3: a sibling operand that names the cell no longer stops the peek                       (cases 286, 287)
#   closure         P1: a cell that a `fn` literal captures is still a peek                                  (case 288)
#   value           P1: a cell that is a value (stored, passed) is still a peek                              (cases 289, 291)
#   any-init        P1: any `let` binding of a cell is private, not only `(cell e)`                          (case 291)
#   atom            P1: `(atom e)` bindings are cells for the plan                                           (case 292: the emitter refuses)
#   tail            P2: a call in tail position may peek                                                     (case 290)
#   owned-position  P2: an owned or stored position may peek                                                 (case 294)
#   object-field    P2: a field read of an object value may peek                                             (case 295)
#   retain-peek     emitter: a peek loads and retains, though the plan has no temporary to release it         (the audit: a leak)
#   no-peek         walker: no `@c` is a peek, as before the rule (sound, slower)                          (compiler/tests/own/peek.sh)
# environment: FIBC (the stage 2 that builds the mutant; default the gate's F of this tree), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-peek), MUT_J (cases at once, default 2).
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(no-sibling closure value any-init atom tail owned-position object-field retain-peek no-peek)
CASES="286- 287- 288- 289- 290- 291- 292- 293- 294- 295-"
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-peek}
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
FIBC=${FIBC:-$gate_f}
[ -x "$FIBC" ] || { echo "mutant-peek: no fibc to build with: set FIBC" >&2; exit 2; }
root=$R; GATE_OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-peek}; mkdir -p "$GATE_OUT"
. "$R/scripts/lib/stage2.sh"; llvm_link_args || exit 2   # LLVM_ARGS: how a stage 2 links LLVM (LLVM_LINK, LLVM_LIBDIR)
unset LD_LIBRARY_PATH

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the mutant)
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-peek: pattern not found in $1: $2" >&2; exit 2; }
}
mutate() {
  local name=$1 t=$2 f
  case $name in
    no-sibling)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(some b\) \(not \(names-cell\? b head args i\)\)\)/((some b) true)/' ;;
    closure)
      f=$t/compiler/own/peek.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(with s \(bad \(into \(\. s bad\) \(\. lit captures\)\)\)\)/s/' ;;
    value)
      f=$t/compiler/own/peek.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(ELocal b\) \(with s \(bad \(conj \(\. s bad\) b\)\)\)\)/((ELocal b) s)/' ;;
    any-init)
      f=$t/compiler/own/peek.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(if \(cell-call\? \(\. b init\)\) \(some v\) nil\)/(some v)/' ;;
    atom)
      f=$t/compiler/own/peek.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(= \(\. \(nth \(builtin-table\) b\) name\) "cell"\)/(or (= (. (nth (builtin-table) b) name) "cell") (= (. (nth (builtin-table) b) name) "atom"))/' ;;
    tail)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(if \(and \(not tail\) \(borrow-position\? callee params i\)\)/(if (borrow-position? callee params i)/' ;;
    owned-position)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(and \(= \(\. p class\) ClBorrow\) \(not \(\. p amp\)\)/(and (not (. p amp))/' ;;
    object-field)
      f=$t/compiler/own/walk/expr.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(if \(and \(not \(w-is-object w e\)\) \(some\? \(peek-cell w x\)\)\)/(if (some? (peek-cell w x))/' ;;
    retain-peek)
      f=$t/compiler/emit/lower/cells.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(cell-load cx \(\. o sname\) \(v-text \(\. r fst\)\) \(nth args 0\)\)\)\)/(try-let ((v (cell-load cx (. o sname) (v-text (. r fst)) (nth args 0)))) (do (lcx-retain cx v) (Ok v)))))/' ;;
    no-peek)
      f=$t/compiler/own/walk/call.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(if \(and \(contains\? @\(\. w peekable\) v\) \(w-is-object w x\)\) b nil\)/nil/' ;;
    *) echo "mutant-peek: no mutant $name" >&2; exit 2 ;;
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
  elif ! "$T/tree/compiler/tests/own/peek.sh" "$T/F" > "$T/peek.log" 2>&1; then
    echo "mutant $m: KILLED by compiler/tests/own/peek.sh ($(tail -n 1 "$T/peek.log"))"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case and peek.sh passed; see $T/cases.log)"; survived=$((survived+1))
  fi
done
echo "mutant-peek: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
