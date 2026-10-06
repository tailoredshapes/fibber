#!/bin/bash
# Mutation check of the fused dense layer (lib/fib/tensor/gemm-fma-f32.fib epilogue, vmath activations, linalg dense): each mutant plants one fault in
# the library, runs the cases that pin it (stdlib 7083 for the epilogue, 7084 for where, 7085 for softmax and layernorm) and passes only if at least one FAILS. The library is read when a program is built, so no stage 2
# is rebuilt. Sources are restored after every mutant and on exit; run it on a clean tree.
# usage: mutant-tensor-fused.sh F        F: a stage 2 fibc.   Exit: 0 every mutant killed, 1 one survived, 2 a plant changed nothing.   ONLY=name runs one.
set -u
F=${1:?usage: mutant-tensor-fused.sh F}
root=$(cd "$(dirname "$0")/.." && pwd)
out=$HOME/.cache/fibber-scratch/mutant-tensor-fused
mkdir -p "$out/orig"
export FIB_LIB=$root/lib
ulimit -v 16000000
files="lib/fib/tensor/gemm-fma-f32.fib lib/fib/tensor/vmath.fib lib/fib/tensor/linalg.fib lib/fib/tensor/masks.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
mutant() { # NAME FILE SED-EXPRESSION [CASE]
  local name=$1 file=$2 expr=$3 case=${4:-7083}; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  local res; res=$(cd "$root" && "$F" cases cases/stdlib --only $case -j 1 2>&1 | tail -1)
  case $res in *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;; *) echo "killed   $name: $res" ;; esac
}
G=lib/fib/tensor/gemm-fma-f32.fib
mutant edge-tile-skips-epilogue $G 's/(when (>= code 0) (finish-tile tp 16 bias code))/(when false (finish-tile tp 16 bias code))/'
mutant full-tile-skips-epilogue $G 's/(when (>= code 0) (finish-tile c ldc/(when false (finish-tile c ldc/'
mutant epilogue-every-inner-block $G 's/(if (>= (+ pp kk) k) code -1)/code/'
mutant epilogue-first-inner-block-only $G 's/(if (>= (+ pp kk) k) code -1)/(if (= pp 0) code -1)/'
mutant bias-ignores-tile-column $G 's/(ofs bias (unchecked-multiply 16 jq))/bias/g'
mutant bias-ignores-column-block $G 's/(ofs biasp jj)/biasp/'
mutant bias-dropped $G 's/(+ (load-f32x8 r) b0)/(load-f32x8 r)/'
mutant bias-second-half-dropped $G 's/(+ (load-f32x8 (ofs r 8)) b1)/(load-f32x8 (ofs r 8))/'
mutant last-bias-element-unwritten $G 's/(dotimes (j n) (array-set! &buf j/(dotimes (j (- n 1)) (array-set! \&buf j/'
mutant bias-stride-ignored $G 's/(array-get src (+ off (\* j st)))/(array-get src (+ off j))/'
mutant last-tile-row-unfinished $G 's/(dotimes (i 6)$/(dotimes (i 5)/'
mutant wrong-activation-code lib/fib/tensor/vmath.fib 's/(= name :silu) 4 (= name :gelu) 5/(= name :silu) 5 (= name :gelu) 4/'
mutant relu-not-relu lib/fib/tensor/vmath.fib 's/(= op 6) (simd\/blend (simd\/gt v (splat f32x8 0.0f32)) v (splat f32x8 0.0f32))/(= op 6) v/'
mutant dense-drops-activation lib/fib/tensor/linalg.fib 's/(dense-multiply (. x seed) x w bias (vm\/activation-code act))/(dense-multiply (. x seed) x w bias 7)/'
M=lib/fib/tensor/masks.fib
mutant where-scalar-steps-like-dense $M 's/(= (reduce + 0 (map abs (strides x))) 0) 0/(= (reduce + 0 (map abs (strides x))) 0) 1/' 7084
mutant where-ignores-slice-offset $M 's/(. condition buffer) (array-get (. condition meta) 0) cs/(. condition buffer) 0 cs/' 7084
mutant where-ignores-value-offset $M 's/(. left buffer) (array-get (. left meta) 0) ls (. right buffer)/(. left buffer) 0 ls (. right buffer)/' 7084
mutant where-treats-broadcast-as-linear $M 's/(cond (contiguous? x) 1/(cond (contiguous? x) 1 (> (count (strides x)) 1) 1/' 7084
mutant where-swaps-branches $M 's/(. left buffer) (array-get (. left meta) 0) ls (. right buffer) (array-get (. right meta) 0) rs)/(. right buffer) (array-get (. right meta) 0) rs (. left buffer) (array-get (. left meta) 0) ls)/' 7084
V=lib/fib/tensor/vmath.fib
mutant softmax-no-max-subtraction $V 's/(e: f64x4 (expv-f64 (- x m)))/(e: f64x4 (expv-f64 x))/' 7085
mutant softmax-f32-no-max-subtraction $V 's/(e: f32x8 (expv-f32 (- x m)))/(e: f32x8 (expv-f32 x))/' 7085
mutant softmax-output-base-uses-source-offset $V 's/(row-exp-f64 src (+ off b) n mx &buf b)/(row-exp-f64 src (+ off b) n mx \&buf (+ off b))/' 7085
mutant softmax-tail-skips-exp $V 's/(let ((e (lane (expv-f64 (splat f64x4 (- (array-get src (+ sb i)) mx))) 0)))/(let ((e 0.0))/' 7085
mutant softmax-f32-no-division $V 's/(simd-store! \&out (+ ob i) (fdiv e dv))\(.*\)(recur (+ i 8))/(simd-store! \&out (+ ob i) e)\1(recur (+ i 8))/' 7085
mutant layernorm-variance-uncentred $V 's/(var (fdiv (row-sum-f64 src sb n mu true) (double n)))/(var (fdiv (row-sum-f64 src sb n 0.0 true) (double n)))/' 7085
mutant layernorm-gamma-index-fixed $V 's/(g: f64x4 (simd-load gamma i))/(g: f64x4 (simd-load gamma 0))/' 7085
mutant layernorm-beta-dropped $V 's/(+ (\* (\* (- x m) iv) g) bt)/(* (* (- x m) iv) g)/' 7085
mutant layernorm-tail-beta-index $V 's/(array-get gamma i)) (array-get beta i)))/(array-get gamma i)) (array-get beta 0)))/' 7085
mutant layernorm-f32-mean-drops-tail $V 's/(mu (fdiv (row-sum-f32 src sb n 0.0f32 false) (float n)))/(mu (fdiv (row-sum-f32 src sb (- n (rem n 8)) 0.0f32 false) (float n)))/' 7085
exit $survived
