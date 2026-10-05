#!/bin/bash
# Mutation check of the SIMD wave-4 package (P4b): like mutant-simd-lower.sh, but a mutant names the file it plants its fault in (the type
# checker's lowering, the builtin table, the emitter, the library). It builds a stage 2 with the fault, runs the cases that should notice
# (cases/stdlib/62xx) and passes only if at least one FAILS (a program the mutant rejects fails its case too).
# The sources are restored after every mutant; run it on a clean tree.
# usage: mutant-simd-p4b.sh BUILDER [OUTDIR]      ONLY="name name" runs only those mutants.
# Exit: 0 every mutant was killed, 1 one survived, 2 the plant did not apply or the build failed.
set -u
builder=${1:?usage: mutant-simd-p4b.sh BUILDER [OUTDIR]}
root=$(cd "$(dirname "$0")/.." && pwd)
out=${2:-$HOME/.cache/fibber-scratch/mutant-simd-p4b}
mkdir -p "$out/tmp"
export FIB_LIB=$root/lib TMPDIR=$out/tmp
ulimit -v 16000000
survived=0
current=
trap '[ -n "$current" ] && cp "$out/orig" "$current"' EXIT
mutant() { # NAME FILE SED-EXPRESSION CASE-PREFIXES..
  local name=$1 file=$2 expr=$3; shift 3; [ -n "${ONLY:-}" ] && case " $ONLY " in *" $name "*) ;; *) return 0 ;; esac
  local f=$root/$file; current=$f
  cp "$f" "$out/orig"
  sed -i -e "$expr" "$f"
  if cmp -s "$f" "$out/orig"; then echo "PLANT-FAILED $name: the sed expression changed nothing in $file"; exit 2; fi
  local rc=0
  (cd "$root" && "$builder" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$out/Fm") > "$out/$name.build" 2>&1 || rc=1
  if [ $rc -ne 0 ]; then echo "BUILD-FAILED $name: $(tail -2 "$out/$name.build" | tr '\n' ' ')"; exit 2; fi
  local res; res=$(cd "$root" && "$out/Fm" cases cases/stdlib --only "$@" -j 3 2>&1 | tail -1)
  cp "$out/orig" "$f"   # restored only after the cases: a mutant in lib/ is read at run time
  case $res in
    *" 0 fail"*" 0 header"*) echo "SURVIVED $name: $res"; survived=1 ;;
    *) echo "killed   $name: $res" ;;
  esac
}
# (1) a local named simd shadows the literal again
mutant local-simd compiler/types/lower/call.fib 's/^  (or (= name "simd")$/  (or (and (= name "simd") (not (lw-is-local lw name)))/' 6228-
# (1) an extern may name a vector again: the position falls to the generic message, and the case wants the vector one
mutant extern-vector compiler/types/lower/top.fib 's/an extern position cannot be a vector/extern positions are scalars/' 6289-
# (2) the fold of three or more operands starts one operand early and counts the second operand twice
mutant wrapping-fold lib/fib/core/forms.fib 's/(loop ((i 3) (acc (List \[(op s)/(loop ((i 2) (acc (List [(op s)/' 6240- 6241-
# (2) the walk descends into a fn literal, so the closure of 6242 wraps instead of trapping
mutant wrapping-into-fn lib/fib/core/forms.fib 's/(or (= s "fn") (or (= s "quote") (= s "quasiquote")))/(or (= s "quote") (= s "quasiquote"))/' 6242-
# (2) a single operand of - is not the negation
mutant wrapping-negate lib/fib/core/forms.fib 's/(and (or (= s "-") (= s "neg")) (= n 1))/(and (or (= s "-x") (= s "neg")) (= n 1))/' 6241-
# (2) the type suffix of an i32 vector is left out of its text
mutant show-suffix compiler/emit/lower/show.fib 's/((SI32) "i32")/((SI32) "")/' 6243-
# (2) the separator of the lanes is a comma
mutant show-separator compiler/emit/lower/show.fib 's/(simd-piece cx " ")/(simd-piece cx ",")/' 6243-
# (3) the bounds check of a whole-vector load compares the end with itself, so it never fires
mutant load-bounds compiler/emit/lower/simdmem.fib 's/(cmp cx "icmp sgt" (v-text hi) (v-text len) LirI1)/(cmp cx "icmp sgt" (v-text hi) (v-text hi) LirI1)/' 6247- 6249-
# (3) a store into a shared array is made in place
mutant store-shared compiler/emit/lower/simdmem.fib '/lj " " lc ")"\]))/s/(v-text ok)/"(i1 1)"/' 6246-
# (3) the tail mask is one lane too long
mutant tail-mask compiler/emit/lower/simdmem.fib 's/(cmp cx "icmp slt" (iota r)/(cmp cx "icmp sle" (iota r)/' 6245- 6251-
# (3) the active-lane check of a masked access lets the lane one past the end through
mutant masked-check compiler/emit/lower/simdmem.fib 's/(cmp cx "icmp ult" (v-text idx)/(cmp cx "icmp ule" (v-text idx)/' 6250-
# (3) reverse is the identity
mutant reverse compiler/emit/lower/simdfn.fib 's/(- (- (lanes-of t) 1) i)/i/' 6252-
# (3) shuffle takes its operands in the other order
mutant shuffle-order compiler/emit/lower/simdfn.fib 's/(v-text a) " " (v-text b) " " (mask-text idx)/(v-text b) " " (v-text a) " " (mask-text idx)/' 6252-
# (4) floor is ceil
mutant floor-ceil compiler/emit/lower/simdfn.fib 's/(some "ffloor")/(some "fceil")/' 6255-
# (4) min is the NaN-ignoring one
mutant min-nan compiler/emit/lower/simdfn.fib 's/(some (if fl "fmin" "smin"))/(some (if fl "fminnum" "smin"))/' 6256-
# (4) abs of the minimum does not trap
mutant abs-check compiler/emit/lower/simdfn.fib 's/(when (and (= name "simd\/abs") (not (float-elem? t)))/(when (and (= name "simd\/abs") (not true))/' 6257-
# (5) int to float conversion is unsigned
mutant convert-unsigned compiler/emit/lower/simdfn.fib 's/tf (some "sitofp")/tf (some "uitofp")/' 6254-
# (5) native-lanes counts the wrong width
mutant native-lanes compiler/types/lower/call.fib 's/(target-lanes (g-target (. lw g)) (scalar-bits s))/(target-lanes (g-target (. lw g)) (+ 8 (scalar-bits s)))/' 6258-
# (5) the slot of a vector def is not a vector
mutant def-vector compiler/emit/inits.fib 's/((LirVec _ _) (str-join \["(zeroinitializer " (lir-ty-text l) ")"\]))/((LirVec _ _) "(i64 0)")/' 6259-
[ $survived -eq 0 ] && echo "mutant-simd-p4b: every mutant was killed"
exit $survived
