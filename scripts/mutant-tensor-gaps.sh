#!/bin/bash
# Mutation check of the tensor-library additions (lib/fib/tensor/unary.fib, masks, axis reductions, optimiser steps): each mutant plants one fault in the
# library, runs the case that pins it and passes only if that case FAILS. The library is read when a program is built, so no stage 2 is rebuilt.
# Sources are restored after every mutant and on exit; run it on a clean tree.
# usage: mutant-tensor-gaps.sh F        F: a stage 2 fibc.   Exit: 0 every mutant killed, 1 one survived, 2 a plant changed nothing.   ONLY=name runs one.
set -u
F=${1:?usage: mutant-tensor-gaps.sh F}
root=$(cd "$(dirname "$0")/.." && pwd)
out=$HOME/.cache/fibber-scratch/mutant-tensor-gaps
mkdir -p "$out/orig"
export FIB_LIB=$root/lib
ulimit -v 16000000
files="lib/fib/tensor/unary.fib lib/fib/tensor/select.fib lib/fib/tensor/masks.fib lib/fib/tensor/axis-lanes.fib lib/fib/tensor/reduce-extra.fib lib/fib/tensor/optim.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
mutant() { # NAME FILE SED-EXPRESSION CASE
  local name=$1 file=$2 expr=$3 case=$4; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  local out2 res row; out2=$(cd "$root" && "$F" cases cases/stdlib --only $case -j 1 2>&1)
  res=$(echo "$out2" | tail -1); row=$(echo "$out2" | grep -E '^[0-9]+-' | sed 's/^[^ ]* *//' | cut -c1-110)
  case $res in
    *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;;
    *) case $row in *rejected*) echo "ERROR    $name: the mutant does not compile: $row"; survived=1 ;; *) echo "killed   $name: $row" ;; esac ;;
  esac
}
U=lib/fib/tensor/unary.fib
mutant sqrt-is-rsqrt $U '/defun apply-f64/,/defun apply-f32/s/(= op 0) (simd\/sqrt x)/(= op 0) (fdiv (splat f64xn 1.0) (simd\/sqrt x))/' 7740
mutant sqrt-is-identity $U 's/(= op 0) (simd\/sqrt x)$/(= op 0) x/' 7740
mutant abs-keeps-sign $U 's/(= op 2) (simd\/abs x)/(= op 2) x/' 7740
mutant sign-of-zero-is-one $U 's/(simd\/gt x (splat f64xn 0.0))/(simd\/ge x (splat f64xn 0.0))/' 7740
mutant sign-of-zero-is-zero $U 's/(splat f64xn -1.0) x))))/(splat f64xn -1.0) (splat f64xn 0.0)))))/' 7740
mutant f32-tail-not-computed $U 's/(simd-store-tail! &buf i (apply-f32 op v))/(simd-store-tail! \&buf i v)/' 7740
mutant f64-tail-not-computed $U 's/(simd-store-tail! &buf i (apply-f64 op v))/(simd-store-tail! \&buf i v)/' 7740
mutant f64-view-not-made-contiguous $U '/defun run-f64/,/defun run-f32/s/(d (contiguous x))/(d x)/' 7740
mutant f32-offset-ignored $U '/defun run-f32/,/defprotocol/s/(simd-load src (+ off i))/(simd-load src i)/' 7740
S=lib/fib/tensor/select.fib
M=lib/fib/tensor/masks.fib
mutant cmp-less-is-less-equal $S 's/(= op 1) (simd\/lt x y)/(= op 1) (simd\/le x y)/' 7741
mutant cmp-tail-lane-dropped $S '/defun compare-f32/,/defun select-f32/s/(dotimes (j cnt)/(dotimes (j (- cnt 1))/' 7741
mutant cmp-scalar-operand-read-as-dense $S 's/(= step 0) (splat f32x8 (array-get a off))/(= step 0) (simd-load-tail a off)/' 7741
mutant select-swaps-branches $S 's/(simd-store! &out i (simd\/blend m x y))/(simd-store! \&out i (simd\/blend m y x))/' 7741
mutant select-tail-swaps-branches $S 's/(if (array-get c (+ co (\* cs i))) (array-get a (+ ao (\* as i))) (array-get b (+ bo (\* bs i))))/(if (array-get c (+ co (* cs i))) (array-get b (+ bo (* bs i))) (array-get a (+ ao (* as i))))/' 7741
mutant select-mask-offset-ignored $S 's/(bytes-f32 c (+ co (\* cs i)) cs)/(bytes-f32 c (* cs i) cs)/' 7741
mutant select-mask-lane-3-wrong $S '/defun bytes-f64/,/defun compare-f64/s/(array-get c (+ base (\* cs 3)))/(array-get c (+ base (* cs 2)))/' 7741
mutant relu-grad-f32-passes-at-zero $S 's/(r: f32x8 (simd\/blend (simd\/gt xv (splat f32x8 0.0f32)) gv/(r: f32x8 (simd\/blend (simd\/ge xv (splat f32x8 0.0f32)) gv/' 7741
mutant relu-grad-f64-multiplies-by-mask $S 's/(r: f64x4 (simd\/blend (simd\/gt xv (splat f64x4 0.0)) gv (splat f64x4 0.0)))/(r: f64x4 (* gv (simd\/blend (simd\/gt xv (splat f64x4 0.0)) (splat f64x4 1.0) (splat f64x4 0.0))))/' 7741
mutant relu-mask-f32-passes-at-zero $S '/defun relu-mask-f32/,/defun compare-f64/s/(simd\/gt xv/(simd\/ge xv/' 7741
mutant compare-ignores-right-step $M 's/(. right buffer) (array-get (. right meta) 0) rs)/(. right buffer) (array-get (. right meta) 0) 1)/' 7741
mutant equal-is-less-equal $M 's/(compare-masks 0 (fn/(compare-masks 2 (fn/' 7741
mutant relu-grad-reads-view-in-buffer-order $M 's/(gc (contiguous (broadcast-to dims g)))/(gc (broadcast-to dims g))/' 7741
mutant where-ignores-slice-offset-of-values $M 's/(. left buffer) (array-get (. left meta) 0) ls/(. left buffer) 0 ls/' 7741
A=lib/fib/tensor/axis-lanes.fib
mutant axis-sum-skips-first-row $A '/defun block4-f32/,/defun block1-f32/s/(j0 (if (= op 0) 0 1))/(j0 1)/' 7742
mutant axis-sum-starts-from-negative-zero $A '/defun block4-f64/,/defun block1-f64/s/(z: f64xn (splat f64xn 0.0))/(z: f64xn (splat f64xn -0.0))/' 7742
mutant axis-max-uses-simd-max $A '/defun lop-f64/,/defun sop-f64/s/(= op 2) (simd\/blend (simd\/or (simd\/ne a a) (simd\/gt a b)) a b)/(= op 2) (simd\/max a b)/' 7742
mutant axis-f32-max-uses-simd-max $A '/defun lop-f32/,/defun sop-f32/s/(= op 2) (simd\/blend (simd\/or (simd\/ne a a) (simd\/gt a b)) a b)/(= op 2) (simd\/max a b)/' 7742
mutant axis-min-tail-takes-max $A '/defun sop-f32/,/defun block4-f32/s/:else (min a b)/:else (max a b)/' 7742
mutant axis-tail-column-dropped $A '/defun reduce-f32/,/defprotocol/s/(when (< c inner) (do (array-set!/(when (< c (- inner 1)) (do (array-set!/' 7742
mutant axis-block-third-vector-misplaced $A 's/(lop-f32 op p2 (simd-load src (+ r (+ c (\* 2 (native-lanes f32))))))/(lop-f32 op p2 (simd-load src (+ r (+ c (native-lanes f32)))))/' 7742
mutant axis-slice-offset-ignored $A '/defun reduce-f64/,/defprotocol/s/(base (+ off (\* (\* o extent) inner)))/(base (* (* o extent) inner))/' 7742
mutant axis-max-nan-accumulator-replaced $A '/defun lop-f32/,/defun sop-f32/s/(simd\/or (simd\/ne a a) (simd\/gt a b))/(simd\/gt a b)/' 7742
mutant axis-view-treated-as-dense $A 's/(and (axis-lanes? (. x seed)) (contiguous? x) /(and (axis-lanes? (. x seed)) /' 7742
mutant axis-keepdims-ignored $A 's/(if keepdims (conj out 1) out)/out/' 7742
mutant axis-maximum-runs-minimum lib/fib/tensor/reduce-extra.fib 's/(lane-axis 2 axis keepdims x)/(lane-axis 3 axis keepdims x)/' 7742
O=lib/fib/tensor/optim.fib
mutant adam-eps-inside-the-sqrt $O 's/(den: f64xn (+ (\* (simd\/sqrt v2) rsv) epsv))/(den: f64xn (* (simd\/sqrt (+ v2 epsv)) rsv))/' 7743
mutant adam-second-moment-without-square $O 's/(v2: f32xn (+ (\* b2v vv) (\* omb2v (\* gv gv))))/(v2: f32xn (+ (* b2v vv) (* omb2v gv)))/' 7743
mutant adam-update-fused-multiply-add $O 's/(p2: f64xn (+ pv (\* nlrv (fdiv m2 den))))/(p2: f64xn (simd\/fma nlrv (fdiv m2 den) pv))/' 7743
mutant adam-tail-keeps-old-second-moment $O 's/(array-set! &vn i v2)/(array-set! \&vn i vx)/' 7743
mutant adam-parameter-offset-ignored $O 's/(. pd buffer) (array-get (. pd meta) 0) (. gd buffer)/(. pd buffer) 0 (. gd buffer)/' 7743
mutant adam-first-moment-offset-ignored $O 's/(. md buffer) (array-get (. md meta) 0)/(. md buffer) 0/' 7743
mutant adam-returns-second-moment-as-first $O 's/(from-array dims @mn 0.0)/(from-array dims @vn 0.0)/' 7743
mutant adam-no-bias-correction-of-step-size $O 's/(@CONV@ (- 0.0 (\/ lr bc1)))//; s/(fptrunc f32 (- 0.0 (\/ lr bc1)))/(fptrunc f32 (- 0.0 lr))/' 7743
mutant adam-reciprocal-sqrt-dropped $O 's/(double (\/ 1.0 (simd\/sqrt bc2)))/(double (simd\/sqrt bc2))/' 7743
mutant momentum-ignores-mu $O 's/(b2: f32xn (+ (\* muv bv) gv))/(b2: f32xn (+ bv gv))/' 7743
mutant momentum-tail-subtracts $O 's/(+ (array-get p (+ po i)) (\* nlr b2))/(- (array-get p (+ po i)) (* nlr b2))/' 7743
exit $survived
