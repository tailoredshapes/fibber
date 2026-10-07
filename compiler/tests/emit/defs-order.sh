#!/bin/bash
# The order in which `def`s are made, on the ahead-of-time path (LANG-2 item 4): cases 8367 and 8368 run under `fibc run`; this builds them with `fibc build`
# and runs the executables, which must exit 0 (a case's `main` answers 0 when every check holds), and a program whose only `def`s are a constant and a made one
# that reads it must answer 66. Before the fix `fibc build` ended with `unsupported: def base is read where it has no value`.
#   compiler/tests/emit/defs-order.sh FIBC [SCRATCH]       exit 0 when all hold
set -u
FIBC=${1:?usage: defs-order.sh FIBC [SCRATCH]}
R=$(cd "$(dirname "$0")/../../.." && pwd); S=${2:-$HOME/.cache/fibber-scratch/defs-order}
mkdir -p "$S"; cd "$R" || exit 2
export FIB_LIB=$R/lib
ulimit -v 16000000
bad=0
build_run() { # NAME SRC WANT [-I dir..]
  local name=$1 src=$2 want=$3; shift 3
  if ! "$FIBC" build "$src" -o "$S/$name" "$@" > "$S/$name.log" 2>&1; then echo "FAIL $name: $(head -c 200 "$S/$name.log")"; bad=1; return; fi
  "$S/$name" > "$S/$name.out" 2>&1; local rc=$?
  if [ "$rc" = "$want" ]; then echo "ok $name (AOT): status $rc"; else echo "FAIL $name: status $rc, want $want: $(head -c 200 "$S/$name.out")"; bad=1; fi
}
build_run defs-8367 cases/stdlib/8367-a-def-reads-the-defs-before-it-through-calls-and-functions.fib 0 -I cases/stdlib/support
build_run defs-8368 cases/stdlib/8368-defs-read-defs-of-required-modules/main.fib 0 -I cases/stdlib/support
cat > "$S/two.fib" <<'EOF'
(ns main)
(def base: i64 7)
(def twice: i64 (* base 2))
(def thrice: i64 (+ twice base base))
(defun main () -> i64 (+ twice thrice base 17))
EOF
build_run two "$S/two.fib" 66
exit $bad
