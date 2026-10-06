#!/bin/bash
# build-all.sh MACHINE CPU: every binary of the AVX-512 round for one machine into bin-MACHINE/ (static, FIB_TARGET_CPU=CPU, lane widths 256 and 512)
S=/home/tmarsh/.cache/fibber-scratch/avx512
T=/tank/repos/tailoredshapes/fibber/.claude/worktrees/agent-aaea0e0e4c75e9c11
m=$1; cpu=$2
cd $T
out=$S/bin-$m; rm -rf $out; mkdir -p $out
B=$S/bld.sh
one() { # NAME SRC BITS [-I..]
  local name=$1 src=$2 bits=$3; shift 3
  $B $m-$name $src $cpu $bits "$@" > $S/obj/$m-$name.say 2>&1 && mv $S/bin/$m-$name $out/$name && echo "built $name" || { echo "FAILED $name: $(head -c 300 $S/obj/$m-$name.say)"; }
}
for w in 256 512; do
  one tb-$w scripts/bench/tensor/bench.fib $w -I scripts/bench/tensor
  one mlp-$w scripts/bench/autodiff/mlp.fib $w
  one dot-simd-$w scripts/shootout/dot-product/dot-product-simd.fib $w
  one saxpy-simd-$w scripts/shootout/saxpy/saxpy-simd.fib $w
  one matmul-simd-$w scripts/shootout/matmul/matmul-simd.fib $w
  one spectral-simd-$w scripts/shootout/spectral-norm/spectral-norm-simd.fib $w
done
one nbody-simd scripts/shootout/n-body/n-body-simd.fib 256
one mandel-simd scripts/shootout/mandelbrot/mandelbrot-simd.fib 256
# the scalar twins: what LLVM's auto-vectoriser does with the target's default width and with -prefer-256-bit
for v in "" p512; do
  if [ -n "$v" ]; then export P512=1; else unset P512; fi
  for pair in saxpy:saxpy/saxpy dot:dot-product/dot-product matmul:matmul/matmul spectral:spectral-norm/spectral-norm; do
    n=${pair%%:*}; f=${pair##*:}
    one $n-fib${v:+-$v} scripts/shootout/$f.fib 256
  done
done
unset P512
# the cases, built as programs that print main's value: 0 is a pass
export CASEKIND=1
for f in $(cd cases/stdlib && grep -l "^;; expect: accept" 70[0-1][0-9]-*.fib 707[0-9]-*.fib 708[0-9]-*.fib 774[0-3]-*.fib 7960-*.fib 7961-*.fib 7962-*.fib 2>/dev/null | sort -u); do
  grep -q "^;; result: 0" cases/stdlib/$f || continue
  id=${f%%-*}
  for w in 256 512; do one case-$id-$w cases/stdlib/$f $w -I cases/stdlib/support; done
done
unset CASEKIND
ls $out | wc -l
