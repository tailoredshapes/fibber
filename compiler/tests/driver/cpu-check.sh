#!/bin/bash
# docs/adr/0008: an x86-64 Linux executable checks at start that the CPU has the features it was built for (emit.cpucheck) and otherwise ends with
#   trap: this program needs CPU (AVX2, FMA); this CPU lacks: ..
# (status 134) instead of SIGILL somewhere later. Checked here:
#   1. a program built for the host and one built for x86-64-v3 run (this host has AVX2 and FMA); both carry the check (glibc's
#      __x86_get_cpuid_feature_leaf is imported); so does SHIPPED, a compiler built by a stage 2 that makes the check (package.sh's bin/fibc, or
#      the gate's F3), when it is given: STAGE2 itself may be built by an older seed
#   2. planted requirement: built for x86-64-v4 (AVX-512, which this host lacks), the program traps at start with the exact message, before main's body
#      prints anything (skipped on a host that has avx512f)
#   3. under qemu-x86_64 -cpu Westmere (no AVX), when qemu is installed, the x86-64-v3 program traps with the message instead of SIGILL: the check
#      itself runs on the CPU it refuses
#   4. no check for baseline x86-64 (it needs nothing beyond SSE2) or for an aarch64 object
# usage: cpu-check.sh STAGE2 [SHIPPED]        (FIB_LIB names lib/)  Exit: 0 every check holds, 1 one fails.
s2=${1:?usage: cpu-check.sh STAGE2 [SHIPPED]}
shipped=${2:-}
T=$(mktemp -d "${TMPDIR:-/tmp}/cpu-check.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; bad=1; }
cat > "$T/h.fib" <<'EOF2'
(ns main (:use fib.core))
(defun main () -> i64 (do (println "hello") 0))
EOF2
has_check() { nm -D "$1" 2>/dev/null | grep -q '__x86_get_cpuid_feature_leaf'; }
for cpu in host x86-64-v3; do
  if FIB_TARGET_CPU=$cpu "$s2" build "$T/h.fib" -o "$T/h-$cpu" 2> "$T/e"; then
    out=$("$T/h-$cpu" 2>&1); st=$?
    [ $st = 0 ] && [ "$out" = hello ] && ok "$cpu: the check passes on this host and the program runs" || no "$cpu: status $st, output [$(echo "$out" | head -c 200)]"
    has_check "$T/h-$cpu" && ok "$cpu: the program carries the check" || no "$cpu: no __x86_get_cpuid_feature_leaf import: no check"
  else no "$cpu build: $(head -c 300 "$T/e")"; fi
done
if [ -n "$shipped" ]; then
  has_check "$shipped" && "$shipped" --version > /dev/null 2>&1 && ok "the compiler $shipped carries the check and passes it: $("$shipped" --version)" \
    || no "the compiler $shipped has no start-up check, or does not start"
else echo "skip the compiler's own check: no SHIPPED compiler given"; fi
if grep -qw avx512f /proc/cpuinfo; then echo "skip planted x86-64-v4: this host has avx512f"
elif FIB_TARGET_CPU=x86-64-v4 "$s2" build "$T/h.fib" -o "$T/h-v4" 2> "$T/e"; then
  "$T/h-v4" > "$T/o" 2> "$T/r"; st=$?
  want="trap: this program needs x86-64-v4 (AVX2, FMA, AVX512F); this CPU lacks: AVX512F, AVX512BW, AVX512CD, AVX512DQ, AVX512VL"
  [ $st = 134 ] && [ "$(cat "$T/r")" = "$want" ] && [ ! -s "$T/o" ] && ok "planted x86-64-v4 requirement: traps at start (134): $(cat "$T/r")" \
    || no "planted x86-64-v4: status $st, stderr [$(head -c 300 "$T/r")], stdout [$(head -c 80 "$T/o")]"
else no "x86-64-v4 build: $(head -c 300 "$T/e")"; fi
if command -v qemu-x86_64 >/dev/null 2>&1 && [ -x "$T/h-x86-64-v3" ]; then
  qemu-x86_64 -cpu Westmere "$T/h-x86-64-v3" > "$T/o" 2> "$T/r"; st=$?
  grep -q '^trap: this program needs x86-64-v3 (AVX2, FMA); this CPU lacks: AVX2, FMA, AVX, ' "$T/r" && [ ! -s "$T/o" ] \
    && ok "qemu Westmere: the v3 program traps with the message (status $st): $(head -c 160 "$T/r")" \
    || no "qemu Westmere: status $st, stderr [$(head -c 300 "$T/r")], stdout [$(head -c 80 "$T/o")]"
else echo "skip qemu Westmere: no qemu-x86_64"; fi
if FIB_TARGET_CPU=x86-64 "$s2" build "$T/h.fib" -o "$T/h-v1" 2> "$T/e"; then
  has_check "$T/h-v1" && no "x86-64 (baseline) carries a check it does not need" || ok "x86-64 (baseline): no check"
else no "x86-64 build: $(head -c 300 "$T/e")"; fi
if "$s2" build --target aarch64-unknown-linux-gnu "$T/h.fib" -o "$T/h-a64.o" --emit obj 2> "$T/e"; then
  nm "$T/h-a64.o" | grep -q __x86_get_cpuid_feature_leaf && no "the aarch64 object carries the x86 check" || ok "aarch64: no check (FMA is baseline)"
else no "aarch64 emit: $(head -c 300 "$T/e")"; fi
[ $bad -eq 0 ] && echo "cpu-check: every check holds"
exit $bad
