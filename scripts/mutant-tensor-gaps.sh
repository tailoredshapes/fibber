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
files="lib/fib/tensor/unary.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
mutant() { # NAME FILE SED-EXPRESSION CASE
  local name=$1 file=$2 expr=$3 case=$4; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  local res; res=$(cd "$root" && "$F" cases cases/stdlib --only $case -j 1 2>&1 | tail -1)
  case $res in *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;; *) echo "killed   $name: $res" ;; esac
}
U=lib/fib/tensor/unary.fib
mutant sqrt-is-rsqrt $U 's/(= op 0) (simd\/sqrt x)/(= op 0) (fdiv (splat f64x4 1.0) (simd\/sqrt x))/' 7740
mutant sqrt-is-identity $U 's/(= op 0) (simd\/sqrt x)$/(= op 0) x/' 7740
mutant abs-keeps-sign $U 's/(= op 2) (simd\/abs x)/(= op 2) x/' 7740
mutant sign-of-zero-is-one $U 's/(simd\/gt x (splat f64x4 0.0))/(simd\/ge x (splat f64x4 0.0))/' 7740
mutant sign-of-zero-is-zero $U 's/(splat f64x4 -1.0) x))))/(splat f64x4 -1.0) (splat f64x4 0.0)))))/' 7740
mutant f32-tail-not-computed $U 's/(simd-store-tail! &buf i (apply-f32 op v))/(simd-store-tail! \&buf i v)/' 7740
mutant f64-tail-not-computed $U 's/(simd-store-tail! &buf i (apply-f64 op v))/(simd-store-tail! \&buf i v)/' 7740
mutant f64-view-not-made-contiguous $U '/defun run-f64/,/defun run-f32/s/(d (contiguous x))/(d x)/' 7740
mutant f32-offset-ignored $U '/defun run-f32/,/defprotocol/s/(simd-load src (+ off i))/(simd-load src i)/' 7740
exit $survived
