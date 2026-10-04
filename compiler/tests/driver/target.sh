#!/bin/bash
# SIMD P3: the code generation target. One decision (native.target/target-info: FIB_TARGET_CPU, else the host) gives the lane count of
# `:native` and `f32xn` to the checker and the `(target ..)` form to the emitted module, so that `emit` and `build` agree.
#   the first line of `emit` is the module's target form for the CPU named by FIB_TARGET_CPU (no features: the name stands for them)
#   unset, the target is the host's CPU with its feature list
#   `f32xn` is 4 lanes on x86-64 and x86-64-v2 (128 bits), 8 on x86-64-v3 (256 bits): the same program is accepted or rejected by the CPU
#   `f64xn` and `(Simd i8 :native)`: 2 / 4 and 16 / 32
# usage: target.sh STAGE2        (FIB_LIB must name lib/; run from anywhere)  Exit: 0 every check holds, 1 one fails.
s2=${1:?usage: target.sh STAGE2}
T=$(mktemp -d "${TMPDIR:-/tmp}/target.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: wanted [$3] got [$2]"; bad=1; fi; }
printf '(defun main () -> i64 7)\n' > "$T/plain.fib"
ck "x86-64: the target form is the first line" "$(FIB_TARGET_CPU=x86-64 "$s2" emit "$T/plain.fib" | head -1)" '(target (cpu "x86-64"))'
ck "x86-64-v3: the target form is the first line" "$(FIB_TARGET_CPU=x86-64-v3 "$s2" emit "$T/plain.fib" | head -1)" '(target (cpu "x86-64-v3"))'
host=$(env -u FIB_TARGET_CPU "$s2" emit "$T/plain.fib" | head -1)
case $host in
  '(target (cpu "'*'") (features "'*'"))') echo "ok   unset: the host's CPU and its features: ${host:0:60}..." ;;
  *) echo "FAIL unset: wanted the host's target form, got [$host]"; bad=1 ;;
esac
lanes() { # CPU TYPE LANES: a function that takes TYPE and returns the explicit vector of LANES; the program is checked (emit is rejected or not)
  printf '(defun f (v: %s) -> %s v)\n(defun main () -> i64 7)\n' "$2" "$3" > "$T/l.fib"
  FIB_TARGET_CPU=$1 "$s2" emit "$T/l.fib" > "$T/l.out" 2> "$T/l.err"; echo $?
}
want() { # CPU TYPE EXPLICIT-TYPE VERDICT(same|differ)
  local st; st=$(lanes "$1" "$2" "$3")
  if [ "$4" = same ]; then
    case $st in 3) echo "FAIL $1: $2 should be $3: $(head -c 200 "$T/l.err")"; bad=1 ;; *) echo "ok   $1: $2 is $3 (accepted)" ;; esac
  else
    case $st in 3) echo "ok   $1: $2 is not $3 (rejected)" ;; *) echo "FAIL $1: $2 should not be $3, status $st"; bad=1 ;; esac; fi
}
for cpu in x86-64 x86-64-v2; do
  want $cpu f32xn "(Simd f32 4)" same;  want $cpu f32xn "(Simd f32 8)" differ
  want $cpu f64xn "(Simd f64 2)" same;  want $cpu "(Simd i8 :native)" "(Simd i8 16)" same
done
want x86-64-v3 f32xn "(Simd f32 8)" same;  want x86-64-v3 f32xn "(Simd f32 4)" differ
want x86-64-v3 f64xn "(Simd f64 4)" same; want x86-64-v3 "(Simd i8 :native)" "(Simd i8 32)" same
[ $bad -eq 0 ] && echo "target: every check holds"
exit $bad
