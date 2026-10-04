#!/bin/bash
# The dev-loop benchmark (docs/design/dev-loop.md section 2.1): whole-command wall times of the edit-run loop, median of N runs, against the
# checked-in baseline scripts/bench/dev-loop.tsv. A delta over +10% is flagged REGRESSION (exit 3), as scripts/bench/quick.sh does; a run
# whose exit status differs from the expectation is FAILED (exit 1).
#   scripts/bench/dev-loop.sh [--record] [-n RUNS]
#   --record   rewrite the baseline from this run (a quiet machine, a tree whose answers are right)
#   -n RUNS    runs of each command (default 7; the ownership suite is run 3 times at most)
# Rows: run -O 0 of `empty` (2 lines), `hello` (2 lines), `p200` (176 lines, scripts/bench/dev-loop/p200.fib), run -O 2 of `p200`, and
# `fibc cases cases/ownership -j 12` (the whole directory).
# The compiler is the stage 2 built from this tree (scripts/lib/stage2.sh) or the fibc named by BENCH_FIBC. Holds /tmp/fibsuite.lock.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/../.." && pwd)}
base=$here/dev-loop.tsv
record=; runs=7
while [ $# -gt 0 ]; do
  case $1 in
    --record) record=1 ;;
    -n) runs=${2:?-n needs a number}; shift ;;
    *) echo "usage: scripts/bench/dev-loop.sh [--record] [-n RUNS]" >&2; exit 2 ;;
  esac
  shift
done
# shellcheck source=../lib/stage2.sh
. "$here/../lib/stage2.sh"
take_suite_lock
if [ -n "${BENCH_FIBC:-}" ]; then fibc=$BENCH_FIBC; used=$BENCH_FIBC
else stage2_ensure || exit 2; fibc=$GATE_OUT/F; used="stage 2 of tree $(tree_stamp)"; fi
export FIB_LIB=$root/lib
work=$GATE_OUT/dev-loop; mkdir -p "$work"
printf '(ns main)\n(defun main () -> i64 0)\n' > "$work/empty.fib"
cp "$root/scripts/bench/hello.fib" "$work/hello.fib"
cp "$here/dev-loop/p200.fib" "$work/p200.fib"

# time_cmd N EXPECTED_STATUS CMD..: the median wall time in ms of N runs; "FAIL" when a status differs.
time_cmd() {
  local n=$1 want=$2 s e st ts=(); shift 2
  for _ in $(seq "$n"); do
    s=$(date +%s%N)
    (ulimit -v 16000000; cd "$root" && exec "$@") > "$work/out" 2>&1; st=$?
    e=$(date +%s%N)
    [ "$st" = "$want" ] || { echo "FAIL (exit $st, want $want)"; return 1; }
    ts+=($(( (e - s) / 1000000 )))
  done
  printf '%s\n' "${ts[@]}" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'
}

rows=$work/rows.tsv; : > "$rows"; bad=0; reg=0
printf '%-16s %8s %8s %8s  %s\n' row base_ms now_ms delta% verdict
row() { # row NAME N STATUS CMD..
  local name=$1 n=$2 want=$3 ms b d v; shift 3
  ms=$(time_cmd "$n" "$want" "$@") || { printf '%-16s %s\n' "$name" "$ms"; bad=1; return; }
  printf '%s\t%s\n' "$name" "$ms" >> "$rows"
  b=$(awk -F'\t' -v n="$name" '$1 == n {print $2}' "$base" 2>/dev/null)
  if [ -z "$b" ]; then printf '%-16s %8s %8s %8s  %s\n' "$name" - "$ms" - "no baseline"; return; fi
  d=$(awk -v a="$b" -v c="$ms" 'BEGIN {printf "%+.1f", (c - a) * 100 / a}')
  v=ok; if awk -v x="$d" 'BEGIN {exit !(x > 10)}'; then v=REGRESSION; reg=1; fi
  printf '%-16s %8s %8s %8s  %s\n' "$name" "$b" "$ms" "$d" "$v"
}
row run-O0-empty "$runs" 0 "$fibc" run -O 0 "$work/empty.fib"
row run-O0-hello "$runs" 0 "$fibc" run -O 0 "$work/hello.fib"
row run-O0-p200 "$runs" 0 "$fibc" run -O 0 "$work/p200.fib"
row run-O2-p200 "$runs" 0 "$fibc" run -O 2 "$work/p200.fib"
n3=$(( runs < 3 ? runs : 3 ))
row cases-own-j12 "$n3" 0 "$fibc" cases cases/ownership -j 12

if [ -n "$record" ]; then
  { echo "# compiler: $used; $(date +%F); $(nproc) cores"
    echo "# scripts/bench/dev-loop.sh --record: row, median ms of $runs runs"
    cat "$rows"; } > "$base"
  echo "recorded $base"
fi
[ "$bad" -ne 0 ] && { echo "dev-loop: FAILED"; exit 1; }
[ "$reg" -ne 0 ] && { echo "dev-loop: REGRESSION over 10%"; exit 3; }
echo "dev-loop: ok"; exit 0
