#!/bin/bash
# Builds and times the benchmark suite, one program at a time, and prints a table.
#   FIBC=/path/to/bin/fibc scripts/bench/run.sh [-n RUNS] [name ...]
# FIBC defaults to ./bin/fibc (an unpacked release, see README Install), else `fibc` on PATH. Each benchmark is
# scripts/bench/NAME.fib; its Rust twin is scripts/bench/rust/FILE.rs run with an argument (the TWINS table
# below). Both must print the same line (the checksum): a column says ok or DIFF. RUNS defaults to 3.
# Needs: /usr/bin/time, rustc (for the twins; without it the Rust columns are blank), cc (fibc build links).
# Memory is capped with ulimit -v 16000000; programs run one after another. Output: a table on stdout, and
# the raw rows in $BENCH_OUT (default: /tmp/bench-rows.tsv).
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=3
if [ "${1:-}" = "-n" ]; then runs=$2; shift 2; fi
fibc=${FIBC:-}
[ -z "$fibc" ] && [ -x "$root/bin/fibc" ] && fibc=$root/bin/fibc
[ -z "$fibc" ] && fibc=$(command -v fibc || true)
if [ -z "$fibc" ]; then echo "run.sh: set FIBC to a fibc (an unpacked release: bin/fibc)" >&2; exit 2; fi
work=${BENCH_WORK:-$(mktemp -d)}
mkdir -p "$work"
out=${BENCH_OUT:-/tmp/bench-rows.tsv}
: > "$out"
ulimit -v 16000000

# name : twin source file : twin argument  (a benchmark without a twin has "-")
TWINS="
num-i64:num:i64
num-f64:num:f64
num-nbody:num:nbody
vec-conj-pop:vec:conj-pop
vec-assoc:vec:assoc
vec-index:vec:index
vec-sort:vec:sort
map-assoc-get:coll:map
set-conj:coll:set-conj
set-disj:coll:set-disj
lazy-fused:misc:lazy-fused
lazy-bound:misc:lazy-bound
strings:misc:strings
dispatch:misc:dispatch
recursion:misc:recursion
binary-trees:misc:binary-trees
hello:-:-
"

median() { sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
# timed CMD...: runs RUNS times, sets MED MIN MAX MEM and LAST (the last stdout)
timed() {
  local ts=() mem=0 i
  for i in $(seq "$runs"); do
    /usr/bin/time -f "%e %M" -o "$work/t" "$@" > "$work/stdout" 2>/dev/null   # linux-only: GNU time -f/-o for wall seconds and peak memory (BSD time has -l)
    read -r e m < "$work/t"
    ts+=("$e"); [ "$m" -gt "$mem" ] && mem=$m
  done
  MED=$(printf '%s\n' "${ts[@]}" | median)
  MIN=$(printf '%s\n' "${ts[@]}" | sort -n | head -1)
  MAX=$(printf '%s\n' "${ts[@]}" | sort -n | tail -1)
  MEM=$mem
  LAST=$(cat "$work/stdout")
}

want=" $* "
printf 'name\tcompile_s\trun_med_s\trun_min_s\trun_max_s\tmaxrss_kb\trust_med_s\tratio\tcheck\n' >> "$out"
for row in $TWINS; do
  IFS=: read -r name twin arg <<< "$row"
  [ "$want" != "  " ] && [[ "$want" != *" $name "* ]] && continue
  src=$here/$name.fib
  s=$(date +%s.%N)
  if ! "$fibc" build "$src" -o "$work/$name" > "$work/build.log" 2>&1; then
    echo "$name: build failed" >&2; head -5 "$work/build.log" >&2
    printf '%s\tFAIL\n' "$name" >> "$out"; continue
  fi
  e=$(date +%s.%N)
  comp=$(echo "$e - $s" | bc -l)
  timed "$work/$name"
  fmed=$MED; fmin=$MIN; fmax=$MAX; fmem=$MEM; fsum=$LAST
  rmed="-"; ratio="-"; check="-"
  if [ "$twin" != "-" ] && command -v rustc > /dev/null; then
    [ -x "$work/rs_$twin" ] || rustc -O -o "$work/rs_$twin" "$here/rust/$twin.rs" 2> "$work/rustc.log" || { echo "rustc $twin failed" >&2; cat "$work/rustc.log" >&2; }
    if [ -x "$work/rs_$twin" ]; then
      timed "$work/rs_$twin" "$arg"
      rmed=$MED; rsum=$LAST
      ratio=$(echo "scale=1; $fmed / ($rmed + 0.0001)" | bc -l)
      if [ "$fsum" = "$rsum" ]; then check=ok; else check="DIFF($fsum|$rsum)"; fi
    fi
  fi
  printf '%s\t%.2f\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$comp" "$fmed" "$fmin" "$fmax" "$fmem" "$rmed" "$ratio" "$check" >> "$out"
done

echo "fibc: $("$fibc" --version 2>&1 | head -1) ($fibc); $runs runs each; times in seconds; ratio = fibber run / Rust run"
column -t -s "$(printf '\t')" "$out"
# compile time of the compiler itself and of hello, if no names were given
if [ "$want" = "  " ] && [ -z "${BENCH_SKIP_EMIT:-}" ]; then
  echo
  echo "compile time (median of 3, fibc emit compiler/fibc.fib takes tens of seconds; BENCH_SKIP_EMIT=1 skips):"
  "$here/compile-time.sh" "$fibc" 3
fi
