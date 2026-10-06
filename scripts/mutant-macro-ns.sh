#!/bin/bash
# scripts/mutant-macro-ns.sh: planted faults for MACRO-NS (spec/syntax.md 3.16 "Names in a template" and "Macro-time helpers";
# docs/design/macro-names.md). Copies compiler/, lib/ and the cases to a scratch directory, breaks ONE rule in the copy, builds a stage 2
# from the copy with FIBC (a stage 2 or the seed) and runs the cases that pin the rule: EVERY one listed for the mode must fail.
# usage: FIBC=fibc scripts/mutant-macro-ns.sh MODE
#   MODE             what is broken (file)                                              what must fail
#   resolution-off   templates are not resolved in the macro's module (qualify.fib)     modules 031 032 033 034, stdlib 7930 7931 7932
#   no-gain          the site does not require the modules an expansion names (macrons)  modules 031 032, stdlib 7930 7931 7932
#   binders          a name the template binds is qualified like any other (qualify)     modules 031
#   private-plain    a private helper is written m/x, not (var m/x) (qualify.fib)        modules 032
#   helpers-off      the macro-time module is the prelude only (macros/runner.fib)        modules 035, stdlib 7932 7933
# environment: FIBC (required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-macro-ns-MODE). Run under ulimit -v 16000000.
# exit: 0 when every listed case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-macro-ns.sh resolution-off|no-gain|binders|private-plain|helpers-off}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-macro-ns-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-macro-ns: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
ulimit -v 16000000
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$R/cases/modules" "$OUT/tree/cases/"
q=compiler/expand/qualify.fib; m=compiler/expand/macrons.fib; r=compiler/macros/runner.fib
case $MODE in
  resolution-off) f=$q; perl -0pi -e 's/\(if \(or \(not \@\(\. ctx templating\)\) \(= \(\. s ns\) prelude-ns\)\)/(if true/' "$OUT/tree/$f"
                  mods="031 032 033 034"; libs="7930 7931 7932" ;;
  no-gain)        f=$m; perl -0pi -e 's/\(when \(some\? \(map-get deps q\)\) \(gain ctx q\)\)/(when false (gain ctx q))/' "$OUT/tree/$f"
                  mods="031 032"; libs="7930 7931 7932" ;;
  binders)        f=$q; perl -0pi -e 's/\(bound-names \(nth items 1\)\)/(map-empty)/' "$OUT/tree/$f"
                  mods="031"; libs="" ;;
  private-plain)  f=$q; perl -0pi -e 's/\(= \(\. t kind\) :fun\) \(list-form \[\(sym "var" pos\) full\] pos\)/(= (. t kind) :fun) full/' "$OUT/tree/$f"
                  mods="032"; libs="" ;;
  helpers-off)    f=$r; perl -0pi -e 's/\(match \(helper-program ctx def\)/(match (if true nil (helper-program ctx def))/' "$OUT/tree/$f"
                  mods="035"; libs="7932 7933" ;;
  *) echo "mutant-macro-ns: unknown MODE $MODE" >&2; exit 2 ;;
esac
cmp -s "$OUT/tree/$f" "$R/$f" && { echo "mutant-macro-ns: the mutation changed nothing" >&2; exit 2; }
(cd "$OUT/tree" && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F") > "$OUT/build.log" 2>&1 \
  || { tail -5 "$OUT/build.log"; echo "mutant-macro-ns: the mutant did not build" >&2; exit 2; }
export FIB_LIB=$OUT/tree/lib
survived=0
check() {   # check DIR PREFIX..: every case of DIR named by a prefix must not pass
  local dir=$1; shift
  [ $# -eq 0 ] && return
  local out; out=$(cd "$OUT/tree" && "$OUT/F" cases "cases/$dir" --only "$@" 2>&1)
  for p in "$@"; do
    line=$(printf '%s\n' "$out" | grep -E "^$p-" | head -1)
    [ -n "$line" ] || { echo "mutant-macro-ns: no case $dir/$p" >&2; exit 2; }
    case $line in
      *" pass"*) echo "SURVIVED  $MODE: $dir/${line:0:150}"; survived=1 ;;
      *) echo "killed    $MODE: $dir/${line:0:150}" ;;
    esac
  done
}
check modules $mods
check stdlib $libs
exit $survived
