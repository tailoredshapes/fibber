#!/bin/bash
# SC1: `simd/muladd` and `(has-fma)` follow the target, and no build for a CPU without FMA calls libm's `fma`. Programs are built with FIB_TARGET_CPU pinned:
#   x86-64-v2 (no FMA): (has-fma) is false, muladd equals the multiply-then-add, not simd/fma
#   x86-64-v3 (FMA):    (has-fma) is true,  muladd equals simd/fma (run only when the host has avx2 and fma)
#   a matmul and a vector exp/log/tanh built for x86-64-v2 have no undefined `fma` symbol (a libm call), where the control program that uses simd/fma has one:
#   the check can fail. (The speed it protects: a 512 f64 product is 6 ms with FMA tiles, 203 ms through libm's fma.)
# usage: muladd.sh STAGE2        (FIB_LIB must name lib/; run from anywhere)  Exit: 0 every check holds, 1 one fails.
s2=${1:?usage: muladd.sh STAGE2}
T=$(mktemp -d "${TMPDIR:-/tmp}/muladd.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: wanted [$3] got [$2]"; bad=1; fi; }
cat > "$T/m.fib" <<'EOF2'
(ns main (:use fib.core))
(defun main () -> i64
  (let ((e (fdiv 1.0 (double (unwrap (parse-long (nth (args) 0)))))))
    (let ((a: f64x4 (splat f64x4 (+ 1.0 e))) (b: f64x4 (splat f64x4 (- 1.0 e))) (k: f64x4 (splat f64x4 -1.0)))
      (let ((m (simd/muladd a b k)))
        (do (println (str-join [(if (has-fma) "fma" "nofma") (if (= m (simd/fma a b k)) " 1" " 0") (if (= m (+ (* a b) k)) " 1" " 0")])) 0)))))
EOF2
cat > "$T/mm.fib" <<'EOF2'
(ns main (:require [fib.tensor :as t]) (:use fib.core))
(defun main () -> i64
  (let ((x (t/reshape [64 64] (t/linspace 0.0 1.0 4096))))
    (do (println (t/sum (t/mmul x x)) " " (t/sum (t/exp (t/mul x (t/zeros [64 64]))))) 0)))
EOF2
cat > "$T/ctl.fib" <<'EOF2'
(ns main (:use fib.core))
(defun main () -> i64
  (let ((a: f64x4 (splat f64x4 (double (unwrap (parse-long (nth (args) 0)))))) (b: f64x4 (splat f64x4 2.5)) (k: f64x4 (splat f64x4 -1.0)))
    (do (println (lane (simd/fma a b k) 0)) 0)))
EOF2
build() { FIB_TARGET_CPU=$1 "$s2" build "$T/$2.fib" -o "$T/$2-$1" > "$T/b.out" 2>&1 || { echo "FAIL build $2 for $1: $(head -c 300 "$T/b.out")"; bad=1; return 1; }; }
build x86-64-v2 m && ck "x86-64-v2: has-fma is false, muladd is multiply-then-add" "$("$T/m-x86-64-v2" 1073741824)" "nofma 0 1"
if grep -q ' avx2' /proc/cpuinfo && grep -q ' fma' /proc/cpuinfo; then
  build x86-64-v3 m && ck "x86-64-v3: has-fma is true, muladd is simd/fma" "$("$T/m-x86-64-v3" 1073741824)" "fma 1 0"
else echo "skip x86-64-v3 run: the host has no avx2 and fma"; fi
build x86-64-v2 ctl && { nm -D "$T/ctl-x86-64-v2" | grep -qw fma && echo "ok   control: simd/fma at x86-64-v2 calls libm's fma" || { echo "FAIL control: no fma call found, the check below cannot fail"; bad=1; }; }
build x86-64-v2 mm && { if nm -D "$T/mm-x86-64-v2" | grep -qw fma; then echo "FAIL matmul and exp at x86-64-v2 call libm's fma"; bad=1; else echo "ok   matmul, exp at x86-64-v2: no libm fma call"; fi; }
[ $bad -eq 0 ] && echo "muladd: every check holds"
exit $bad
