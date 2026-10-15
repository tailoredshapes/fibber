#!/bin/bash
# compiler/tests/live/stress.sh [FIBC] [--socket-only|--tsan-only] [--quick]: swaps under load, the pause and the retirement (package
# LV3 of docs/design/live-repl.md section 6; serve.liveretire, serve.livesplit, rt/live.lir, fib.os.live, the pause points of
# fib.http.server). Two parts, each a stamp of its own in mk/tools.mk:
#   socket  stress.py over a real socket against stress-app.fib (8 spawned workers and the pool calling `f` while the attach thread
#           swaps it 1000 times; pause, retire, resume; the trap in a worker task; the long call) and swap-app.fib (the http server's
#           pause points); its docstring lists the checks. About two minutes; --quick is 100 swaps (the mutants).
#   tsan    scripts/tsan.sh --check over stress-tsan.fib emitted in live mode, with stress-swapper.lir appended (TSAN_APPEND): the
#           stub's acquire load against the runtime's release store (`fib.live-patch`), the pause counters, 8 threads and the pool;
#           clean means no report beyond scripts/tsan-baseline.txt in TSAN_RUNS runs (2 here). What TSAN does not see is in the
#           program's header. About two minutes (lairf is built once per F).
# Exit 0 when every check holds. The mutants of scripts/mutant-live.sh: retire-without-pause (socket), patch-plain-store and
# stub-plain-load (tsan) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-}; part=both; quick=
while [ $# -gt 0 ]; do
  case $1 in --socket-only) part=socket ;; --tsan-only) part=tsan ;; --quick) quick=1 ;; *) fibc=$1 ;; esac
  shift
done
[ -x "$fibc" ] || { echo "usage: stress.sh FIBC [--socket-only|--tsan-only] [--quick] (or FIBC in the environment)"; exit 2; }
fibc=$(cd "$(dirname "$fibc")" && pwd)/$(basename "$fibc")
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
cd "$root" || exit 2
bad=0
if [ $part != tsan ]; then
  swaps=1000; [ -n "$quick" ] && swaps=100
  python3 "$here/stress.py" "$fibc" "$root" --swaps $swaps || bad=1
fi
if [ $part != socket ]; then
  out=${TSAN_OUT:-$HOME/.cache/fibber-scratch/live-stress-tsan}
  swaps=300; [ -n "$quick" ] && swaps=100
  if F=$fibc TSAN_OUT=$out TSAN_RUNS=${TSAN_RUNS:-2} FIB_LIVE=1 TSAN_APPEND="$here/stress-swapper.lir" "$root/scripts/tsan.sh" --check "$here/stress-tsan.fib" $swaps 20000 > "$out.log" 2>&1
  then echo "ok tsan: $(tail -1 "$out.log")"
  else echo "FAIL tsan: $(grep -E '^NEW|^exit|^tsan' "$out.log" | head -5 | tr '\n' ' ')"; tail -3 "$out.log"; bad=1; fi
fi
[ $bad = 0 ] && echo "live stress: ok" || echo "live stress: FAILED"
exit $bad
