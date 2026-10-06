#!/bin/bash
# Mutation check of the lane-generic tensor kernels (the AVX-512 wave, docs/design/avx512.md): each mutant plants one fault in the library, runs the case that pins it
# (at the default width, or at 512 bits with FIB_VECTOR_BITS=512 for a fault only the wide shape has) and passes only if that case FAILS. The library is read when a
# program is built, so no stage 2 is rebuilt. Sources are restored after every mutant and on exit; run it on a clean tree.
# usage: mutant-avx512.sh F        F: a stage 2 fibc.   Exit: 0 every mutant killed, 1 one survived, 2 a plant changed nothing.   ONLY=name runs one.
set -u
F=${1:?usage: mutant-avx512.sh F}
root=$(cd "$(dirname "$0")/.." && pwd)
out=$HOME/.cache/fibber-scratch/mutant-avx512
mkdir -p "$out/orig"
export FIB_LIB=$root/lib
ulimit -v 16000000
files="lib/fib/tensor/gemm-fma-f64.fib lib/fib/tensor/gemm-fma-f32.fib lib/fib/tensor/simd-f64.fib lib/fib/tensor/axis-lanes.fib lib/fib/tensor/vmath.fib lib/fib/tensor/unary.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
mutant() { # NAME FILE SED-EXPRESSION CASE [BITS]
  local name=$1 file=$2 expr=$3 case=$4 bits=${5:-}; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  local out2 res row
  out2=$(cd "$root" && FIB_VECTOR_BITS=$bits "$F" cases cases/stdlib --only $case -j 4 2>&1)
  res=$(echo "$out2" | tail -1); row=$(echo "$out2" | grep -E '^[0-9]+-' | grep -E ' (FAIL|rejected)' | head -1 | sed 's/^[^ ]* *//' | cut -c1-110)
  case $res in
    *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;;
    *) case $row in *rejected*) echo "ERROR    $name: the mutant does not compile: $row"; survived=1 ;; *) echo "killed   $name: ${bits:+[$bits bits] }$row" ;; esac ;;
  esac
}
G=lib/fib/tensor/gemm-fma-f64.fib
H=lib/fib/tensor/gemm-fma-f32.fib
# a tile that reorders an accumulation: the wide tile's last accumulator takes its neighbour's chain
mutant f64-wide-tile-shares-an-accumulator $G 's/(simd\/fma x5 b3 a53)/(simd\/fma x5 b3 a52)/' 7960
mutant f64-narrow-tile-swaps-two-products $G 's/(simd\/fma x0 b1 a01) (simd\/fma x1 b0 a10)/(simd\/fma x1 b1 a01) (simd\/fma x0 b0 a10)/' 7960
# a block that restarts the chain instead of continuing it from C
mutant f64-block-restarts-the-chain $G 's/(> pp 0) tp w)/false tp w)/' 7960
mutant f32-block-restarts-the-chain $H 's/(> pp 0) tp (ofs biasp jj)/false tp (ofs biasp jj)/' 7961
# wrong tail handling: the scratch tile copies one column too few; a packed panel is one vector short
mutant f64-edge-tile-drops-a-column $G 's/(dotimes (j nc) (store-f64 (ofs cp/(dotimes (j (- nc 1)) (store-f64 (ofs cp/' 7960
mutant f64-packed-panel-one-vector-short $G 's/(dotimes (v (quot q l))/(dotimes (v (- (quot q l) 1))/' 7960
mutant f32-packed-panel-one-vector-short $H 's/(dotimes (v (quot q l))/(dotimes (v (- (quot q l) 1))/' 7961
mutant f64-edge-scratch-pitch-wrong $G 's/(kern ap bp tp q kk load w)/(kern ap bp tp (+ q 8) kk load w)/' 7960
# the epilogue: bias of the wrong tile, activation applied once per inner block
mutant f32-epilogue-bias-of-the-first-tile $H 's/(finish-tile c ldc (ofs bias (unchecked-multiply q jq)) code q)/(finish-tile c ldc bias code q)/' 7961
mutant f32-epilogue-on-every-inner-block $H 's/(if (>= (+ pp kk) k) code -1) w)/code w)/' 7961
mutant f32-bias-row-width-wrong $H 's/(finish-tile tp q bias code q)/(finish-tile tp q bias code 8)/' 7961
# lane counts that must follow the width: found only by the wide shape (512 bits), the default width computes the same
mutant sum-fast-packed-edge-counts-four-lanes lib/fib/tensor/simd-f64.fib '0,/(packed (- n (rem n (native-lanes f64))))/s//(packed (- n (rem n 4)))/' 7013 512
mutant axis-column-block-counts-four-lanes lib/fib/tensor/axis-lanes.fib 's/(n1 (- inner (rem inner (native-lanes f64))))/(n1 (- inner (rem inner 4)))/' 7742 512
mutant axis-unrolled-block-counts-sixteen-columns lib/fib/tensor/axis-lanes.fib 's/(n4 (- inner (rem inner (\* 4 (native-lanes f64)))))/(n4 (- inner (rem inner 16)))/' 7742 512
mutant vmath-whole-lift-skips-the-function lib/fib/tensor/vmath.fib 's/(simd\/convert (apply-lanes op w)))) r))/(simd\/convert w))) r))/' 7080 512
mutant unary-f64-tail-not-computed lib/fib/tensor/unary.fib 's/(simd-store-tail! &buf i (apply-f64 op v))/(simd-store-tail! \&buf i v)/' 7740 512
restore
exit $survived
