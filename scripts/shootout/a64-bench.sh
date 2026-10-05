#!/bin/bash
# First performance numbers on aarch64 (A64-1; report docs/shootout/aarch64.md): the same programs on any machine, in a form that runs on macOS (bash 3.2, no
# flock, no /usr/bin/time, no Java: perl's Time::HiRes times a run, md5 or md5sum checks the output). Per program: the scalar fibber build, the SIMD fibber build (where
# the directory has <name>-simd.fib) and the C twin (cc -O3), each timed RUNS times; the row says the median and the minimum wall seconds of the whole process and
# whether the output matched the md5 below. The fibber builds use the host's CPU (FIB_TARGET_CPU unset), as the shootout's `host` column does.
#   FIBC=/path/to/fibc scripts/shootout/a64-bench.sh [-n RUNS] [--out FILE] [--only LABEL..]
# Rows: machine<TAB>label<TAB>variant<TAB>status<TAB>median<TAB>min. Environment: FIBC (required), FIB_LIB (default <tree>/lib), A64_SCRATCH (default $TMPDIR/a64-bench).
set -u
unset FIB_TARGET_CPU
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
fibc=${FIBC:?set FIBC to a stage 2 fibc}
export FIB_LIB=${FIB_LIB:-$root/lib}
scratch=${A64_SCRATCH:-${TMPDIR:-/tmp}/a64-bench}; mkdir -p "$scratch"
runs=3; out=$scratch/rows.tsv; only=
while [ $# -gt 0 ]; do
  case $1 in
    -n) runs=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --only) shift; while [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; do only="$only $1"; shift; done ;;
    *) echo "a64-bench: unknown option $1" >&2; exit 2 ;;
  esac
done
machine=$(uname -sm | tr ' ' '-')
md5of() { if command -v md5sum >/dev/null 2>&1; then md5sum | cut -c1-32; else md5 -r | cut -c1-32; fi; }
now() { perl -MTime::HiRes=time -e 'printf "%.6f\n", time'; }
# label | directory | program | arguments | md5 of the correct output (those of scripts/shootout/*/sizes.txt and simd-bench.sh; tensor-mmul has none: its checksum is compared across machines)
TABLE='n-body|n-body|n-body|50000000|32c32132315472b32572eb1f52fc01a6
spectral-norm|spectral-norm|spectral-norm|5500|1584fbeab0a952f314fbf0fd7621885f
mandelbrot|mandelbrot|mandelbrot|16000|8c2ed8883de64eccd3154ac612021fe8
fannkuch-redux|fannkuch-redux|fannkuch-redux|12|0d70eeb93670d7b8580d83dbcad066f1
binary-trees|binary-trees|binary-trees|21|baf0dcbc307297f68bd9459833db9f73
dot-mem|dot-product|dot-product|100000000|6a2af661e2476978717dea02afc6a15e
dot-cache|dot-product|dot-product|32768 61035|eab5a509a90ee9d0cdcdc5828211acfa
saxpy-mem|saxpy|saxpy|100000000|2b3a0d6020ded2d3e0062b11605c75b9
saxpy-cache|saxpy|saxpy|32768 61035|853578cdee1b3dd4a08732371f1855ce
matmul|matmul|matmul|2048|4bdcfd860a1bf6e409bbb88a547ee891
tensor-mmul-f64|tensor-mmul|tensor-mmul|1024 3|-
tensor-mmul-f32|tensor-mmul|tensor-mmul|1024 3 f32|-'
# timed LABEL VARIANT MD5 COMMAND..
timed() {
  local label=$1 variant=$2 md5=$3; shift 3
  local times= r t0 t1 got status=ok first=
  for r in $(seq "$runs"); do
    t0=$(now); got=$( ( ulimit -v 16000000 2>/dev/null; "$@" 2>/dev/null ) | md5of ); t1=$(now)
    if [ "$md5" != - ]; then [ "$got" = "$md5" ] || status=FAIL; else [ -z "$first" ] && first=$got; [ "$got" = "$first" ] || status=FAIL; fi
    times="$times $(perl -e "printf '%.3f', $t1 - $t0")"
  done
  local sorted med min
  sorted=$(printf '%s\n' $times | sort -g); med=$(echo "$sorted" | sed -n "$(( (runs + 1) / 2 ))p"); min=$(echo "$sorted" | head -1)
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$machine" "$label" "$variant" "$status" "$med" "$min" "${md5:0:8}${md5:+/}$got" | tee -a "$out"
}
: > "$out"
echo "$TABLE" | while IFS='|' read -r label dir prog args md5; do
  if [ -n "$only" ]; then hit=; for w in $only; do [ "$w" = "$label" ] && hit=1; done; [ -n "$hit" ] || continue; fi
  d=$here/$dir; b=$scratch/$label; mkdir -p "$b"
  scalar=$d/$prog.fib; simd=$d/$prog-simd.fib; cs=$d/$prog.c
  [ -f "$scalar" ] || scalar=$d/${prog//-/}.fib
  "$fibc" build "$scalar" -I "$root/lib" -o "$b/scalar" > "$b/scalar.log" 2>&1 || { echo "build of $scalar failed: $(head -c 300 "$b/scalar.log")" >&2; continue; }
  # shellcheck disable=SC2086
  timed "$label" fib-scalar "$md5" "$b/scalar" $args
  if [ -f "$simd" ] && "$fibc" build "$simd" -I "$root/lib" -o "$b/simd" > "$b/simd.log" 2>&1; then
    # shellcheck disable=SC2086
    timed "$label" fib-simd "$md5" "$b/simd" $args
  fi
  if [ -f "$cs" ]; then
    cflags=-O3; [ "$(uname -m)" = x86_64 ] && cflags="-O3 -march=native"; grep -q -e '-ffp-contract=off' "$d/README.md" 2>/dev/null && cflags="$cflags -ffp-contract=off"
    # shellcheck disable=SC2086
    if cc $cflags -o "$b/c" "$cs" -lm 2> "$b/c.log"; then timed "$label" c-cc-O3 "$md5" "$b/c" $args; fi
  fi
done
