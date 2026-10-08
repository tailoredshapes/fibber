#!/bin/bash
# scripts/tsan-planted.sh: planted faults for scripts/tsan.sh. A race detector that cannot fail proves nothing, so each check puts one known
# race back and passes only if tsan.sh REPORTS it (and is silent on the same program without the fault).
#   atomic-mt   the `fib.mt` flag (rt/core.lir, rt/thread.lir) made plain again, by a sed on the LLVM IR (TSAN_SED): the allocator's reads and
#               the spawn's store must be reported as a data race on global 'fib.mt'          kernel: spawnjoin
#   tile-seed   `tile-window` (lib/fib/view/tiles.fib) made to read element 0 itself again, in a scratch copy of lib/ (FIB_LIB): the read
#               must be reported against the write of the tile that owns element 0             kernel: tiles
#   enqueue-share  `fib.share-task` (rt/task.lir, called by await-or-park) no longer share-marks the tasks it hands to other threads (the call deleted from the IR): the
#               plain counts of an `async` task used on two threads must be reported (fib.release against fib.release)  case: ownership/32
#   upgrade-atomic  the count read of `fib.upgrade` (rt/weak.lir) plain again: reported as a race with retain-slow's atomic add  case: ownership/110
#   pool-handoff  the ring slot of the work-stealing deque (rt/deque.lir) published and read with plain accesses instead of release/acquire (fib.sched-push, fib.sched-steal, fib.sched-pop; TSAN
#               ignores standalone fences, so the slot is the edge it sees): the task a thief takes is reported as a race with the owner's writes   kernel: sched tree
#   all         all five (default)
# usage: scripts/tsan-planted.sh [atomic-mt|tile-seed|enqueue-share|upgrade-atomic|pool-handoff|all]       environment: F (as tsan.sh), PLANT_OUT (scratch), TSAN_RUNS (default 5)
# exit: 0 every planted race was reported and the unplanted program was silent, 1 one was missed or the unplanted run reported it, 2 setup
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-all}
OUT=${PLANT_OUT:-$HOME/.cache/fibber-scratch/tsan-planted}
mkdir -p "$OUT"
export TSAN_RUNS=${TSAN_RUNS:-5} TSAN_BASELINE=/dev/null
K=$root/scripts/tsan/kernels.fib
bad=0

run() { # run LABEL PROGRAM ARGS..: the output of one tsan.sh --check run into $OUT/LABEL.out
  local label=$1 prog=$2; shift; shift
  TSAN_OUT=$OUT/$label "$root/scripts/tsan.sh" --check "$prog" "$@" > "$OUT/$label.out" 2>&1; local rc=$?
  [ $rc = 2 ] && { echo "SETUP-FAILED $label: $(tail -2 "$OUT/$label.out" | tr '\n' ' ')"; exit 2; }
  return 0
}
verdict() { # verdict NAME PLANTED-LABEL CLEAN-LABEL PATTERN
  if grep -q "^NEW.*$4" "$OUT/$2.out"; then planted=reported; else planted=MISSED; fi
  if grep -q "$4" "$OUT/$3.out"; then clean=REPORTED; else clean=silent; fi
  echo "$1: planted race $planted; unplanted $clean   [$(grep -m1 "^NEW.*$4" "$OUT/$2.out" | cut -c1-150)]"
  [ "$planted" = reported ] && [ "$clean" = silent ] || bad=1
}

if [ "$mode" = all ] || [ "$mode" = atomic-mt ]; then
  TSAN_SED='s/load atomic i32, ptr @fib\.mt monotonic, align 4/load i32, ptr @fib.mt, align 4/; s/store atomic i32 1, ptr @fib\.mt monotonic, align 4/store i32 1, ptr @fib.mt, align 4/' run mt-planted "$K" spawnjoin 300
  run mt-clean "$K" spawnjoin 300
  verdict atomic-mt mt-planted mt-clean "global 'fib.mt'"
fi
if [ "$mode" = all ] || [ "$mode" = tile-seed ]; then
  rm -rf "$OUT/lib"; cp -r "$root/lib" "$OUT/lib"
  # the scratch copy reads the seed inside the task again: the body of tile-window shadows its parameter with the old read
  sed -i -E 's/^  \(unsafe (\(Win .* seed\))\)\)$/  (let ((seed (array-get arr 0))) (unsafe \1)))/' "$OUT/lib/fib/view/tiles.fib"
  cmp -s "$OUT/lib/fib/view/tiles.fib" "$root/lib/fib/view/tiles.fib" && { echo "SETUP-FAILED tile-seed: the plant changed nothing"; exit 2; }
  FIB_LIB=$OUT/lib run tile-planted "$K" tiles 200000 4
  run tile-clean "$K" tiles 200000 4
  verdict tile-seed tile-planted tile-clean "tile-window"
fi
if [ "$mode" = all ] || [ "$mode" = enqueue-share ]; then
  A=$root/cases/ownership/32-await-inside-loop.fib
  TSAN_SED='/^define internal void @fib\.share-task\(/,/^}/{/call void @fib\.share\(ptr %t\)/d}' run enq-planted "$A"
  run enq-clean "$A"
  verdict enqueue-share enq-planted enq-clean "fib.release"
fi
if [ "$mode" = all ] || [ "$mode" = upgrade-atomic ]; then
  W=$root/cases/ownership/110-weak-and-atom-of-dyn-send-cross-threads.fib
  TSAN_SED='/^define internal ptr @fib\.upgrade\(/,/^}/s/load atomic i64, ptr %cp monotonic, align 8/load i64, ptr %cp, align 8/' run up-planted "$W"
  run up-clean "$W"
  verdict upgrade-atomic up-planted up-clean "fib.upgrade"
fi
if [ "$mode" = all ] || [ "$mode" = pool-handoff ]; then
  P=$root/scripts/tsan/sched.fib
  TSAN_SED='/^define internal void @fib\.sched-push\(/,/^}/s/store atomic ptr (%[a-z0-9.]+), (ptr %[a-z0-9.]+) release, align 8/store ptr \1, \2, align 8/; /^define internal ptr @fib\.sched-(steal|pop)\(/,/^}/s/load atomic ptr, (ptr %[a-z0-9.]+) acquire, align 8/load ptr, \1, align 8/' run pool-planted "$P" tree 12
  run pool-clean "$P" tree 12
  verdict pool-handoff pool-planted pool-clean "fib.sched"
fi
[ $bad = 0 ] && echo "tsan-planted: every planted race was reported" || echo "tsan-planted: FAILED"
exit $bad
