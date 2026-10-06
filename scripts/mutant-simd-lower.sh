#!/bin/bash
# Mutation check of the lane-vector lowering (compiler/emit/lower/simd.fib): each mutant plants one fault in the emitter, builds a stage 2 with it,
# runs the cases of the vector block that should notice (cases/stdlib/62xx), and passes only if at least one of them FAILS. A mutant that survives
# means a case is missing. The sources are restored after every mutant (and on exit), so the tree is clean afterwards; run it on a clean tree.
# usage: mutant-simd-lower.sh BUILDER [OUTDIR]      BUILDER: a fibc that builds compiler/fibc.fib (a seed or a stage 2)
# Exit: 0 every mutant was killed, 1 one survived, 2 the plant did not apply or the build failed.
set -u
builder=${1:?usage: mutant-simd-lower.sh BUILDER [OUTDIR]}
root=$(cd "$(dirname "$0")/.." && pwd)
out=${2:-$HOME/.cache/fibber-scratch/mutant-simd}
mkdir -p "$out/tmp"
export FIB_LIB=$root/lib TMPDIR=$out/tmp
ulimit -v 16000000
f=$root/compiler/emit/lower/simd.fib
g=$root/compiler/emit/lower/simdfn.fib
cp "$f" "$out/simd.fib.orig"
cp "$g" "$out/simdfn.fib.orig"
trap 'cp "$out/simd.fib.orig" "$f"; cp "$out/simdfn.fib.orig" "$g"' EXIT
survived=0
mutant() { # NAME SED-EXPRESSION CASE-PREFIXES..   (ONLY=name runs one)
  local name=$1 expr=$2; shift 2; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  cp "$out/simd.fib.orig" "$f"; cp "$out/simdfn.fib.orig" "$g"
  local tgt=$f orig=$out/simd.fib.orig; [ -n "${INFN:-}" ] && { tgt=$g; orig=$out/simdfn.fib.orig; }
  sed -i -e "$expr" "$tgt"
  if cmp -s "$tgt" "$orig"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  (cd "$root" && "$builder" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$out/Fm") > "$out/$name.build" 2>&1 \
    || { echo "BUILD-FAILED $name: $(tail -2 "$out/$name.build" | tr '\n' ' ')"; exit 2; }
  local res; res=$(cd "$root" && "$out/Fm" cases cases/stdlib --only "$@" -j 3 2>&1 | tail -1)
  case $res in
    *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;;
    *) echo "killed   $name: $res" ;;
  esac
}
# wrong lane order: the literal puts lane i at index n-1-i
mutant lane-order 's/(v-text x) " (i32 " (str i) "))"\])/(v-text x) " (i32 " (str (- (count es) (+ i 1))) "))"])/' 6200- 6204- 6213- 6219-
# unchecked overflow: the or of the overflow flags is never true
mutant unchecked-overflow 's/(str-join \["(reduce-or " (v-text flags) ")"\])/(str-join ["(and (reduce-or " (v-text flags) ") (i1 0))"])/' 6230- 6231- 6232- 6233-
mutant unchecked-neg 's/(str-join \["(reduce-or " (v-text amin) ")"\])/(str-join ["(and (reduce-or " (v-text amin) ") (i1 0))"])/' 6234-
mutant unchecked-hsum 's/(let ((ovf (val cx (str-join \["(extractvalue " (v-text r) " 1)"\]) LirI1)))/(let ((ovf (val cx "(and (i1 0) (i1 0))" LirI1)))/' 6235-
# wrong reduce: the minimum of integer lanes is the maximum
mutant wrong-reduce 's/"reduce-smin "/"reduce-smax "/' 6214- 6291-
# wrong tree: the upper half of the halving tree is the lower half
mutant wrong-tree 's/(mask-text (vec (range h k)))/(mask-text (vec (range h)))/' 6215- 6291-
# mask polarity: `lt` compares with greater-than
mutant mask-polarity 's/(if fl "olt" "slt")/(if fl "ogt" "sgt")/' 6210- 6211- 6212-
# mask not: the complement does not flip
mutant mask-not 's/(= name "simd\/not") (val cx (str-join \["(xor " a " " (vconst t "1") ")"\]) t)/(= name "simd\/not") (val cx (str-join ["(xor " a " " (vconst t "0") ")"]) t)/' 6212-
# blend takes its lanes from the wrong side
mutant blend-order 's/" " (v-text p) " " (v-text q) ")"\]) (unwrap-or (v-ty p) t))/" " (v-text q) " " (v-text p) ")"]) (unwrap-or (v-ty p) t))/' 6212-
# the shift count mask drops its low bit (an unmasked count is poison in lIR, whose folding can agree by luck, so it is not the plant)
mutant shift-mask 's/(vconst t (str (- (lir-ty-bits (elem-of t)) 1)))/(vconst t (str (- (lir-ty-bits (elem-of t)) 2)))/' 6206- 6207- 6208-
# the dynamic lane index is not checked
mutant lane-bounds 's/(icmp uge " (v-text i) " (i64 " (str n) "))"/(icmp uge " (v-text i) " (i64 99999))"/' 6236- 6237-
# JSON-2 (emit.lower.simdfn): movemask puts the lanes in the wrong order (the mask is reversed before the bitcast); a mask of an odd lane count is padded with true lanes;
# the lanes are not zero-extended but sign-extended; ctz and clz are swapped
INFN=1 mutant movemask-order 's/(let ((b (val cx (str-join \["(bitcast " (lir-ty-text wt) " " (v-text m) ")"\]) wt)))/(let ((b (val cx (str-join ["(bitcast " (lir-ty-text wt) " (shufflevector " (v-text m) " " (v-text m) " " (mask-text (mapv (fn (i: i64) (- (- w 1) i)) (range w))) "))"]) wt)))/' 7980-
INFN=1 mutant movemask-pad 's/(mask-text (mapv (fn (i: i64) (if (< i n) i n)) (range w)))/(mask-text (mapv (fn (i: i64) (if (< i n) i 0)) (range w)))/' 7980-
INFN=1 mutant movemask-sext 's/(val cx (str-join \["(zext i64 " (v-text b) ")"\]) LirI64)/(val cx (str-join ["(sext i64 " (v-text b) ")"]) LirI64)/' 7980-
INFN=1 mutant ctz-clz-swapped 's/(if (= name "ctz") "cttz" "ctlz")/(if (= name "ctz") "ctlz" "cttz")/' 7980-
[ $survived -eq 0 ] && echo "mutant-simd-lower: every mutant was killed"
exit $survived
