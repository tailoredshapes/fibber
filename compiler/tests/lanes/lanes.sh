#!/bin/bash
# The lane-generic tensor kernels at 256 and 512 bits (docs/design/avx512.md). No AVX-512 hardware is needed: FIB_VECTOR_BITS=512 makes `native-lanes`,
# `f64xn` and `:native` eight lanes (sixteen for f32) on any target, and LLVM splits the vectors into the registers the CPU has, so the 512-bit shape of
# every kernel (tile, tails, lane counts) runs here. (qemu-user's TCG has no AVX-512, so a zmm run is possible only on hardware: docs/shootout/avx512-2026-10-06.md.)
#   1. the tensor cases pass at 512 bits (the suite's own oracles, 7000 to 7099 and 7740 to 7743, 7960 to 7979);
#   2. bench.fib kernels at a small size print the same checksum at 256 and 512 bits: bit for bit for every kernel that is ordered or lane-wise; the kernels
#      whose sums are reassociated by the lane count (sum-fast, f64 softmax and layernorm) may differ and are listed as such, never silently;
#   3. the lane-generic shootout programs print the md5 of their scalar twin at both widths;
#   4. a program built for x86-64-v4 at 512 bits has the 6x32 micro-kernel in zmm (24 fused multiply-adds in the inner loop), one for x86-64-v3 has no zmm.
# usage: lanes.sh F       F: a stage 2 fibc.  Exit 0 all hold, 1 one failed.
set -u
F=${1:?usage: lanes.sh F}
root=$(cd "$(dirname "$0")/../../.." && pwd)
out=${LANES_OUT:-$HOME/.cache/fibber-scratch/lanes}
mkdir -p "$out"
export FIB_LIB=$root/lib
ulimit -v 16000000
cd "$root" || exit 1
bad=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; bad=1; }

# 1. the cases
res=$(FIB_VECTOR_BITS=512 "$F" cases cases/stdlib --only 700 701 702 703 704 705 706 707 708 709 774 796 -j 6 2>&1 | tail -1)
case $res in *" 0 fail, 0 pending, 0 header error"*) ok "tensor cases at 512 bits: $res" ;; *) no "tensor cases at 512 bits: $res" ;; esac

# 2. checksums of the benchmark kernels
build() { # name cpu bits src [-I..]
  local name=$1 cpu=$2 bits=$3 src=$4; shift 4
  FIB_TARGET_CPU=$cpu FIB_VECTOR_BITS=$bits "$F" build "$src" -I "$root/lib" "$@" -o "$out/$name" > "$out/$name.log" 2>&1 || { no "build $name: $(head -c 300 "$out/$name.log")"; return 1; }
}
if build tb-256 x86-64-v3 256 scripts/bench/tensor/bench.fib -I scripts/bench/tensor && build tb-512 x86-64-v3 512 scripts/bench/tensor/bench.fib -I scripts/bench/tensor; then
  exact="matmul64:128 matmul32:128 fma64:100000 fma32:100000 sum:100000 max:100000 sum0:256 sum1:256 max0:256 max1:256 bcast:256 expvec:100000 logvec:100000 tanhvec:100000 expvec32:100000 mlp:32 mlpfused:32 attn:128 attnfused:128"
  loose="sumfast:100000 mean:100000 softmax:256 softmaxfused:256 layernorm:256 layernormfused:256"
  for kv in $exact $loose; do
    k=${kv%%:*}; n=${kv##*:}
    a=$("$out/tb-256" "$k" "$n" 2 | grep '^cs'); b=$("$out/tb-512" "$k" "$n" 2 | grep '^cs')
    if [ -z "$a" ] || [ -z "$b" ]; then no "bench $k: no checksum ($a / $b)"
    elif [ "$a" = "$b" ]; then ok "bench $k: the same checksum at both widths: $a"
    elif [[ " $exact " == *" $kv "* ]]; then no "bench $k: 256 bits $a, 512 bits $b (must be bit-identical)"
    else echo "info $k: differs as its reassociated sums do: 256 bits $a, 512 bits $b"; fi
  done
fi

# 3. the shootout programs: the md5 of the scalar twin at both widths
chk() { # name dir file size md5
  for bits in 256 512; do
    build "sc-$1-$bits" x86-64-v3 $bits "scripts/shootout/$2/$3.fib" || continue
    got=$("$out/sc-$1-$bits" "$4" | md5sum | cut -c1-32)
    [ "$got" = "$5" ] && ok "shootout $1 at $bits bits: $got" || no "shootout $1 at $bits bits: $got, want $5"
  done
}
chk dot dot-product dot-product-simd 1000000 51f909a149c8798fa1223d3abe66dc07
chk saxpy saxpy saxpy-simd 1000000 8cff1bd0e08970a714e2d8ba4c5cead1
chk matmul matmul matmul-simd 256 a89463dd45295a657ee2921b3358a01b
chk spectral spectral-norm spectral-norm-simd 1500 21474fad468928914234d3644f18a964

# 4. the machine code
if command -v objdump > /dev/null; then
  cat > "$out/gemm.fib" <<'EOF'
(ns main (:require [fib.tensor :as t]))
(defun main () -> i64
  (let ((a (t/full [64 64] 1.5)) (b (t/full [64 64] 2.0)))
    (do (println (t/sum (t/mmul a b))) 0)))
EOF
  # building for a CPU the host lacks is allowed (the program traps at start there); only the code is looked at
  if build gemm-v4 x86-64-v4 512 "$out/gemm.fib" && build gemm-v3 x86-64-v3 256 "$out/gemm.fib"; then
    z4=$(objdump -d "$out/gemm-v4" | grep -c 'vfmadd231pd.*zmm'); z3=$(objdump -d "$out/gemm-v3" | grep -c 'zmm')
    [ "$z4" -ge 24 ] && ok "x86-64-v4 at 512 bits: $z4 zmm fused multiply-adds (the 6x32 tile has 24 in its loop)" || no "x86-64-v4 at 512 bits: only $z4 zmm fused multiply-adds"
    [ "$z3" -eq 0 ] && ok "x86-64-v3 at 256 bits: no zmm instruction" || no "x86-64-v3 at 256 bits: $z3 zmm instructions"
  fi
else echo "skip machine code: no objdump"; fi
exit $bad
