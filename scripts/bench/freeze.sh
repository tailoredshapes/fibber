#!/bin/bash
# scripts/bench/freeze.sh: the reference-count contention table with and without `freeze` (P-count-b; docs/shootout/parallel.md).
# Builds scripts/bench/freeze-contention.fib with FIBC and prints, for each row (MODE K) and thread count T, the median of three rounds in ms
# for T tasks doing N iterations each. A checksum that differs between the rows of one (K, T) is printed as FAIL.
#   FIBC=/path/to/F scripts/bench/freeze.sh ["MODE:K MODE:K .." ["THREADS" [N]]]
#   defaults: rows "private:64 shared:1 frozen:1 shared:64 frozen:64 read:64 readf:64", THREADS "1 2 4 8 16 28", N 1000000
# Each run takes /tmp/fibsuite.lock (the gate and the other benchmarks hold it), under `ulimit -v 16000000` and MALLOC_ARENA_MAX=2;
# the rows are medians and the 28-thread rows move by 10-20% between runs on this shared machine (trust ratios of 2x and more).
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
FIBC=${FIBC:?set FIBC to a stage 2}
rows=${1:-"private:64 shared:1 frozen:1 shared:64 frozen:64 read:64 readf:64"}; threads=${2:-"1 2 4 8 16 28"}; n=${3:-1000000}
work=${BENCH_WORK:-$(mktemp -d)}; mkdir -p "$work"
export MALLOC_ARENA_MAX=2 FIB_LIB=$root/lib; unset LD_LIBRARY_PATH
ulimit -v 16000000
"$FIBC" build "$here/freeze-contention.fib" -I "$root/compiler" -I "$root/lib" -o "$work/freeze-contention" || exit 2
run_locked() { if [ -n "${BENCH_HAVE_LOCK:-}" ]; then "$@"; else flock /tmp/fibsuite.lock "$@"; fi; }
printf '%-12s' "row"; for t in $threads; do printf '%10s' "T=$t"; done; echo "   (ms, median of 3, N=$n per task)"
for r in $rows; do
  m=${r%%:*}; k=${r##*:}
  printf '%-12s' "$m K=$k"
  for t in $threads; do
    out=$(run_locked "$work/freeze-contention" "$m" "$k" "$t" "$n")
    us=$(echo "$out" | sed -n 's/^t //p' | sort -n | sed -n 2p)
    printf '%10s' "$(echo "$us" | awk '{printf "%.0f", $1/1000}')"
  done
  echo
done
