#!/bin/bash
# scripts/bench/parallel-closure.sh: the closure forms of fib.parallel against the macro forms (P-count-b; docs/shootout/parallel.md).
# Builds scripts/bench/parallel-closure.fib with FIBC and prints, per MODE, the median of three rounds (ms) at W workers over N elements, and
# FAIL when the checksum of a mode differs from the macro form's (a wrong answer is not a speedup).
#   FIBC=/path/to/F scripts/bench/parallel-closure.sh ["MODES" [W [N]]]     defaults: "pmap pmap-each preduce preduce-n preduce-r", 28, 1000000
# Runs under /tmp/fibsuite.lock, `ulimit -v 16000000`, MALLOC_ARENA_MAX=2. The machine is shared: trust ratios of 2x and more.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
FIBC=${FIBC:?set FIBC to a stage 2}
modes=${1:-"pmap pmap-each preduce preduce-n preduce-r"}; w=${2:-28}; n=${3:-1000000}
work=${BENCH_WORK:-$(mktemp -d)}; mkdir -p "$work"
export MALLOC_ARENA_MAX=2 FIB_LIB=$root/lib; unset LD_LIBRARY_PATH
ulimit -v 16000000
"$FIBC" build "$here/parallel-closure.fib" -I "$root/compiler" -I "$root/lib" -o "$work/parallel-closure" || exit 2
run_locked() { if [ -n "${BENCH_HAVE_LOCK:-}" ]; then "$@"; else flock /tmp/fibsuite.lock "$@"; fi; }
echo "W=$w N=$n (ms, median of 3)"
for m in $modes; do
  out=$(run_locked "$work/parallel-closure" "$m" "$w" "$n")
  ms=$(echo "$out" | sed -n 's/^t //p' | sort -n | sed -n 2p)
  v=$(echo "$out" | sed -n 's/^v //p' | sort -u | tr '\n' ' ')
  printf '%-10s %8s ms   v=%s\n' "$m" "$ms" "$v"
done
