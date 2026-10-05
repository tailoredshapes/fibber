#!/bin/bash
# Times scripts/bench/windows.fib (exclusive-view windows against array loops; package EV2), built at -O2 and run RUNS times under the suite lock,
# prints the median ns per element of every kernel and checks the claims: a window loop within 1.3x of the array loop (read-modify-write, fill, saxpy,
# blocked write within 2.5x because every tile is lent once), saxpy through windows within 1.3x of the SIMD kernel, four tiles on four tasks faster than one
# (a compute-bound kernel), and every window line with the checksum of its array line. Exit 0 when all hold, 1 when one does not, 2 on a setup error.
#   FIBC=/path/to/stage2/fibc scripts/bench/windows.sh [-n RUNS]
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=3; [ "${1:-}" = "-n" ] && { runs=$2; shift 2; }
fibc=${FIBC:?set FIBC to a stage 2 fibc}
scratch=${WINDOWS_BENCH_SCRATCH:-$HOME/.cache/fibber-scratch/windows-bench}; mkdir -p "$scratch/tmp"
export FIB_LIB=${FIB_LIB:-$root/lib} TMPDIR=$scratch/tmp
ulimit -v 16000000
cd "$root" || exit 2
"$fibc" build scripts/bench/windows.fib -I lib -o "$scratch/windows" || { echo "windows.sh: build failed"; exit 2; }
: > "$scratch/rows.tsv"
for i in $(seq "$runs"); do flock /tmp/fibsuite.lock "$scratch/windows" >> "$scratch/rows.tsv" || { echo "windows.sh: run failed"; exit 2; }; done
med() { awk -F'\t' -v k="$1" '$1==k{print $2}' "$scratch/rows.tsv" | sort -g | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
sum() { awk -F'\t' -v k="$1" '$1==k{print $3}' "$scratch/rows.tsv" | sort -u; }
bad=0
for k in rmw-window rmw-array fill-window fill-array saxpy-window saxpy-array saxpy-simd tile-window tile-array par-1 par-4; do
  printf '%-14s %10s ns/elem\n' "$k" "$(med $k)"
done
ratio() { # NAME A B LIMIT: A/B at most LIMIT
  local r; r=$(awk -v a="$(med "$2")" -v b="$(med "$3")" 'BEGIN{printf "%.2f", a/b}')
  if awk -v r="$r" -v l="$4" 'BEGIN{exit !(r<=l)}'; then echo "ok    $1: $r (limit $4)"; else echo "FAIL  $1: $r (limit $4)"; bad=1; fi
}
ratio "rmw window/array" rmw-window rmw-array 1.3
ratio "fill window/array" fill-window fill-array 1.3
ratio "saxpy window/array" saxpy-window saxpy-array 1.3
ratio "saxpy window/simd" saxpy-window saxpy-simd 1.3
ratio "tile window/array" tile-window tile-array 2.5
ratio "4 tiles / 1 tile (time)" par-4 par-1 0.7
for pair in "rmw-window rmw-array" "fill-window fill-array" "saxpy-window saxpy-array" "saxpy-window saxpy-simd" "tile-window tile-array" "par-1 par-4"; do
  set -- $pair
  [ "$(sum "$1")" = "$(sum "$2")" ] || { echo "FAIL  checksums differ: $1 $(sum "$1") against $2 $(sum "$2")"; bad=1; }
done
exit $bad
