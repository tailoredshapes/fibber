#!/bin/bash
# The SIMD shootout (SIMD wave 4, P5): times the scalar fibber program, the SIMD fibber program (<name>-simd.fib), Java and C
# of each kernel, the two fibber ones built for three CPUs (FIB_TARGET_CPU unset = the host, x86-64-v3, x86-64), checks every
# output against the md5 in the table below, and prints one TSV row per measurement. Report: docs/shootout/simd.md.
#
#   FIBC=/path/to/stage2/fibc scripts/shootout/simd-bench.sh [-n RUNS] [--out FILE] [--cpus host,v3,base] [KERNEL..]
#
# KERNEL is a label of the table (n-body spectral-norm mandelbrot dot-mem dot-cache saxpy-mem saxpy-cache matmul); no names
# runs them all. Every timed run holds `flock /tmp/fibsuite.lock` and runs under ulimit -v 16000000. The time is wall seconds
# of the whole process (EPOCHREALTIME around it, microsecond resolution); the row gives the median and the minimum of RUNS (5).
# Environment: FIBC (required), FIB_LIB (default <tree>/lib), SIMD_SCRATCH (default ~/.cache/fibber-scratch/p5-bench).
set -u
unset FIB_TARGET_CPU
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
scratch=${SIMD_SCRATCH:-$HOME/.cache/fibber-scratch/p5-bench}
fibc=${FIBC:?set FIBC to a stage 2 fibc}
export FIB_LIB=${FIB_LIB:-$root/lib}
runs=5; out=; cpus=host,v3,base; want=()
while [ $# -gt 0 ]; do
  case $1 in
    -n) runs=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --cpus) cpus=$2; shift 2 ;;
    -*) echo "simd-bench: unknown option $1" >&2; exit 2 ;;
    *) want+=("$1"); shift ;;
  esac
done
mkdir -p "$scratch"
[ -n "$out" ] || out=$scratch/rows.tsv

# label | directory and program name | arguments | md5 of the correct output
TABLE='n-body|n-body|50000000|32c32132315472b32572eb1f52fc01a6
spectral-norm|spectral-norm|5500|1584fbeab0a952f314fbf0fd7621885f
mandelbrot|mandelbrot|16000|8c2ed8883de64eccd3154ac612021fe8
dot-mem|dot-product|100000000|6a2af661e2476978717dea02afc6a15e
dot-cache|dot-product|32768 61035|eab5a509a90ee9d0cdcdc5828211acfa
saxpy-mem|saxpy|100000000|2b3a0d6020ded2d3e0062b11605c75b9
saxpy-cache|saxpy|32768 61035|853578cdee1b3dd4a08732371f1855ce
matmul|matmul|2048|4bdcfd860a1bf6e409bbb88a547ee891'

cpu_env() { case $1 in host) echo "" ;; v3) echo "x86-64-v3" ;; base) echo "x86-64" ;; esac; }
src() { # DIR NAME EXT
  local f=$1/$2.$3 g=$1/${2//-/}.$3
  if [ -f "$f" ]; then echo "$f"; elif [ -f "$g" ]; then echo "$g"; fi
}

# timed_median LABEL EXPECTED_MD5 COMMAND..: runs, checks, appends the TSV row
timed_median() {
  local bench=$1 variant=$2 md5=$3; shift 3
  local times=() r t0 t1 got status=ok
  for r in $(seq "$runs"); do
    t0=$EPOCHREALTIME
    got=$(flock /tmp/fibsuite.lock bash -c 'ulimit -v 16000000; exec "$@"' _ "$@" 2>/dev/null | md5sum | cut -c1-32)
    t1=$EPOCHREALTIME
    [ "$got" = "$md5" ] || status=FAIL
    times+=("$(echo "$t1 - $t0" | bc -l)")
  done
  local sorted med min
  sorted=$(printf '%s\n' "${times[@]}" | sort -g)
  med=$(echo "$sorted" | sed -n "$(( (runs + 1) / 2 ))p"); min=$(echo "$sorted" | head -1)
  printf '%s\t%s\t%s\t%.3f\t%.3f\t%s\n' "$bench" "$variant" "$status" "$med" "$min" "$*" | tee -a "$out"
}

: > "$out"
echo "$TABLE" | while IFS='|' read -r label dir args md5; do
  if [ ${#want[@]} -gt 0 ]; then
    hit=; for w in "${want[@]}"; do [ "$w" = "$label" ] && hit=1; done
    [ -n "$hit" ] || continue
  fi
  d=$here/$dir; b=$scratch/$label; mkdir -p "$b"
  fibs=$(src "$d" "$dir" fib); simd=$d/$dir-simd.fib; javas=$(src "$d" "$dir" java); cs=$(src "$d" "$dir" c)
  cflags=; grep -q -e '-ffp-contract=off' "$d/README.md" 2>/dev/null && cflags=-ffp-contract=off
  gcc -O3 -march=native $cflags -o "$b/c" "$cs" -lm || continue
  rm -rf "$b/java"; mkdir -p "$b/java"; javac -d "$b/java" "$javas" || continue
  jclass=$(sed -n 's/^\(public \)\{0,1\}\(final \)\{0,1\}class[[:space:]]\+\([A-Za-z0-9_$]*\).*/\3/p' "$javas" | head -1)
  for c in $(echo "$cpus" | tr ',' ' '); do
    tc=$(cpu_env "$c")
    for kind in scalar simd; do
      if [ $kind = scalar ]; then s=$fibs; else s=$simd; fi
      env ${tc:+FIB_TARGET_CPU=$tc} "$fibc" build "$s" -I "$root/lib" -o "$b/$kind-$c" > "$b/$kind-$c.log" 2>&1 \
        || { echo "build of $s for $c failed: $(head -c 300 "$b/$kind-$c.log")" >&2; continue; }
    done
  done
  # shellcheck disable=SC2086
  for c in $(echo "$cpus" | tr ',' ' '); do
    timed_median "$label" "fib-scalar@$c" "$md5" "$b/scalar-$c" $args
    timed_median "$label" "fib-simd@$c" "$md5" "$b/simd-$c" $args
  done
  timed_median "$label" "java" "$md5" java -cp "$b/java" "$jclass" $args
  timed_median "$label" "c" "$md5" "$b/c" $args
done
