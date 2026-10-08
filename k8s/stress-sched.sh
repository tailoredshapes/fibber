#!/bin/bash
# The pool stress cases (8640-8648: the work-stealing pool's fork/join, trap, storm and blocking cases) repeated, at a fixed worker count: the weak-memory
# test of the Chase-Lev deque, which TSAN checks on x86 only (an aarch64 core may reorder what x86 does not). Run in a pod (make k8s-run) on the
# arm64 node, in the tree, with build/F built.
# usage: k8s/stress-sched.sh N THREADS      N rounds of the nine cases with FIB_THREADS=THREADS; prints one line per round and a total; exit 1 on any failure
set -u
n=${1:?N rounds}; threads=${2:?THREADS}
F=${FIBC:-build/F}
ok=0; bad=0
for i in $(seq 1 "$n"); do
  out=$(FIB_THREADS=$threads timeout 600 "$F" cases cases/stdlib --only 8640- 8641- 8642- 8643- 8644- 8645- 8646- 8647- 8648- -j 1 2>&1); code=$?
  line=$(grep -E '^[0-9]+ cases:' <<< "$out" || echo "no summary (exit $code)")
  if [ $code -eq 0 ]; then ok=$((ok + 1)); else bad=$((bad + 1)); echo "round $i FAILED (exit $code): $line"; grep -E 'FAIL|timed out' <<< "$out" | head -5; fi
  echo "round $i/$n threads=$threads: $line"
done
echo "stress-sched: $ok of $n rounds passed, $bad failed (FIB_THREADS=$threads, $(uname -m), $(nproc) cpus)"
[ "$bad" -eq 0 ]
