#!/bin/bash
# scripts/mutant-lang2.sh: planted faults for package LANG-2's compiler changes (constructors as values, the order of `def`s, integer literals at narrower
# widths, literal patterns, the note on a shadowed prelude type). Copies compiler/ and lib/ to a scratch directory, breaks ONE rule in the copy, builds a
# stage 2 from the copy with FIBC and runs the cases that pin the rule: each must NOT pass.
# usage: FIBC=fibc scripts/mutant-lang2.sh MODE
#   MODE          what is broken                                                         what must fail
#   defs-order    a made def's body is requested before the constants are made           8367 8368 (`def base is read where it has no value`)
#   no-ctor-eta   a constructor named as a value is not turned into its function         8362 and ownership 395 396
#   lit-no-range  a literal adopts i8, i16, i32 without the range test                   8357
#   lit-no-adopt  a literal never adopts an integer width                                 8356 1503
#   pat-no-adopt  a literal pattern never takes the scrutinee's width                     8356 8359
#   no-shadow-note the arity error of a shadowed prelude type does not say so             8365
# environment: FIBC (required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-lang2-MODE). Run under ulimit -v 16000000.
# exit: 0 when every named case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-lang2.sh defs-order|no-ctor-eta|lit-no-range|lit-no-adopt|pat-no-adopt|no-shadow-note}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-lang2-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-lang2: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
ulimit -v 16000000
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$R/cases/ownership" "$OUT/tree/cases/"
T=$OUT/tree/compiler
case $MODE in
  defs-order)
    f=$T/emit/inits.fib
    perl -0pi -e 's/\(body \(body-name \(program-g p\) \(BodyDef d\) \[\]\)\)/(body (program-request p (BodyDef d) []))/' "$f"; stdlib="8367 8368"; own="" ;;
  no-ctor-eta)
    f=$T/types/lower/expr.fib
    perl -0pi -e 's/\(n \(ctor-as-function lw n form\)\)/(n (global-form lw r name form))/' "$f"; stdlib="8362"; own="395 396" ;;
  lit-no-range)
    f=$T/types/infer/unify.fib
    perl -0pi -e 's/\(= s SI8\) \(<= m 127\)/(= s SI8) true/' "$f"; stdlib="8357"; own="" ;;
  lit-no-adopt)
    f=$T/types/infer/unify.fib
    perl -0pi -e 's/\(= s SI32\) \(<= m 2147483647\)\n\s*\(= s SI16\) \(<= m 32767\)\n\s*\(= s SI8\) \(<= m 127\)\n//' "$f"; stdlib="8356 1503"; own="" ;;
  pat-no-adopt)
    f=$T/types/infer/pattern.fib
    perl -0pi -e 's/\(\(LitInt n \(I64\)\) \(int-pattern-type cx p l n \(store-resolve \(\. cx st\) s\)\)\)/((LitInt n (I64)) (Ok (lit-type l)))/' "$f"; stdlib="8356 8359"; own="" ;;
  no-shadow-note)
    f=$T/types/lower/typeform.fib
    perl -0pi -e 's/\(shadow-note g m head id\)/""/' "$f"; stdlib="8365"; own="" ;;
  *) echo "mutant-lang2: unknown MODE $MODE" >&2; exit 2 ;;
esac
cmp -s "$f" "$R/${f#$OUT/tree/}" && { echo "mutant-lang2: the mutation changed nothing" >&2; exit 2; }
(cd "$OUT/tree" && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F") > "$OUT/build.log" 2>&1 \
  || { tail -5 "$OUT/build.log"; echo "mutant-lang2: the mutant did not build" >&2; exit 2; }
export FIB_LIB=$OUT/tree/lib
rc=0
for kind in stdlib own; do
  nums=${!kind}; [ -n "$nums" ] || continue
  dir=cases/stdlib; [ $kind = own ] && dir=cases/ownership
  only=(); for n in $nums; do only+=("$n-"); done
  all=$(cd "$OUT/tree" && "$OUT/F" cases "$dir" --only "${only[@]}" 2>&1) || true
  for n in $nums; do
    line=$(printf '%s\n' "$all" | grep -E "^$n-" | head -1)
    case $line in
      *" pass"*) echo "SURVIVED  $MODE: $line"; rc=1 ;;
      *) echo "killed    $MODE: ${line:0:150}" ;;
    esac
  done
done
exit $rc
