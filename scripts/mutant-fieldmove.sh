#!/bin/bash
# scripts/mutant-fieldmove.sh: the mutation review of the field move (spec/types.md §6.3 "Field moves"; the optimiser step of lib/fib/tensor/optim-owned.fib). Copies compiler/, lib/ and rt of
# this tree to a scratch directory, applies ONE mutant of a rule, builds a stage 2 from the copy (nothing in the tree changes) and runs the cases that guard the rule: at least one must FAIL
# (a wrong answer, a trap, a crash, an audit failure, an allocation count over its bound or a build error all count). A mutant under which every case passes means the rule has no case.
#
# usage: scripts/mutant-fieldmove.sh [MUTANT..]        (default: all)
#   no-later-read   own.lastuse `record-field`: a later read of the shell no longer stops the steal                  (case 381)
#   always-steal    emit.lower.ops `lcx-steal`: the steal does not test that the shell is unique (pattern steal too)  (cases 380, 264, 266)
#   borrowed-shell  own.walk.state `w-steal-field?`: a shell the frame does not own may be stolen from               (case 383)
#   part-of-shell   own.lastuse `record-field`: the base of the field read may be a part of the shell                (case 382)
#   no-rule4        own.walk.state `w-want-own-field`: a borrowed parameter is not made owned (sound, slower)        (case 379: its allocation bound)
#   no-own          optim-owned.fib `own-f32!`, `own-f64!`: no unique-write protocol before the raw write            (case 8200: a shared tensor is written)
#   take-always     prelude `vec-take-at`: a shared vector's tail slot is emptied without the copy                   (case 8203)
# environment: FIBC (the stage 2 that builds the mutant; default the gate's F of this tree), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-fieldmove), MUT_J (cases at once, default 2).
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(no-later-read always-steal borrowed-shell part-of-shell no-rule4 no-own take-always)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-fieldmove}
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
FIBC=${FIBC:-$gate_f}
[ -x "$FIBC" ] || { echo "mutant-fieldmove: no fibc to build with: set FIBC" >&2; exit 2; }
root=$R; GATE_OUT=$OUT; mkdir -p "$GATE_OUT"
. "$R/scripts/lib/stage2.sh"; llvm_link_args || exit 2   # LLVM_ARGS: how a stage 2 links LLVM (LLVM_LINK, LLVM_LIBDIR)
unset LD_LIBRARY_PATH

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the mutant)
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-fieldmove: pattern not found in $1: $2" >&2; exit 2; }
}
# the cases that guard a mutant: "directory:prefix.."
cases_of() {
  case $1 in
    no-later-read|borrowed-shell|part-of-shell) echo "ownership:379- 380- 381- 382- 383-" ;;
    always-steal) echo "ownership:264- 266- 379- 380- 381- 382- 383-" ;;
    no-rule4) echo "ownership:379-" ;;
    no-own) echo "stdlib:8200- 8201- 8202-" ;;
    take-always) echo "stdlib:8203- 8202-" ;;
  esac
}
mutate() {
  local name=$1 t=$2 f
  case $name in
    no-later-read)
      f=$t/compiler/own/lastuse.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(= r r2\)\s+\(none-live\? u live \[\(some \(\* r 8\)\) \(some \(\+ \(\* r 8\) 1\)\)\]\)/(= r r2)/' ;;
    always-steal)
      f=$t/compiler/emit/lower/ops.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(fb-term f \(str-join \["\(br " \(v-text unique\) " " ls " " lk "\)"\]\)\)/(fb-term f (str-join ["(br " ls ")"]))/' ;;
    borrowed-shell)
      f=$t/compiler/own/walk/state.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(\(Pair \(EField _ _ _\) \(MDerived s\)\)\s+\(and \(w-movable w s\)/((Pair (EField _ _ _) (MDerived s))\n          (and true/' ;;
    part-of-shell)
      f=$t/compiler/own/lastuse.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(match \(\. base kind\)\s+\(\(ELocal _\)\s+\(match \(Pair \(mode-of u base\) s\)\s+\(\(Pair \(MBorrowed \(SiteBind r\)\) \(SiteBind r2\)\)/(match (. base kind)\n    ((EField _ _ _)\n     (match (Pair (mode-of u base) s)\n       ((Pair (MDerived (SiteBind r)) (SiteBind r2))/' ;;
    no-rule4)
      f=$t/compiler/own/walk/state.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(defun w-want-own-field \(w: Walker x: Expr m: Mode\) -> bool\n  \(match \(Pair \(\. x kind\) m\)/(defun w-want-own-field (w: Walker x: Expr m: Mode) -> bool\n  (match (Pair (. x kind) MScalar)/' ;;
    no-own)
      f=$t/lib/fib/tensor/optim-owned.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(array-set! &c o x\)/()/g' ;;
    take-always)
      f=$t/lib/prelude.fib; cp "$f" "$f.orig"
      sub "$f" 's/\(let \(\(e \(array-take! &c \(- i tailoff\)\)\)\)\s+\(Pair e \(VecOf cnt shift root \@c\)\)\)/(let ((e (array-get tail (- i tailoff)))) (Pair e (VecOf cnt shift root tail)))/' ;;
    *) echo "mutant-fieldmove: no mutant $name" >&2; exit 2 ;;
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
  cp -r "$R/cases/ownership" "$R/cases/stdlib" "$T/tree/cases/"
  mutate "$m" "$T/tree" || exit 2
  export FIB_LIB=$T/tree/lib
  if ! (cd "$T/tree" && ulimit -v 16000000 && "$FIBC" build compiler/fibc.fib -I compiler -I lib "${LLVM_ARGS[@]}" -o "$T/F") > "$T/build.log" 2>&1; then
    echo "mutant $m: KILLED (the mutant does not even build: $(tail -n 1 "$T/build.log"))"; killed=$((killed+1)); continue
  fi
  spec=$(cases_of "$m"); dir=${spec%%:*}; only=${spec#*:}
  (cd "$T/tree" && ulimit -v 16000000 && "$T/F" cases "cases/$dir" --only $only -j "${MUT_J:-2}") > "$T/cases.log" 2>&1
  failing=$(grep -E '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log" | awk '{print $1}' | cut -c1-4 | tr '\n' ' ')
  if [ -n "$failing" ]; then
    echo "mutant $m: KILLED by cases $failing"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case passed; see $T/cases.log)"; survived=$((survived+1))
  fi
done
echo "mutant-fieldmove: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
