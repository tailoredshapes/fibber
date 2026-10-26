#!/bin/bash
# compiler/tests/live/lir.sh [FIBC]: a body swap of a function that holds a `lir` form (docs/design/inline-lir.md: a live swap re-emits a changed site). A program loops printing `answer N`,
# `answer` being a lir form that adds 1 to a cell; the terminal client redefines it with a lir form that adds 100; the running program must print the new value, and a redefinition that
# keeps the same lir text must leave it alone. Exit 0 when every check holds.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: lir.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
out=${TMPDIR:-/tmp}/live-lir-$$; mkdir -p "$out"; pid=; trap '[ -n "$pid" ] && kill "$pid" 2>/dev/null; rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok $1"; else echo "FAIL $1: got [$2] want [$3]"; bad=1; fi; }
cat > "$out/app.fib" <<'EOT'
(ns main (:use fib.core) (:require [fib.os.time :as t]))
(defun answer (p: ptr) -> i64
  (unsafe (lir [p: ptr] -> i64 "(BLOCK entry (RET (ADD (ATOMIC-LOAD ACQUIRE I64 p) (I64 1))))")))
(defun main () -> i64
  (unsafe (let [cell (alloc 8)]
    (do (store-i64 cell 10)
        (loop [i 0]
          (if (< i 120)
              (do (println "answer" (answer cell)) (t/sleep-ns 100000000) (recur (+ i 1)))
              0))))))
EOT
"$fibc" run --live "$out/app.fib" > "$out/stdout" 2> "$out/stderr" &
pid=$!
for _ in $(seq 100); do grep -q "live at" "$out/stderr" 2>/dev/null && break; sleep 0.1; done
last() { tail -n 1 "$out/stdout"; }
for _ in $(seq 50); do [ "$(last)" = "answer 11" ] && break; sleep 0.1; done
ck "the program prints the first body's value" "$(last)" "answer 11"
swap() { printf '%s\n' "(defun answer (p: ptr) -> i64 (unsafe (lir [p: ptr] -> i64 \"(BLOCK entry (RET (ADD (ATOMIC-LOAD ACQUIRE I64 p) (I64 $1))))\")))" | timeout 60 "$fibc" repl --connect "$pid" 2>&1 | grep -c "swapped answer"; }
ck "a redefinition with another lir body is swapped" "$(swap 100)" 1
for _ in $(seq 50); do [ "$(last)" = "answer 110" ] && break; sleep 0.1; done
ck "and the running program takes the new body" "$(last)" "answer 110"
ck "a redefinition with another body again is swapped" "$(swap 7)" 1
for _ in $(seq 50); do [ "$(last)" = "answer 17" ] && break; sleep 0.1; done
ck "and takes that one" "$(last)" "answer 17"
exit "$bad"
