#!/bin/bash
# scripts/mutant-simd-literal.sh: the mutation review of the vector literal `<<..>>` (docs/design/simd-and-tensors.md 2.3; SIMD wave 2, P2).
# Copies compiler/, lib/ and cases/stdlib of this tree to a scratch directory, applies ONE mutant to the reader (compiler/syntax/lexer.fib,
# simd.fib), builds a stage 2 from the copy, and runs the cases 6270-6288: at least one must FAIL (a wrong answer, a trap, a refused or an
# accepted program, a build error all count). A mutant under which every case passes means the rule has no case.
#
# usage: scripts/mutant-simd-literal.sh [MUTANT..]        (default: all)
#   open-eq                 syntax/lexer.fib: `<<=` opens a literal (the `=` exclusion is gone)
#   open-gtgt               syntax/lexer.fib: `<<>` opens a literal
#   open-space              syntax/lexer.fib: `<<` before a space opens a literal
#   close-everywhere        syntax/lexer.fib: `>>` is a closer outside literals too
#   token-runs-through-closesyntax/lexer.fib: `>>` does not end a number or symbol
#   any-suffix              syntax/lexer.fib: every suffix is accepted
#   empty-ok                syntax/simd.fib: `<<>>` is not an error
#   lanes-65                syntax/simd.fib: 65 elements are accepted
#   no-class-check          syntax/simd.fib: literals of different types are accepted
#   no-range                syntax/simd.fib: an integer element is not range-checked against the suffix
#   f32-no-round            syntax/simd.fib: a float element is not rounded to f32
#   exact-2-53              syntax/simd.fib: every float width is 'exact' up to 2^53
#   own-suffix-any          syntax/simd.fib: an element's own suffix may differ from the vector's
#   head-name               syntax/simd.fib: the form's head is not `simd`
# environment: F (the stage 2 that builds the mutants; default the gate's F), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-simd-literal), MUT_J (cases at once, default 2), LLVM_LIBDIR.
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(open-eq open-gtgt open-space close-everywhere token-runs-through-close any-suffix empty-ok lanes-65 no-class-check no-range f32-no-round exact-2-53 own-suffix-any head-name)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-simd-literal}
F=${F:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
LLVM_LIBDIR=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
[ -x "$F" ] || { echo "mutant-simd-literal: no stage 2 F: set F" >&2; exit 2; }
CASES="6270 6271 6272 6273 6274 6275 6276 6277 6278 6279 6280 6281 6282 6283 6284 6285 6286 6287 6288"

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the mutant)
  cp "$1" "$1.orig"
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-simd-literal: pattern not found in $1: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
mutate() { # mutate NAME TREE
  local name=$1 t=$2
  case $name in
    open-eq)
      sub "$t/compiler/syntax/lexer.fib" 's/\n\s+\(not \(= \(cursor-peek-nth cur 2\) \(some \\=\)\)\)//' ;;
    open-gtgt)
      sub "$t/compiler/syntax/lexer.fib" 's/\n\s+\(or \(not \(= \(cursor-peek-nth cur 2\) \(some \\>\)\)\) \(= \(cursor-peek-nth cur 3\) \(some \\>\)\)\)//' ;;
    open-space)
      sub "$t/compiler/syntax/lexer.fib" 's/\(starts-form-char \(cursor-peek-nth cur 2\)\)/true/' ;;
    close-everywhere)
      sub "$t/compiler/syntax/lexer.fib" 's/\(and simd \(= \(cursor-peek-nth cur 1\)/(and true (= (cursor-peek-nth cur 1)/' ;;
    token-runs-through-close)
      sub "$t/compiler/syntax/lexer.fib" 's/\(not \(and simd \(at-simd-close\? cur\)\)\)/true/' ;;
    any-suffix)
      sub "$t/compiler/syntax/lexer.fib" 's/\(or \(= s \"i8\"\) \(= s \"i16\"\)/(or true (= s \"i16\")/' ;;
    empty-ok)
      sub "$t/compiler/syntax/simd.fib" 's/\(= \(count items\) 0\) \(Err/false (Err/' ;;
    lanes-65)
      sub "$t/compiler/syntax/simd.fib" 's/\(def max-lanes: i64 64\)/(def max-lanes: i64 65)/' ;;
    no-class-check)
      sub "$t/compiler/syntax/simd.fib" 's/\(not \(= c want\)\)/false/' ;;
    no-range)
      sub "$t/compiler/syntax/simd.fib" 's/:else\n\s+\(at-pos e \(number\/check-literal \(SInt v \(int-width-named sfx\)\)\)\)\)\)/:else (Ok e)))/' ;;
    f32-no-round)
      sub "$t/compiler/syntax/simd.fib" 's/\(= sfx \"f32\"\) \(at-pos e \(number\/parse-number \(str-concat \(text-of cur e\) \"f32\"\)\)\)/(= sfx \"f32\") (Ok e)/' ;;
    exact-2-53)
      sub "$t/compiler/syntax/simd.fib" 's/\(if \(= sfx \"f32\"\) 16777216 9007199254740992\)/9007199254740992/' ;;
    own-suffix-any)
      sub "$t/compiler/syntax/simd.fib" 's/\(if \(= \(class-of e\) sfx\) \(Ok e\) \(Err \(disagrees cur e sfx\)\)\)/(Ok e)/g' ;;
    head-name)
      sub "$t/compiler/syntax/simd.fib" 's/\(SSym \"simd\"\)/(SSym \"simdx\")/' ;;
    *) echo "mutant-simd-literal: no mutant $name" >&2; exit 2 ;;
  esac
}

only=()
for c in $CASES; do only+=("$c-"); done
killed=0; survived=0
for m in "${MUTANTS[@]}"; do
  T=$OUT/$m; rm -rf "$T"
  mkdir -p "$T/tree/cases" "$T/tmp"
  export TMPDIR=$T/tmp
  cp -r "$R/compiler" "$R/lib" "$T/tree/"
  cp -r "$R/cases/stdlib" "$T/tree/cases/"
  mutate "$m" "$T/tree" || exit 2
  export FIB_LIB=$T/tree/lib
  FM=$T/F
  if ! (cd "$T/tree" && ulimit -v 16000000 && "$F" build compiler/fibc.fib -I compiler -I lib -L "$LLVM_LIBDIR" -l LLVM-21 -o "$FM") > "$T/build.log" 2>&1; then
    echo "mutant $m: KILLED (the mutant does not even build: $(tail -n 1 "$T/build.log"))"; killed=$((killed+1)); continue
  fi
  (cd "$T/tree" && ulimit -v 16000000 && "$FM" cases cases/stdlib --only "${only[@]}" -j "${MUT_J:-2}") > "$T/cases.log" 2>&1
  bad='^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)'
  failing=$(grep -cE "$bad" "$T/cases.log")
  if [ "$failing" -gt 0 ]; then
    echo "mutant $m: KILLED by $(grep -E "$bad" "$T/cases.log" | awk '{print $1}' | cut -c1-4 | tr '\n' ' ')"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case passed; see $T/cases.log)"; survived=$((survived+1))
  fi
  rm -rf "$T/tree" "$T/F"
done
echo "mutant-simd-literal: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
