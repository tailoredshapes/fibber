#!/bin/bash
# The benchmark round on one cloud instance (docs/shootout/aws-2026-10-06.md). Runs from the unpacked bundle:
#   bin/ (static fibber binaries, built elsewhere), tree/scripts/{shootout,bench/tensor,bench/autodiff}, this file.
#   nohup bash run-all.sh > run.log 2>&1 &      results in ./results; ./results/DONE at the end
set -u
B=$(cd "$(dirname "$0")" && pwd); cd "$B"
R=$B/results; mkdir -p "$R"; W=/dev/shm/bench; mkdir -p "$W"/{c,java,out}
arch=$(uname -m); trip=$arch-linux-gnu
SH=$B/tree/scripts/shootout
ulimit -v unlimited
log() { echo "[$(date +%T)] $*"; }
SMOKE=${SMOKE:-}   # SMOKE=1: small sizes, one run each (a check of this script, not a measurement)

# ---------------------------------------------------------------- machine facts
{ echo "== uname"; uname -a; echo "== lscpu"; lscpu; echo "== nproc"; nproc; echo "== free -g"; free -g; echo "== os"; head -2 /etc/os-release
  echo "== gcc"; gcc --version | head -1; echo "== java"; java -version 2>&1; echo "== python"; python3 --version
  echo "== numpy"; python3 -c 'import numpy; print(numpy.__version__); numpy.show_config()' 2>&1
  echo "== libblas.so.3 alternatives"; update-alternatives --display libblas.so.3-$trip 2>&1
  echo "== cpu model"; grep -m1 'model name\|CPU part' /proc/cpuinfo; echo "== flags"; grep -m1 -i 'flags\|features' /proc/cpuinfo
  echo "== instance type"; cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null
} > "$R/facts.txt" 2>&1
log facts done

# BLAS directories: reference (Netlib) and OpenBLAS, each a libblas.so.3 to put first on LD_LIBRARY_PATH
mkdir -p "$W/refblas" "$W/openblas"
ln -sf /usr/lib/$trip/blas/libblas.so.3 "$W/refblas/libblas.so.3"
ln -sf "$(ls /usr/lib/$trip/openblas-pthread/libblas.so.3)" "$W/openblas/libblas.so.3"
ls -l "$W/refblas" "$W/openblas" >> "$R/facts.txt"

# ---------------------------------------------------------------- shootout: build C and Java
case $arch in x86_64) march=-march=native ;; *) march=-mcpu=native ;; esac
src() { local f=$1/$2.$3 g=$1/${2//-/}.$3 h=$1/${2//-/_}.$3; if [ -f "$f" ]; then echo "$f"; elif [ -f "$g" ]; then echo "$g"; elif [ -f "$h" ]; then echo "$h"; fi; }
BENCHES="binary-trees fannkuch-redux fasta k-nucleotide mandelbrot n-body pidigits regex-redux reverse-complement spectral-norm dot-product saxpy matmul"
: > "$R/build.txt"
for d in $BENCHES; do
  s=$(src "$SH/$d" "$d" c); extra=""
  [ -f "$SH/$d/cflags" ] && extra=$(tr '\n' ' ' < "$SH/$d/cflags")
  grep -q -e '-ffp-contract=off' "$SH/$d/README.md" 2>/dev/null && extra="-ffp-contract=off $extra"
  echo "gcc -O3 $march $extra -o $W/c/$d $s -lm $extra" >> "$R/build.txt"
  gcc -O3 $march $extra -o "$W/c/$d" "$s" -lm $extra >> "$R/build.txt" 2>&1 || echo "C BUILD FAILED $d" >> "$R/build.txt"
  s=$(src "$SH/$d" "$d" java)
  cls=$(sed -n 's/^\(public \)\{0,1\}\(final \)\{0,1\}\(abstract \)\{0,1\}class[[:space:]]\+\([A-Za-z0-9_$]*\).*/\4/p' "$s" | head -1)
  mkdir -p "$W/java/$d"; echo "$cls" > "$W/java/$d/.class"
  echo "javac -d $W/java/$d $s   (class $cls)" >> "$R/build.txt"
  javac -d "$W/java/$d" "$s" >> "$R/build.txt" 2>&1 || echo "JAVA BUILD FAILED $d" >> "$R/build.txt"
done
log builds done

# ---------------------------------------------------------------- fasta inputs (the Java fasta twin, as run.sh)
INPUTS="25000000 5000000"; [ -n "$SMOKE" ] && INPUTS="100000 1000000 50000"
for n in $INPUTS; do java -cp "$W/java/fasta" fasta $n > "$W/fasta-$n.txt"; done
md5sum "$W"/fasta-*.txt > "$R/inputs.md5"
log inputs done

# ---------------------------------------------------------------- shootout: timed runs
# timed BENCH VARIANT RUNS MD5 INPUT CMD..   -> results/shootout.tsv rows: bench variant runs status median_e median_u median_rss_kb all_elapsed md5
printf 'bench\tvariant\truns\tstatus\telapsed_s\tuser_s\trss_kb\tall_elapsed\tmd5\n' > "$R/shootout.tsv"
timed() {
  local bench=$1 var=$2 n=$3 md5=$4 input=$5; shift 5
  local i st=ok rows=() got=- all=""
  for i in $(seq "$n"); do
    /usr/bin/time -f '%e %U %M' -o "$W/t" "$@" < "$input" > "$W/out/stdout" 2> "$W/out/stderr"
    local rc=$?
    got=$(md5sum < "$W/out/stdout" | cut -c1-32)
    if [ $rc -ne 0 ]; then st="FAIL-exit-$rc"; break; fi
    [ "$got" = "$md5" ] || { st=FAIL-md5; break; }
    rows+=("$(tail -n 1 "$W/t")"); all="$all $(tail -n 1 "$W/t" | cut -d' ' -f1)"
  done
  LAST_ST=$st; local med="- - -"
  [ "$st" = ok ] && med=$(printf '%s\n' "${rows[@]}" | sort -g | awk '{a[NR]=$0} END{print a[int((NR+1)/2)]}')
  set -- $med
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$bench" "$var" "$n" "$st" "$1" "$2" "$3" "${all# }" "$got" >> "$R/shootout.tsv"
  log "$bench $var $st $1"
}
# label | dir | args | md5 | input
TABLE="binary-trees|binary-trees|21|baf0dcbc307297f68bd9459833db9f73|-
fannkuch-redux|fannkuch-redux|12|0d70eeb93670d7b8580d83dbcad066f1|-
fasta|fasta|25000000|fd55b9e8011c781131046b6dd87511e1|-
k-nucleotide|k-nucleotide|25000000|f02cc90543a73d26a3bbf306227194b6|25000000
mandelbrot|mandelbrot|16000|8c2ed8883de64eccd3154ac612021fe8|-
n-body|n-body|50000000|32c32132315472b32572eb1f52fc01a6|-
pidigits|pidigits|10000|5b185f9a67a426baf78aa3bbb5baf8df|-
regex-redux|regex-redux|5000000|8fc8018a2c9d742ec732437feef4ef39|5000000
reverse-complement|reverse-complement|25000000|86befdda368165b26e6ef16620037658|25000000
spectral-norm|spectral-norm|5500|1584fbeab0a952f314fbf0fd7621885f|-
dot-mem|dot-product|100000000|6a2af661e2476978717dea02afc6a15e|-
dot-cache|dot-product|32768 61035|eab5a509a90ee9d0cdcdc5828211acfa|-
saxpy-mem|saxpy|100000000|2b3a0d6020ded2d3e0062b11605c75b9|-
saxpy-cache|saxpy|32768 61035|853578cdee1b3dd4a08732371f1855ce|-
matmul|matmul|2048|4bdcfd860a1bf6e409bbb88a547ee891|-"
[ -n "$SMOKE" ] && TABLE="binary-trees|binary-trees|16|2f8c4208684231318d69289ebb44b9d0|-
k-nucleotide|k-nucleotide|100000|0c829afd94c4dbf2793ff03cb8ee5b88|100000
regex-redux|regex-redux|50000|4de18ad9cf738abbb8f28552aa6dc092|50000
reverse-complement|reverse-complement|1000000|e1f3f9bd58199445c02ae78571a8d62a|1000000
pidigits|pidigits|1000|d68ffe833fdc0ed6ed4b47b7090e6340|-
saxpy-cache|saxpy|32768 61035|853578cdee1b3dd4a08732371f1855ce|-
matmul|matmul|256|a89463dd45295a657ee2921b3358a01b|-"
echo "$TABLE" | while IFS='|' read -r label dir args md5 inp; do
  n=5; case $label in fannkuch-redux|k-nucleotide) n=3 ;; esac; [ -n "$SMOKE" ] && n=1
  input=/dev/null; [ "$inp" != - ] && input=$W/fasta-$inp.txt
  # shellcheck disable=SC2086
  timed "$label" fib "$n" "$md5" "$input" "$B/bin/$dir-fib" $args
  # a build for this CPU that fails: the x86-64-v3 build of the same program, as a separate row
  [ "$LAST_ST" != ok ] && [ -x "$B/bin-v3/$dir-fib" ] && timed "$label" fib-v3 "$n" "$md5" "$input" "$B/bin-v3/$dir-fib" $args
  [ -x "$B/bin/$dir-simd" ] && timed "$label" fib-simd "$n" "$md5" "$input" "$B/bin/$dir-simd" $args
  [ -x "$W/c/$dir" ] && timed "$label" c "$n" "$md5" "$input" "$W/c/$dir" $args
  timed "$label" java "$n" "$md5" "$input" java -cp "$W/java/$dir" "$(cat "$W/java/$dir/.class")" $args
done
log shootout done

# ---------------------------------------------------------------- tensor table (fibber bench.fib vs NumPy; reference BLAS column and OpenBLAS column)
[ -z "${SKIP_TENSOR:-}" ] && ( cd "$B/tree/scripts/bench/tensor" && LD_LIBRARY_PATH=$W/refblas python3 run.py ${SMOKE:+--small} --fibc true --bin "$B/bin/tensor-bench" --openblas "$W/openblas" --tsv "$R/tensor.tsv" ) > "$R/tensor.txt" 2>&1
log tensor done

# ---------------------------------------------------------------- autodiff MLP (300 steps, 5 runs, pinned to CPU 0; NumPy on OpenBLAS)
export OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 BLIS_NUM_THREADS=1
mkdir -p "$R/autodiff"
STEPS=300; RUNS="1 2 3 4 5"; [ -n "$SMOKE" ] && { STEPS=3; RUNS=1; }
for opt in sgd adam; do
  for r in $RUNS; do
    taskset -c 0 "$B/bin/mlp" tape $opt $STEPS > "$R/autodiff/tape-$opt-$r.txt"
    taskset -c 0 "$B/bin/mlp" hand $opt $STEPS > "$R/autodiff/hand-$opt-$r.txt"
    LD_LIBRARY_PATH=$W/openblas taskset -c 0 python3 "$B/tree/scripts/bench/autodiff/mlp.py" numpy $opt $STEPS > "$R/autodiff/numpy-$opt-$r.txt"
  done
done
unset OMP_NUM_THREADS OPENBLAS_NUM_THREADS MKL_NUM_THREADS BLIS_NUM_THREADS
log autodiff done

# ---------------------------------------------------------------- fib.parallel scaling (N = 1e8; two processes of three rounds each per cell)
export MALLOC_ARENA_MAX=2
printf 'mode\tworkers\tn\trounds_ms\tvalues\n' > "$R/parallel.tsv"
PN=100000000; WS="1 2 4 8 16 32"; [ -n "$SMOKE" ] && { PN=1000000; WS="1 4"; }
for w in $WS; do
  for m in pmap pmap-each preduce preduce-n preduce-r; do
    o=$( { "$B/bin/parallel-closure" $m $w $PN; "$B/bin/parallel-closure" $m $w $PN; } )
    printf '%s\t%s\t%s\t%s\t%s\n' $m $w $PN "$(echo "$o" | sed -n 's/^t *//p' | tr '\n' ' ')" "$(echo "$o" | sed -n 's/^v *//p' | sort -u | tr '\n' ' ')" >> "$R/parallel.tsv"
  done
  log "parallel W=$w done"
done
for r in $RUNS; do "$B/bin/preduce-1e8" > "$R/preduce-1e8-$r.txt"; done
log parallel done
touch "$R/DONE"
