#!/bin/bash
# The quick benchmark: a fast subset of scripts/bench (each program runs about 1-2 s), 3 runs each, the median, compared with the
# checked-in baseline scripts/bench/baseline.tsv. Prints a delta per benchmark; a delta over +10% is flagged REGRESSION; an answer
# that differs from the baseline's (the checksum: a hash of what the program printed) is flagged WRONG ANSWER, which is a failure
# whatever the time (a wrong answer is not a speedup).
#   scripts/bench/quick.sh [--record] [-n RUNS] [NAME..]
#   --record   rewrite the baseline from this run (do it on a quiet machine, from a tree whose answers are right)
#   -n RUNS    runs of each program (default 3)
#   NAME..     only these (default: the list QUICK below)
# Exit status: 0 nothing flagged, 1 a wrong answer or a build failure, 3 a regression only, 2 a usage or setup error.
# The compiler is the stage 2 built from this tree (scripts/lib/stage2.sh: cached under $GATE_OUT, built by FIBC/SEED or the previous F),
# or the fibc named by BENCH_FIBC. The baseline says which was used; times of different compilers are not comparable.
# Alone: takes /tmp/fibsuite.lock, the one scripts/gate.sh holds, so a gate run and a benchmark never overlap; nothing else heavy
# may run meanwhile (the lead's rule: benchmarks alone).
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/../.." && pwd)}   # GATE_ROOT: bench another checkout (scripts/batch.sh); its programs, this script's baseline
src=$root/scripts/bench
base=$here/baseline.tsv
record=; runs=3
while [ $# -gt 0 ]; do
  case $1 in
    --record) record=1 ;;
    -n) runs=${2:?-n needs a number}; shift ;;
    -*) echo "usage: scripts/bench/quick.sh [--record] [-n RUNS] [NAME..]" >&2; exit 2 ;;
    *) break ;;
  esac
  shift
done
# shellcheck source=../lib/stage2.sh
. "$here/../lib/stage2.sh"
take_suite_lock
QUICK="num-f64 num-nbody vec-conj-pop vec-index vec-sort map-assoc-get set-conj lazy-fused lazy-bound strings"
names=${*:-$QUICK}

if [ -n "${BENCH_FIBC:-}" ]; then fibc=$BENCH_FIBC; used="$BENCH_FIBC"; pick_builder >/dev/null 2>&1 || true
else stage2_ensure || exit 2; fibc=$GATE_OUT/F; used="stage 2 of tree $(tree_stamp)"; fi
work=$GATE_OUT/bench; mkdir -p "$work"
median() { sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }

# Builds NAME with the compiler, cached by the compiler's identity and the source's hash.
build_bench() {
  local n=$1 key
  key=$(sha1sum < "$fibc" | cut -c1-12)-$(sha1sum < "$src/$n.fib" | cut -c1-12)
  if [ "$(cat "$work/$n.key" 2>/dev/null)" = "$key" ] && [ -x "$work/$n" ]; then return 0; fi
  rm -f "$work/$n"
  (ulimit -v 16000000; "$fibc" build "$src/$n.fib" -o "$work/$n" > "$work/$n.build.log" 2>&1) || return 1
  echo "$key" > "$work/$n.key"
}

rows=$work/rows.tsv; : > "$rows"; bad=0; reg=0
printf '%-14s %9s %9s %8s  %s\n' name base_s now_s delta% verdict
for n in $names; do
  if [ ! -f "$src/$n.fib" ]; then echo "quick.sh: no benchmark $n" >&2; exit 2; fi
  if ! build_bench "$n"; then printf '%-14s build FAILED (%s)\n' "$n" "$work/$n.build.log"; bad=1; continue; fi
  ts=(); sum=
  for i in $(seq "$runs"); do
    /usr/bin/time -f "%e" -o "$work/t" bash -c "ulimit -v 16000000; exec \"$work/$n\"" > "$work/stdout" 2> "$work/stderr"   # linux-only: GNU time -f/-o for wall seconds and peak memory (BSD time has -l)
    ts+=("$(tail -n 1 "$work/t")"); s=$(sha1sum < "$work/stdout" | cut -c1-12)
    if [ -n "$sum" ] && [ "$s" != "$sum" ]; then sum=UNSTABLE; else sum=$s; fi
  done
  med=$(printf '%s\n' "${ts[@]}" | median)
  printf '%s\t%s\t%s\n' "$n" "$med" "$sum" >> "$rows"
  b=$(awk -F'\t' -v n="$n" '$1 == n && $0 !~ /^#/ { print $2 "\t" $3 }' "$base" 2>/dev/null)
  if [ -n "$record" ]; then printf '%-14s %9s %9s %8s  %s\n' "$n" - "$med" - "recorded ($sum)"; continue; fi
  if [ -z "$b" ]; then printf '%-14s %9s %9s %8s  %s\n' "$n" - "$med" - "not in the baseline"; continue; fi
  bt=${b%%$'\t'*}; bs=${b##*$'\t'}
  d=$(echo "scale=1; ($med - $bt) * 100 / $bt" | bc -l | sed 's/^\./0./; s/^-\./-0./')
  v=ok
  if [ "$sum" != "$bs" ]; then v="WRONG ANSWER (checksum $sum, baseline $bs)"; bad=1
  elif [ "$(echo "$d > 10 && $med - $bt > 0.03" | bc -l)" = 1 ]; then v="REGRESSION (over 10%)"; reg=1; fi
  printf '%-14s %9s %9s %+8s  %s\n' "$n" "$bt" "$med" "$d" "$v"
done
if [ -n "$record" ]; then
  { echo "# scripts/bench/quick.sh --record: name, median seconds of $runs runs, checksum (sha1 of the program's output)"
    echo "# compiler: $used; $(date -u +%Y-%m-%d); tree $(tree_stamp); $(nproc) cores"
    if [ -f "$base" ]; then awk -F'\t' '$0 !~ /^#/' "$base" | while IFS=$'\t' read -r n t c; do grep -q "^$n	" "$rows" || printf '%s\t%s\t%s\n' "$n" "$t" "$c"; done; fi
    cat "$rows"; } | sort -s -t$'\t' -k1,1 > "$work/new-base"
  { grep '^#' "$work/new-base"; grep -v '^#' "$work/new-base"; } > "$base"
  echo "recorded $base"
  [ "$bad" -eq 0 ]; exit
fi
[ "$bad" -ne 0 ] && { echo "BENCH FAIL: a wrong answer or a build failure"; exit 1; }
[ "$reg" -ne 0 ] && { echo "BENCH: regression over 10% flagged"; exit 3; }
echo "BENCH ok (nothing over +10%, all answers as in the baseline)"
