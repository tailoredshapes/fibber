#!/bin/bash
# compiler/tests/driver/host-threads.sh: a fibber library called by several host threads at once (docs/design/host-threads.md).
# The library (hostthreads/kernels.fib) is built the way an embedder builds one: `fibc build --emit obj --export ..` and `cc -shared`.
# The host (hostthreads/host.c) starts N threads that wait at a barrier and then call at once, 2000 rounds each, checking every answer.
#   4 threads, 16 threads, each 12 runs: every run exits 0 (a crash is 139, a trap 134, a wrong answer 1)
#   the same with the main thread calling first (the runtime is started by a thread that is not one of the callers)
# usage: host-threads.sh STAGE2        (run from the repository root; FIB_LIB defaults to the tree's lib)
set -uo pipefail
s2=${1:?usage: host-threads.sh STAGE2}
root=$PWD; here=$root/compiler/tests/driver/hostthreads
export FIB_LIB=${FIB_LIB:-$root/lib}
ulimit -v 12000000
mkdir -p "$HOME/.cache/fibber-scratch"
t=$(mktemp -d "$HOME/.cache/fibber-scratch/host-threads.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0
timeout 300 "$s2" build "$here/kernels.fib" -I "$FIB_LIB" --emit obj -o "$t/k.o" --export add --export sum-squares --export text-len > "$t/build.out" 2>&1 \
  || { echo "FAIL  build"; cat "$t/build.out"; exit 1; }
cc -shared -o "$t/libk.so" "$t/k.o" -lpthread -lm && cc -O2 -o "$t/host" "$here/host.c" -ldl -lpthread || { echo "FAIL  cc"; exit 1; }
runs() { # NAME THREADS EXTRA..: 12 runs, each must exit 0
  local name=$1 n=$2 bad=0 codes="" rc; shift 2
  for _ in $(seq 12); do
    timeout 120 "$t/host" "$t/libk.so" "$n" "$@" > "$t/out" 2>&1; rc=$?
    [ $rc -eq 0 ] || { bad=$((bad+1)); codes="$codes $rc"; }
  done
  if [ $bad -eq 0 ]; then echo "ok    $name"; else echo "FAIL  $name: $bad of 12 runs failed (exit codes:$codes)"; fail=1; fi
}
runs "4 threads at once, cold start" 4
runs "16 threads at once, cold start" 16
runs "4 threads at once, main thread called first" 4 main-first
runs "16 threads at once, main thread called first" 16 main-first
exit $fail
