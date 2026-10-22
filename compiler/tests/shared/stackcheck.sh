#!/bin/bash
# compiler/tests/shared/stackcheck.sh STAGE2 [library|planted|depth]..: the stack check in a shared library and in an ordinary program that catches
# (docs/design/stack-check.md). Linux x86-64; run from the repository root.
#   library   stackfns.fib built as a library with its header (which must declare fib_status and fib_error), stackhost.c calls the recursions past the stack of the
#             host's main thread, of a second thread, of threads with 256 KiB and 64 MiB stacks, of 8 threads at once and of 60 started in turn: every call returns,
#             `fib_status` says 1 and `fib_error` says `stack overflow`, the call after answers, an allocating recursion is the same, another trap is a status too
#   planted   the same library built with FIB_STACK_CHECK=0 (no check emitted): the host must NOT pass (it dies of the overflow), or the test above cannot fail
#   depth     the deepest recursion that completes, with the check and without it (a program that catches, a main stack of 256 MiB): the check takes no more than its
#             margin (256 KiB) of it, and 100,000 frames and a million frames answer
set -uo pipefail
s2=${1:?usage: stackcheck.sh STAGE2 [part..]}; shift
s2=$(cd "$(dirname "$s2")" && pwd)/$(basename "$s2")
root=$PWD; export FIB_LIB=${FIB_LIB:-$root/lib}
here=$root/compiler/tests/shared
parts=${*:-library planted depth}
ulimit -v 12000000
scratch=${FIB_SCRATCH:-${TMPDIR:-$HOME/.cache/fibber-scratch}}; mkdir -p "$scratch"
t=$(mktemp -d "$scratch/stackcheck.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0
part() { case " $parts " in *" $1 "*) return 0;; *) return 1;; esac; }
ex=(--export depth --export grow --export keep-and-dive=keep_and_dive --export label --export idx --export deep-unit=deep_unit)
cc -O1 -o "$t/host" "$here/stackhost.c" -ldl -lpthread || { echo "FAIL  cc"; exit 1; }
if part library; then
  echo "== library"
  timeout 600 "$s2" build "$here/stackfns.fib" -I "$root/lib" --shared --header "$t/stackfns.h" "${ex[@]}" -o "$t/libstackfns.so" > "$t/build.out" 2>&1 || { echo "FAIL  build"; cat "$t/build.out"; exit 1; }
  for sym in "int32_t fib_status(void);" "int64_t fib_error(uint8_t* out, int64_t out_cap);"; do
    if grep -qF "$sym" "$t/stackfns.h"; then echo "ok    header declares $sym"; else echo "FAIL  header lacks $sym"; fail=1; fi
  done
  syms=$(nm -D --defined-only "$t/libstackfns.so")
  if printf '%s\n' "$syms" | grep -q ' T fib_status$'; then echo "ok    fib_status is exported"; else echo "FAIL  fib_status not exported"; fail=1; fi
  timeout 300 "$t/host" "$t/libstackfns.so" > "$t/host.out" 2>&1; rc=$?
  cat "$t/host.out"
  if [ $rc -eq 0 ] && ! grep -q '^FAIL' "$t/host.out"; then echo "ok    the host survived every overflow (exit 0)"; else echo "FAIL  host exit $rc"; fail=1; fi
fi
if part planted; then
  echo "== planted (the check is not emitted)"
  FIB_STACK_CHECK=0 timeout 600 "$s2" build "$here/stackfns.fib" -I "$root/lib" --shared "${ex[@]}" -o "$t/libplanted.so" > "$t/pbuild.out" 2>&1 || { echo "FAIL  planted build"; cat "$t/pbuild.out"; exit 1; }
  timeout 300 "$t/host" "$t/libplanted.so" > "$t/planted.out" 2>&1; rc=$?
  if [ $rc -ne 0 ]; then echo "ok    without the check the host does not survive (exit $rc)"; else echo "FAIL  the planted fault was not caught by the host test"; fail=1; fi
fi
if part depth; then
  echo "== depth"
  timeout 600 "$s2" build "$here/depthprobe.fib" -I "$root/lib" -o "$t/probe-on" > "$t/d1.out" 2>&1 || { echo "FAIL  build probe"; cat "$t/d1.out"; exit 1; }
  FIB_CATCH=all FIB_STACK_CHECK=0 timeout 600 "$s2" build "$here/depthprobe.fib" -I "$root/lib" -o "$t/probe-off" > "$t/d2.out" 2>&1 || { echo "FAIL  build probe (off)"; exit 1; }
  # probe-on is a program that catches (it uses try), so it has the check; probe-off is the same program compiled without it
  ok() { [ "$("$1" "$2" 2>/dev/null | tail -n 1)" = "$2" ]; }
  deepest() { local lo=1000000 hi=60000000 mid; while [ $((hi-lo)) -gt 20000 ]; do mid=$(((lo+hi)/2)); if ok "$1" $mid; then lo=$mid; else hi=$mid; fi; done; echo $lo; }
  if ok "$t/probe-on" 100000 && ok "$t/probe-on" 1000000; then echo "ok    100000 and 1000000 frames answer with the check"; else echo "FAIL  a depth that fits did not answer"; fail=1; fi
  r=$("$t/probe-on" 1000000000 2>&1 | tail -n 1)
  if [ "$r" = "stack overflow" ]; then echo "ok    a recursion past the stack is caught: [$r]"; else echo "FAIL  got [$r]"; fail=1; fi
  don=$(deepest "$t/probe-on"); doff=$(deepest "$t/probe-off")
  echo "deepest recursion that completes: with the check $don frames, without $doff frames (bisected to 20000)"
  # the margin is 256 KiB of a frame of at least 16 bytes: at most 16384 frames; the bisection step is 20000
  if [ $((doff - don)) -le 40000 ] && [ $((doff - don)) -ge -40000 ]; then echo "ok    the check costs no more depth than its margin"; else echo "FAIL  the depths differ by $((doff-don))"; fail=1; fi
fi
exit $fail
