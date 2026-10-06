#!/bin/bash
# scripts/bench/atoms.sh: the atom-contention table (P-count-a; docs/shootout/atoms.md). Builds scripts/bench/atom-contention.fib with FIBC and
# prints, for each MODE and thread count T, the median over three rounds (the program runs three) of the wall time in ms for T tasks doing
# N = 200 000 operations each on one shared atom (or adder). A wrong final value (v) is printed as FAIL, not as a time.
#   FIBC=/path/to/F scripts/bench/atoms.sh ["MODES" ["THREADS" [N]]]
#   defaults: MODES="read swap cas adder oread oswap swap-own read-own", THREADS="1 2 4 8 16 28"
# Every run takes /tmp/fibsuite.lock (the gate and the other benchmarks hold it), runs under `ulimit -v 16000000` with MALLOC_ARENA_MAX=2,
# and the rows are medians: the machine is shared and the 28-thread rows move by 10-20% between runs. Output is one line per mode.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
FIBC=${FIBC:?set FIBC to a stage 2}
modes=${1:-"read swap cas adder oread oswap swap-own read-own"}; threads=${2:-"1 2 4 8 16 28"}; n=${3:-200000}
work=${BENCH_WORK:-$(mktemp -d)}; mkdir -p "$work"
export MALLOC_ARENA_MAX=2 FIB_LIB=$root/lib; unset LD_LIBRARY_PATH
ulimit -v 16000000
"$FIBC" build "$here/atom-contention.fib" -I "$root/compiler" -I "$root/lib" -o "$work/atom-contention" || exit 2
# BENCH_HAVE_LOCK=1: the caller already holds /tmp/fibsuite.lock for the whole run (`flock /tmp/fibsuite.lock env BENCH_HAVE_LOCK=1 scripts/bench/atoms.sh`),
# so a table is not taken in pieces between other jobs
run_locked() { if [ -n "${BENCH_HAVE_LOCK:-}" ]; then "$@"; else flock /tmp/fibsuite.lock "$@"; fi; }
printf '%-9s' "mode"; for t in $threads; do printf '%10s' "T=$t"; done; echo "   (ms, median of 3, N=$n per task)"
for m in $modes; do
  printf '%-9s' "$m"
  for t in $threads; do
    out=$(run_locked "$work/atom-contention" "$m" "$t" "$n")
    v=$(echo "$out" | sed -n 's/^v //p')
    want=$(( 3 * t * n ))
    case $m in swap|cas|adder) [ "$v" = "$want" ] || { printf '%10s' FAIL; continue; } ;; oswap) [ "$v" = 1 ] || { printf '%10s' FAIL; continue; } ;; esac
    us=$(echo "$out" | sed -n 's/^t //p' | sort -n | sed -n 2p)
    printf '%10s' "$(echo "$us" | awk '{printf "%.1f", $1/1000}')"
  done
  echo
done
