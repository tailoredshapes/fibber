#!/bin/bash
# Mutation check of the fused dense layer (lib/fib/tensor/gemm-fma-f32.fib epilogue, vmath activations, linalg dense): each mutant plants one fault in
# the library, runs the cases that pin it (stdlib 7083 for the epilogue, 7084 for where) and passes only if at least one FAILS. The library is read when a program is built, so no stage 2
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
mutant where-ignores-slice-offset $M 's/(cb (array-get (. condition meta) 0))/(cb 0)/' 7084
mutant where-ignores-value-offset $M 's/(lb (array-get (. left meta) 0))/(lb 0)/' 7084
mutant where-treats-broadcast-as-linear $M 's/(cond (contiguous? x) 1/(cond (contiguous? x) 1 (> (count (strides x)) 1) 1/' 7084
mutant where-swaps-branches $M 's/(if (array-get cx (+ cb (\* cs i))) (array-get lx (+ lb (\* ls i))) (array-get rx (+ rb (\* rs i))))/(if (array-get cx (+ cb (* cs i))) (array-get rx (+ rb (* rs i))) (array-get lx (+ lb (* ls i))))/' 7084
exit $survived
