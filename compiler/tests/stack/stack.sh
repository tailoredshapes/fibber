#!/bin/bash
# The stack guard and the main stack (docs/design/stack.md) on the ahead-of-time path: the cases of cases/stdlib 8350-8354 run the same programs under
# `fibc run` (the JIT); this script builds them with `fibc build` and checks the executable's own status and standard error, which no case header can.
#   compiler/tests/stack/stack.sh FIBC [SCRATCH]      FIBC names a stage 2 built from this tree TWICE over (the runtime is embedded: F builds F2, F2's programs
#                                                    and F2 itself have the new runtime); exit 0 when every check holds, 1 otherwise.
# Checks (each prints `ok N text` or `FAIL N text`):
#   1-3  8350 (main), 8351 (a task), 8352 (under try): status 134 and `trap: stack overflow` on standard error
#   4    a read of a PROT_NONE page (not a stack): status 139 (SIGSEGV), `fatal signal 11 at 0x` and not the word `stack`
#   5    a recursion of two million frames runs under the default stack (256 MiB) and answers; status is the low byte of the sum
#   6    FIB_STACK_MB=0 (the process's own stack, 8 MiB here): the same recursion is a stack overflow, with the message and status 134
#   7    FIB_STACK_MB=2: a recursion of 200,000 frames (more than 2 MiB holds) is a stack overflow; FIB_STACK_MB=64: it runs
#   8    FIB_STACK_MB=abc and =-1 and =9999999999: a fatal error that names FIB_STACK_MB, status 134
#   9    a SIGBUS (a mapped file cut short) is reported as `fatal signal 7`, not as a stack overflow
#   10   with FIB_MUSL_DIR set: a static musl executable overflows with the message, and runs a deep recursion under FIB_STACK_MB=64
set -u
FIBC=${1:?usage: stack.sh FIBC [SCRATCH]}
R=$(cd "$(dirname "$0")/../../.." && pwd)
S=${2:-$HOME/.cache/fibber-scratch/stack-test}
mkdir -p "$S"; cd "$R" || exit 2
export FIB_LIB=$R/lib
ulimit -v 16000000
ulimit -s 8192
bad=0; n=0
say() { n=$((n + 1)); if [ "$1" = ok ]; then echo "ok $n $2"; else echo "FAIL $n $2"; bad=1; fi; }
build() { "$FIBC" build "$1" -I lib -o "$2" > "$2.log" 2>&1 || { echo "FAIL build $1: $(tail -n 2 "$2.log")"; bad=1; return 1; }; }
# expect_trap NAME EXE STATUS TEXT [ENV..]: the run's status and standard error
expect() {
  local what=$1 exe=$2 want=$3 text=$4; shift 4
  env "$@" "$exe" > "$exe.out" 2> "$exe.err"; local rc=$?
  if [ "$rc" = "$want" ] && { [ -z "$text" ] || grep -qF -- "$text" "$exe.err"; }; then say ok "$what: status $rc, \"$text\""
  else say FAIL "$what: status $rc (want $want), stderr: $(head -c 200 "$exe.err")"; fi
}
for c in 8350-trap-stack-overflow-in-main-is-a-trap-not-a-silent-exit:main 8351-trap-stack-overflow-in-a-task-ends-the-process:task \
         8352-trap-stack-overflow-under-try-is-fatal-and-says-so:try; do
  f=cases/stdlib/${c%%:*}.fib; k=${c##*:}
  build "$f" "$S/$k" && expect "overflow in $k (AOT)" "$S/$k" 134 "trap: stack overflow" X=1
done
cat > "$S/wild.fib" <<'EOF'
(ns main)
(extern mmap :private (i64 i64 i32 i32 i32 i64) -> ptr)
(defun main () -> i64 (unsafe (load-i64 (mmap 0 4096 0i32 34i32 -1i32 0))))
EOF
build "$S/wild.fib" "$S/wild" && { expect "wild read (AOT)" "$S/wild" 139 "fatal signal 11 at 0x" X=1
  grep -q "stack" "$S/wild.err" && say FAIL "wild read said stack" || say ok "wild read does not say stack"; }
build cases/stdlib/8353-deep-recursion-of-two-million-frames-runs.fib "$S/deep" && {
  "$S/deep" > /dev/null 2> "$S/deep.err"; rc=$?
  [ "$rc" = $((2000001000000 % 256)) ] && [ ! -s "$S/deep.err" ] && say ok "two million frames run (AOT): status $rc" || say FAIL "two million frames: status $rc, $(head -c 100 "$S/deep.err")"
  expect "FIB_STACK_MB=0 is the process stack (8 MiB)" "$S/deep" 134 "trap: stack overflow" FIB_STACK_MB=0; }
cat > "$S/depthn.fib" <<'EOF'
(ns main (:use fib.core))
(defun depth (n: i64) -> i64 (if (= n 0) 0 (+ 1 (depth (- n 1)))))
(defun main () -> i64 (match (sys-getenv "N") ((some s) (match (parse-long s) ((some n) (do (println (depth n)) 0)) (nil 1))) (nil 2)))
EOF
build "$S/depthn.fib" "$S/depthn" && {
  expect "FIB_STACK_MB=2, 200000 frames" "$S/depthn" 134 "trap: stack overflow" FIB_STACK_MB=2 N=200000
  expect "FIB_STACK_MB=64, 200000 frames" "$S/depthn" 0 "" FIB_STACK_MB=64 N=200000
  for v in abc -1 9999999999 ""; do expect "FIB_STACK_MB='$v'" "$S/depthn" 134 "FIB_STACK_MB is not a whole number" FIB_STACK_MB="$v" N=10; done; }
cat > "$S/bus.fib" <<'EOF'
(ns main)
(extern mmap :private (i64 i64 i32 i32 i32 i64) -> ptr)
(extern ftruncate :private (i32 i64) -> i32)
(extern open :private (ptr i32 i32) -> i32)
(extern unlink :private (ptr) -> i32)
(defun main () -> i64
  (unsafe
    (let ((path (alloc 32)))
      (do (dotimes (i 12) (store-i8 (ptr+ path i) (trunc i8 (nth [47 116 109 112 47 98 117 115 46 116 109 112] i))))
          (store-i8 (ptr+ path 12) (trunc i8 0))
          (let ((fd (open path 66i32 384i32)))
            (do (ftruncate fd 4096)
                (let ((m (mmap 0 8192 3i32 1i32 fd 0)))
                  (do (ftruncate fd 0) (unlink path) (load-i64 m)))))))))
EOF
if build "$S/bus.fib" "$S/bus"; then
  expect "SIGBUS is not a stack overflow" "$S/bus" 135 "fatal signal 7 at 0x" X=1
fi
# a system that will not make the 256 MiB thread (an address-space limit) runs the program on the process's own stack, and still says so on overflow
( ulimit -v 200000
  N=1 "$S/depthn" > /dev/null 2>&1 && echo "ok fallback: the program runs under ulimit -v 200000" || echo "FAIL fallback: no run under ulimit -v 200000"
  env N=100000000 "$S/depthn" > /dev/null 2> "$S/fb.err"; [ $? = 134 ] && grep -q "trap: stack overflow" "$S/fb.err" && echo "ok fallback: overflow on the process's own stack is still the message" || echo "FAIL fallback overflow" ) | tee "$S/fb.res"
n=$((n + 2)); grep -q FAIL "$S/fb.res" && bad=1
# a static musl executable (docs/design/static-linking.md): only when the musl pieces are at FIB_MUSL_DIR
if [ -n "${FIB_MUSL_DIR:-}" ]; then
  if FIB_STATIC=1 "$FIBC" build cases/stdlib/8350-trap-stack-overflow-in-main-is-a-trap-not-a-silent-exit.fib -I lib -o "$S/main.static" > "$S/static.log" 2>&1; then
    expect "overflow in main (static musl)" "$S/main.static" 134 "trap: stack overflow" X=1
    FIB_STATIC=1 "$FIBC" build "$S/depthn.fib" -I lib -o "$S/depthn.static" > /dev/null 2>&1 && expect "FIB_STACK_MB=64, 3 million frames (static musl)" "$S/depthn.static" 0 "" FIB_STACK_MB=64 N=3000000
  else say FAIL "static build: $(tail -n 2 "$S/static.log")"; fi
else echo "skip static musl (FIB_MUSL_DIR is not set)"; fi
echo "stack: $n checks, $([ $bad = 0 ] && echo all hold || echo SOME FAILED)"
exit $bad
