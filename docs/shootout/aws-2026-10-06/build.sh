#!/bin/bash
# Builds the static fibber benchmark binaries of one architecture into bundle-ARCH/bin.
#   build.sh x86|arm TREE
# x86: FIB_TARGET_CPU=sapphirerapids, F emit -> L build --emit obj -> gcc -static
# arm: FIB_TARGET_CPU=neoverse-v2, --target aarch64-unknown-linux-gnu, aarch64-linux-gnu-gcc -static
set -u
arch=$1; tree=$2; cpu=${3:-}; sfx=${4:-}
S=$HOME/.cache/fibber-scratch/aws
F=$S/F; L=$S/L
export FIB_LIB=$tree/lib
export LD_LIBRARY_PATH=/usr/lib/llvm-21/lib
# FIB_EMIT_EXE: emit the executable kind of module (what `build` lowers), not the case kind that prints main's value
export FIB_EMIT_EXE=1
ulimit -v 16000000
out=$S/bundle-$arch/bin$sfx; mkdir -p "$out" "$S/obj-$arch$sfx"
case $arch in
  x86) export FIB_TARGET_CPU=sapphirerapids; tgt=(); cc=gcc ;;
  arm) export FIB_TARGET_CPU=neoverse-v2; tgt=(--target aarch64-unknown-linux-gnu); cc=aarch64-linux-gnu-gcc ;;
esac
[ -n "$cpu" ] && export FIB_TARGET_CPU=$cpu
echo "FIB_TARGET_CPU=$FIB_TARGET_CPU"
# one NAME SRC [-I DIR]..
one() {
  local name=$1 src=$2; shift 2
  local o=$S/obj-$arch$sfx/$name
  # arm: F emit runs without FIB_TARGET_CPU (with it, the macro runner's host JIT is asked for the AArch64 CPU and LLVM aborts:
  # "64-bit code requested on a subtarget that doesn't support it"); the module's target form is then set to the CPU for lairf
  if if [ "$arch" = arm ]; then env -u FIB_TARGET_CPU "$F" emit "${tgt[@]}" -I "$tree/lib" "$@" "$src" | sed "1s/^(target (cpu \"[^\"]*\"))/(target (cpu \"$FIB_TARGET_CPU\"))/"; else "$F" emit "${tgt[@]}" -I "$tree/lib" "$@" "$src"; fi > "$o.lir" 2> "$o.err" && [ -s "$o.lir" ] &&
     "$L" build "$o.lir" "${tgt[@]}" -o "$o.o" -O 2 --emit obj 2>> "$o.err" &&
     $cc -static "$o.o" -o "$out/$name" -lm -lpthread 2>> "$o.err"; then
    echo "built $arch $name"
  else echo "FAILED $arch $name: $(head -c 400 "$o.err")"; fi
}
sh=$tree/scripts/shootout
for d in binary-trees fannkuch-redux fasta k-nucleotide mandelbrot n-body pidigits regex-redux reverse-complement spectral-norm dot-product saxpy matmul; do
  s=$sh/$d/$d.fib; [ -f "$s" ] || s=$sh/$d/${d//-/}.fib
  one "$d-fib" "$s"
  [ -f "$sh/$d/$d-simd.fib" ] && one "$d-simd" "$sh/$d/$d-simd.fib"
done
one tensor-bench "$tree/scripts/bench/tensor/bench.fib" -I "$tree/scripts/bench/tensor"
one mlp "$tree/scripts/bench/autodiff/mlp.fib"
one parallel-closure "$tree/scripts/bench/parallel-closure.fib" -I "$tree/compiler"
sed -e 's/r28 (psum a 28)/r32 (psum a 32)/' -e 's/ 28:" (ms t6 t7)/ 32:" (ms t6 t7)/' -e 's/r16 r28 (= r1/r16 r32 (= r1/' -e 's/(= r16 r28)/(= r16 r32)/' \
    -e 's/(Par 28 0 false)/(Par 32 0 false)/' -e 's/workers 28:/workers 32:/' "$tree/docs/shootout/parallel/preduce-1e8.fib" > "$S/preduce-1e8-w32.fib"
one preduce-1e8 "$S/preduce-1e8-w32.fib"
