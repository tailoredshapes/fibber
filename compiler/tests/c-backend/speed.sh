#!/bin/bash
# The speed of the C route against LLVM (docs/design/lir2c.md section 5): each program is built natively (`F build`, LLVM -O2) and through C
# (`F --via c --cc CC build`), run RUNS times each, the median wall time of each reported with the ratio, and the outputs compared (a
# different output is a failure, not a speedup: ADR 0019).
#   compiler/tests/c-backend/speed.sh --fibc F [--cc CC] [-n RUNS] [NAME=FILE.fib:ARGS]..
#   default programs: the quick bench set (scripts/bench/quick.sh) and five shootout programs at their small size.
# Takes /tmp/fibsuite.lock as the benchmarks do (nothing else heavy runs meanwhile). Environment: TMPDIR, FIB_LIB (set here).
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
fibc=""; cc=${FIB_CC:-gcc}; runs=3; progs=()
while [ $# -gt 0 ]; do
  case $1 in --fibc) fibc=$2; shift ;; --cc) cc=$2; shift ;; -n) runs=$2; shift ;; *) progs+=("$1") ;; esac; shift
done
: "${fibc:?--fibc F}"
export FIB_LIB=$root/lib
work=${TMPDIR:-/tmp}/cspeed; mkdir -p "$work"
if [ ${#progs[@]} -eq 0 ]; then
  for n in lazy-bound lazy-fused map-assoc-get num-f64 num-nbody set-conj strings vec-conj-pop vec-index vec-sort; do progs+=("$n=$root/scripts/bench/$n.fib:"); done
  progs+=("n-body=$root/scripts/shootout/n-body/n-body.fib:500000" "binary-trees=$root/scripts/shootout/binary-trees/binary-trees.fib:16"
          "fannkuch-redux=$root/scripts/shootout/fannkuch-redux/fannkuch-redux.fib:10" "mandelbrot=$root/scripts/shootout/mandelbrot/mandelbrot.fib:2000"
          "fasta=$root/scripts/shootout/fasta/fasta.fib:1000000")
fi
median() { sort -n | awk '{ a[NR] = $1 } END { print a[int((NR + 1) / 2)] }'; }
time_of() { # time_of EXE ARGS: median elapsed seconds of RUNS runs, the output's sha1 in $sum
  local exe=$1 args=$2 ts=() i s; sum=
  for i in $(seq "$runs"); do
    # shellcheck disable=SC2086
    /usr/bin/time -f "%e" -o "$work/t" bash -c "ulimit -v 16000000; exec \"$exe\" $args" > "$work/out" 2> "$work/err"
    ts+=("$(tail -n 1 "$work/t")"); s=$(sha1sum < "$work/out" | cut -c1-12)
    if [ -n "$sum" ] && [ "$s" != "$sum" ]; then sum=UNSTABLE; else sum=$s; fi
  done
  printf '%s\n' "${ts[@]}" | median
}
exec 9> /tmp/fibsuite.lock; flock 9
printf '%-16s %9s %9s %8s  %s\n' program native_s c_s ratio output
bad=0
for p in "${progs[@]}"; do
  name=${p%%=*}; rest=${p#*=}; file=${rest%%:*}; args=${rest#*:}
  (cd "$root" && ulimit -v 16000000 && "$fibc" build "$file" -o "$work/$name.n" > "$work/$name.n.log" 2>&1) || { printf '%-16s native build FAILED\n' "$name"; bad=1; continue; }
  (cd "$root" && ulimit -v 16000000 && "$fibc" --via c --cc "$cc" build "$file" -o "$work/$name.c" > "$work/$name.c.log" 2>&1) || { printf '%-16s C build FAILED (%s)\n' "$name" "$work/$name.c.log"; bad=1; continue; }
  tn=$(time_of "$work/$name.n" "$args"); sn=$sum
  tc=$(time_of "$work/$name.c" "$args"); sc=$sum
  ratio=$(echo "scale=2; $tc / $tn" | bc -l | sed 's/^\./0./')
  if [ "$sn" = "$sc" ] && [ "$sn" != UNSTABLE ]; then v=same; else v="DIFFERENT ($sn vs $sc)"; bad=1; fi
  printf '%-16s %9s %9s %8s  %s\n' "$name" "$tn" "$tc" "$ratio" "$v"
done
exit $bad
