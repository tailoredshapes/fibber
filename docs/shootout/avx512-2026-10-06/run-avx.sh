#!/bin/bash
# The AVX-512 round on one cloud instance (docs/shootout/avx512-2026-10-06.md). Runs from the unpacked bundle:
#   bin/ (static fibber binaries built elsewhere: tb-256 tb-512 mlp-* *-simd-* *-fib case-*), tree/scripts/{shootout,bench/tensor,bench/autodiff}, this file.
#   nohup bash run-avx.sh > run.log 2>&1 &      results in ./results; ./results/DONE at the end
# Every timed program is a single thread pinned to one CPU. SMOKE=1: small sizes, one run each (a check of this script).
set -u
B=$(cd "$(dirname "$0")" && pwd); cd "$B"
R=$B/results; mkdir -p "$R"; W=/dev/shm/bench; mkdir -p "$W/out" "$W/openblas"
trip=$(uname -m)-linux-gnu
SH=$B/tree/scripts/shootout
ulimit -v unlimited
log() { echo "[$(date +%T)] $*"; }
SMOKE=${SMOKE:-}
RUNS=5; [ -n "$SMOKE" ] && RUNS=1
PIN="taskset -c 4"

# ------------------------------------------------------------------ machine facts
{ echo "== uname"; uname -a; echo "== lscpu"; lscpu; echo "== nproc"; nproc; echo "== free -g"; free -g; echo "== os"; head -2 /etc/os-release
  echo "== python"; python3 --version; echo "== numpy"; python3 -c 'import numpy; print(numpy.__version__); numpy.show_config()' 2>&1
  echo "== libblas.so.3 alternatives"; update-alternatives --display libblas.so.3-$trip 2>&1
  echo "== flags"; grep -m1 -i 'flags' /proc/cpuinfo
  echo "== instance type"; cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null
  echo "== cpufreq"; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq 2>&1
} > "$R/facts.txt" 2>&1
ln -sf "$(ls /usr/lib/$trip/openblas-pthread/libblas.so.3)" "$W/openblas/libblas.so.3"
ls -l "$W/openblas" >> "$R/facts.txt"
log facts done

# ------------------------------------------------------------------ 1. correctness on the hardware: every case, built for both widths, prints 0 for a pass
: > "$R/cases.tsv"
for f in bin/case-*; do
  out=$(timeout 600 "$f" 2>&1 | tail -1)
  printf '%s\t%s\n' "$(basename "$f")" "$out" >> "$R/cases.tsv"
done
log "cases: $(grep -c -P '\t0$' "$R/cases.tsv") of $(wc -l < "$R/cases.tsv") print 0"

# ------------------------------------------------------------------ 2. the benchmark kernels: checksum at both widths on small sizes, and the tensor table
T=$B/tree/scripts/bench/tensor
: > "$R/checksums.tsv"
for kv in matmul64:128 matmul32:128 fma64:100000 fma32:100000 sum:100000 sumfast:100000 mean:100000 max:100000 sum0:256 sum1:256 max0:256 max1:256 bcast:256 expvec:100000 logvec:100000 tanhvec:100000 expvec32:100000 softmax:256 softmaxfused:256 layernorm:256 layernormfused:256 mlp:32 mlpfused:32 attn:128 attnfused:128; do
  k=${kv%%:*}; n=${kv##*:}
  a=$(bin/tb-256 "$k" "$n" 2 | grep '^cs' | cut -d' ' -f2-); b=$(bin/tb-512 "$k" "$n" 2 | grep '^cs' | cut -d' ' -f2-)
  printf '%s\t%s\t%s\t%s\n' "$k" "$a" "$b" "$([ "$a" = "$b" ] && echo same || echo DIFF)" >> "$R/checksums.tsv"
done
log "checksums done"
S=; [ -n "$SMOKE" ] && S=--small
for v in 256 512; do
  OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 $PIN python3 "$T/run.py" --fibc true --bin "$B/bin/tb-$v" --openblas "$W/openblas" $S --tsv "$R/tensor-$v.tsv" > "$R/tensor-$v.txt" 2>&1
  log "tensor $v done"
done
for kc in 64 256; do
  [ -x bin/tb-512-kc$kc ] || continue
  $PIN python3 "$T/run.py" --fibc true --bin "$B/bin/tb-512-kc$kc" --openblas "$W/openblas" $S --only matmul --tsv "$R/tensor-512-kc$kc.tsv" > "$R/tensor-512-kc$kc.txt" 2>&1
  log "tensor 512 kc$kc done"
done

# ------------------------------------------------------------------ 3. the shootout SIMD programs and their scalar twins
printf 'bench\tvariant\truns\tstatus\telapsed_s\tall_elapsed\tmd5\n' > "$R/shootout.tsv"
timed() { # BENCH VARIANT MD5 CMD..
  local bench=$1 var=$2 md5=$3; shift 3
  local i st=ok all="" got=-
  for i in $(seq $RUNS); do
    /usr/bin/time -f '%e' -o "$W/t" $PIN "$@" > "$W/out/stdout" 2> "$W/out/stderr"
    local rc=$?
    got=$(md5sum < "$W/out/stdout" | cut -c1-32)
    if [ $rc -ne 0 ]; then st="FAIL-exit-$rc"; break; fi
    [ "$got" = "$md5" ] || { st=FAIL-md5; break; }
    all="$all $(tail -n 1 "$W/t")"
  done
  local med=-
  [ "$st" = ok ] && med=$(printf '%s\n' $all | sort -g | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$bench" "$var" "$RUNS" "$st" "$med" "${all# }" "$got" >> "$R/shootout.tsv"
  log "$bench $var $st $med"
}
if [ -n "$SMOKE" ]; then
  DOTN=1000000; DOTM=51f909a149c8798fa1223d3abe66dc07; SAXN=1000000; SAXM=8cff1bd0e08970a714e2d8ba4c5cead1; MMN=256; MMM=a89463dd45295a657ee2921b3358a01b; SPN=1500; SPM=21474fad468928914234d3644f18a964
  DOTC=""
else
  DOTN=100000000; DOTM=6a2af661e2476978717dea02afc6a15e; SAXN=100000000; SAXM=2b3a0d6020ded2d3e0062b11605c75b9; MMN=2048; MMM=4bdcfd860a1bf6e409bbb88a547ee891; SPN=5500; SPM=1584fbeab0a952f314fbf0fd7621885f
  DOTC="32768 61035"
fi
for w in 256 512; do
  timed dot-mem simd-$w $DOTM bin/dot-simd-$w $DOTN
  [ -n "$DOTC" ] && timed dot-cache simd-$w eab5a509a90ee9d0cdcdc5828211acfa bin/dot-simd-$w $DOTC
  timed saxpy-mem simd-$w $SAXM bin/saxpy-simd-$w $SAXN
  [ -n "$DOTC" ] && timed saxpy-cache simd-$w 853578cdee1b3dd4a08732371f1855ce bin/saxpy-simd-$w $DOTC
  timed matmul simd-$w $MMM bin/matmul-simd-$w $MMN
  timed spectral-norm simd-$w $SPM bin/spectral-simd-$w $SPN
done
if [ -z "$SMOKE" ]; then
  timed n-body simd 32c32132315472b32572eb1f52fc01a6 bin/nbody-simd 50000000
  timed mandelbrot simd 8c2ed8883de64eccd3154ac612021fe8 bin/mandel-simd 16000
fi
for v in "" -p512; do
  timed dot-mem scalar$v $DOTM bin/dot-fib$v $DOTN
  [ -n "$DOTC" ] && timed dot-cache scalar$v eab5a509a90ee9d0cdcdc5828211acfa bin/dot-fib$v $DOTC
  timed saxpy-mem scalar$v $SAXM bin/saxpy-fib$v $SAXN
  [ -n "$DOTC" ] && timed saxpy-cache scalar$v 853578cdee1b3dd4a08732371f1855ce bin/saxpy-fib$v $DOTC
  timed matmul scalar$v $MMM bin/matmul-fib$v $MMN
  timed spectral-norm scalar$v $SPM bin/spectral-fib$v $SPN
done
log "shootout done"

# a compute-bound scalar loop the auto-vectoriser widens: the target's default width and -prefer-256-bit (bench/lanes/poly.fib: 1024 doubles, 2e6 passes)
: > "$R/poly.tsv"
for v in "" -p512; do
  for i in $(seq $RUNS); do
    /usr/bin/time -f '%e' -o "$W/t" $PIN bin/poly-fib$v 1024 $([ -n "$SMOKE" ] && echo 2000 || echo 2000000) > "$W/out/stdout"
    printf 'poly%s\t%s\t%s\n' "$v" "$(cat "$W/t")" "$(cat "$W/out/stdout")" >> "$R/poly.tsv"
  done
done
log "poly done"

# ------------------------------------------------------------------ 4. the autodiff MLP step: fib.tensor at both widths against NumPy on OpenBLAS (5 runs, one CPU)
mkdir -p "$R/autodiff"
STEPS=200; [ -n "$SMOKE" ] && STEPS=20
for opt in sgd adam; do
  for r in $(seq $RUNS); do
    for w in 256 512; do
      $PIN bin/mlp-$w tape $opt $STEPS > "$R/autodiff/tape-$opt-$w-$r.txt"
    done
    LD_LIBRARY_PATH=$W/openblas OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 $PIN python3 "$B/tree/scripts/bench/autodiff/mlp.py" numpy $opt $STEPS > "$R/autodiff/numpy-$opt-$r.txt"
  done
done
for opt in sgd adam; do for w in 256 512; do $PIN bin/mlp-$w hand $opt $STEPS > "$R/autodiff/hand-$opt-$w-1.txt"; done; done
log "autodiff done"
touch "$R/DONE"
