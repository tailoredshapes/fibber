#!/bin/bash
# scripts/mutant-regex-literal.sh: the mutation review of the regex literal and the split `Regex` (stdlib §4.9; batch 6, RXM).
# Copies compiler/, lib/ and cases/stdlib of this tree to a scratch directory, applies ONE mutant, builds a stage 2 from the copy when
# the mutant is in compiler/ (a library mutant runs the tree's F against the mutated lib: FIB_LIB), and runs the regex cases 6100-6106:
# at least one must FAIL (a wrong answer, a trap, a crash, a refused program or a build error all count). A mutant under which every
# case passes means the rule has no case.
#
# usage: scripts/mutant-regex-literal.sh [MUTANT..]        (default: all)
#   no-hoist        expand.regex: a literal is a call of `re-pattern` where it stands, not a reference to a def made at compile time
#   not-constant    types.lower: `(re-pattern "lit")` is not in the constant grammar, so the def is made at run time
#   reader-backslash syntax.literal: a backslash in `#"..."` no longer holds the character after it, so `#"a\"b"` ends at the escaped quote
#   no-validation   expand.regex: a pattern the library refuses is not an error at the literal
#   tab-flag        regex.dfa: `tab-forward` records the end of a match one byte early
#   tab-incomplete  regex.dfa: `freeze-dfa` does not explore the transitions (the table keeps its unmade entries)
#   no-lazy-state   regex.api: a search with a lazy DFA runs with no matcher state (`dfa-on?` ignores the matcher)
#   always-table    regex.dfa: a table is trusted though the states did not all fit the budget (`ok` is always true)
# environment: F (the stage 2 of this tree; default the gate's F), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-regex-literal),
#   MUT_J (cases at once, default 2), LLVM_LIBDIR.
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(no-hoist not-constant reader-backslash no-validation tab-flag tab-incomplete no-lazy-state always-table)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-regex-literal}
F=${F:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
LLVM_LIBDIR=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
[ -x "$F" ] || { echo "mutant-regex-literal: no stage 2 F: set F" >&2; exit 2; }
CASES="6100 6101 6102 6103 6104 6105 6106"

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the mutant)
  cp "$1" "$1.orig"
  perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-regex-literal: pattern not found in $1: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
# mutate NAME TREE: prints "compiler" when the mutant needs a new stage 2, "lib" when it does not
mutate() {
  local name=$1 t=$2
  case $name in
    no-hoist)
      sub "$t/compiler/expand/regex.fib" 's/\(Ok \(sym name pos\)\)/(Ok (Stx (SList [(sym "fib.regex.api\/re-pattern" pos) (Stx (SStr text) pos)]) pos))/'; echo compiler ;;
    not-constant)
      sub "$t/compiler/types/lower/mod.fib" 's/\(or \(regex-literal-call\? g head args\)/(or false/'; echo compiler ;;
    reader-backslash)
      sub "$t/compiler/syntax/literal.fib" 's/\(match \(cursor-bump cur\)\n(\s+)\(nil \(Err \(err-at cur start UnterminatedString\)\)\)\n(\s+)\(\(some _\) \(recur\)\)/(match (cursor-peek cur)\n$1(nil (Err (err-at cur start UnterminatedString)))\n$2((some _) (recur))/'
      echo compiler ;;
    no-validation)
      sub "$t/compiler/expand/regex.fib" 's/\(Err msg\) \(Err \(malformed-error "regex literal" \(str-join \[msg " in #\\"" text "\\""\]\) pos\)\)/(Err msg) (Ok (sym "nil" pos))/'; echo compiler ;;
    tab-flag)
      sub "$t/lib/fib/regex/dfa.fib" 's/\(recur \(\+ i w\) nx \(if \(= \(bit-and nx 1\) 1\) \(\+ i w\) last\)\)/(recur (+ i w) nx (if (= (bit-and nx 1) 1) i last))/'; echo lib ;;
    tab-incomplete)
      sub "$t/lib/fib/regex/dfa.fib" 's/\(if \(and \(>= e0 0\) \(explore-all d\)\)/(if (and (>= e0 0) true)/'; echo lib ;;
    no-lazy-state)
      sub "$t/lib/fib/regex/api.fib" 's/\(or \(frozen\? rx\) \(some\? m\)\)/true/'; echo lib ;;
    always-table)
      sub "$t/lib/fib/regex/dfa.fib" 's/\(Tab true e0 /(Tab true e0 /; s/\(if \(and \(>= e0 0\) \(explore-all d\)\)/(if (and (>= e0 0) (do (explore-all d) true))/'; echo lib ;;
    *) echo "mutant-regex-literal: no mutant $name" >&2; exit 2 ;;
  esac
}

killed=0; survived=0
for m in "${MUTANTS[@]}"; do
  T=$OUT/$m; rm -rf "$T"
  mkdir -p "$T/tree/cases" "$T/tmp"
  export TMPDIR=$T/tmp
  cp -r "$R/compiler" "$R/lib" "$T/tree/"
  cp -r "$R/cases/stdlib" "$T/tree/cases/"
  kind=$(mutate "$m" "$T/tree") || { echo "$kind"; exit 2; }
  export FIB_LIB=$T/tree/lib
  FM=$F
  if [ "$kind" = compiler ]; then
    FM=$T/F
    if ! (cd "$T/tree" && ulimit -v 16000000 && "$F" build compiler/fibc.fib -I compiler -I lib -L "$LLVM_LIBDIR" -l LLVM-21 -o "$FM") > "$T/build.log" 2>&1; then
      echo "mutant $m: KILLED (the mutant does not even build: $(tail -n 1 "$T/build.log"))"; killed=$((killed+1)); continue
    fi
  fi
  (cd "$T/tree" && ulimit -v 16000000 && "$FM" cases cases/stdlib --only $CASES -j "${MUT_J:-2}") > "$T/cases.log" 2>&1
  failing=$(grep -cE '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log")
  if [ "$failing" -gt 0 ]; then
    echo "mutant $m: KILLED by $(grep -E '^[0-9]+-.* (FAIL|ERROR|CRASH|PENDING|HEADER)' "$T/cases.log" | awk '{print $1}' | cut -c1-4 | tr '\n' ' ')"; killed=$((killed+1))
  else
    echo "mutant $m: SURVIVED (every case passed; see $T/cases.log)"; survived=$((survived+1))
  fi
done
echo "mutant-regex-literal: $killed killed, $survived survived"
[ "$survived" -eq 0 ]
