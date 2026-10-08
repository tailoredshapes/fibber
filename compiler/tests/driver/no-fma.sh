#!/bin/bash
# linux-only-file: x86-64-v2/v3 runs and /proc/cpuinfo; the Mac is arm64 (skipped: compiler/tests/expected-macos.txt)
# docs/adr/0008: `simd/fma` (exact, one rounding) on a target without FMA hardware is a compile-time warning at each call site, once, and a trap
# with the same text at run time; never a libm call or an emulation. `simd/muladd` stays legal everywhere, and a `(has-fma)` dispatch on such a
# target does not lower its fused branch (no warning for it). Checked here:
#   1. FIB_TARGET_CPU=x86-64-v3 (FMA): no warning; the program runs and prints the fused result.
#   2. --target aarch64-unknown-linux-gnu --emit obj (FMA): no warning.
#   3. --target wasm32-wasip1 --emit obj (no FMA): one warning per call site, with FILE:LINE:COL and the exact text, the generic site once
#      although it is lowered for f64x4 and for f32x4.
#   4. FIB_TARGET_CPU=x86-64 (no FMA, a test-only CPU below the x86-64-v3 baseline): the same warnings "on x86-64"; the executable traps with
#      `trap: simd/fma has no hardware support on x86-64; ..` (status 134) and has no undefined `fma` (libm) symbol, where a control program
#      that calls libm's fma through an extern has one (the symbol check can fail).
#   5. simd/muladd alone, and a (has-fma) dispatch, at x86-64 and on wasm32: no warning, and the x86-64 build runs.
# compiler/tests/driver/no-fma-faults.sh plants the two faults this must catch (no warning; a libm call in place of the trap).
# usage: no-fma.sh STAGE2        (FIB_LIB names lib/; run from anywhere)  Exit: 0 every check holds, 1 one fails.
s2=${1:?usage: no-fma.sh STAGE2}
T=$(mktemp -d "${TMPDIR:-/tmp}/no-fma.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; bad=1; }
cat > "$T/f.fib" <<'EOF2'
(ns main (:use fib.core))
(defun twice (a: t b: t c: t) :where ((Float t)) -> t
  (simd/fma a b c))
(defun main () -> i64
  (let ((a: f64x4 (splat f64x4 (double (unwrap (parse-long (nth (args) 0)))))) (b: f64x4 (splat f64x4 2.5)) (k: f64x4 (splat f64x4 -1.0))
        (x: f32x4 (splat f32x4 1.5f32)))
    (do (println (lane (simd/fma a b k) 0) (lane (twice a b k) 1) (lane (twice x x x) 2))
        0)))
EOF2
cat > "$T/m.fib" <<'EOF2'
(ns main (:use fib.core))
(defun main () -> i64
  (let ((a: f64x4 (splat f64x4 3.0)) (b: f64x4 (splat f64x4 2.5)) (k: f64x4 (splat f64x4 -1.0)))
    (do (println (lane (simd/muladd a b k) 0) (lane (if (has-fma) (simd/fma a b k) (simd/muladd a b k)) 1)
                 (if (and (has-fma) true) (lane (simd/fma a b k) 2) 6.5))
        0)))
EOF2
cat > "$T/ctl.fib" <<'EOF2'
(ns main (:use fib.core))
(extern fma (f64 f64 f64) -> f64)
(defun main () -> i64
  (do (println (unsafe (fma (double (unwrap (parse-long (nth (args) 0)))) 2.5 -1.0))) 0))
EOF2
msg() { echo "simd/fma has no hardware support on $1; it traps at run time: use simd/muladd for portable code"; }
want_sites() { printf '%s\n' "$T/f.fib:3:3: warning: $(msg "$1")" "$T/f.fib:7:24: warning: $(msg "$1")"; }
warnings() { grep 'warning:' "$1" | LC_ALL=C sort; }
# 1. x86-64-v3
if FIB_TARGET_CPU=x86-64-v3 "$s2" build "$T/f.fib" -o "$T/f-v3" 2> "$T/e-v3"; then
  [ -z "$(warnings "$T/e-v3")" ] && ok "x86-64-v3: no warning" || no "x86-64-v3 warned: $(warnings "$T/e-v3" | head -2)"
  if grep -q ' fma' /proc/cpuinfo && grep -q ' avx2' /proc/cpuinfo; then
    out=$("$T/f-v3" 3 2>&1); [ "$out" = "6.5 6.5 3.75" ] && ok "x86-64-v3: runs, fused: $out" || no "x86-64-v3 run: [$out]"
  else echo "skip x86-64-v3 run: the host has no avx2 and fma"; fi
else no "x86-64-v3 build failed: $(head -c 300 "$T/e-v3")"; fi
# 2. aarch64
if "$s2" build --target aarch64-unknown-linux-gnu "$T/f.fib" -o "$T/f-a64.o" --emit obj 2> "$T/e-a64"; then
  [ -z "$(warnings "$T/e-a64")" ] && ok "aarch64: no warning" || no "aarch64 warned: $(warnings "$T/e-a64" | head -2)"
else no "aarch64 emit failed: $(head -c 300 "$T/e-a64")"; fi
# 3. wasm32
if "$s2" build --target wasm32-wasip1 "$T/f.fib" -o "$T/f-w.o" --emit obj 2> "$T/e-w"; then
  [ "$(warnings "$T/e-w")" = "$(want_sites wasm32-wasip1 | LC_ALL=C sort)" ] && ok "wasm32-wasip1: one warning per call site: $(warnings "$T/e-w" | head -1 | sed "s|$T/||")" \
    || no "wasm32-wasip1 warnings: [$(warnings "$T/e-w" | sed "s|$T/||" | tr '\n' '|')]"
else no "wasm32 emit failed: $(head -c 300 "$T/e-w")"; fi
# 4. x86-64 (v1): warnings, the trap, no libm
if FIB_TARGET_CPU=x86-64 "$s2" build "$T/f.fib" -o "$T/f-v1" 2> "$T/e-v1"; then
  [ "$(warnings "$T/e-v1")" = "$(want_sites x86-64 | LC_ALL=C sort)" ] && ok "x86-64: one warning per call site" \
    || no "x86-64 warnings: [$(warnings "$T/e-v1" | sed "s|$T/||" | tr '\n' '|')]"
  "$T/f-v1" 3 > "$T/o-v1" 2> "$T/r-v1"; st=$?
  [ $st = 134 ] && [ "$(cat "$T/r-v1")" = "trap: $(msg x86-64)" ] && [ ! -s "$T/o-v1" ] && ok "x86-64: the run traps (134): $(cat "$T/r-v1")" \
    || no "x86-64 run: status $st, stderr [$(head -c 200 "$T/r-v1")], stdout [$(head -c 80 "$T/o-v1")]"
  nm -D "$T/f-v1" | grep -qw fma && no "x86-64: the program calls libm's fma" || ok "x86-64: no libm fma symbol"
else no "x86-64 build failed: $(head -c 300 "$T/e-v1")"; fi
if FIB_TARGET_CPU=x86-64 "$s2" build "$T/ctl.fib" -o "$T/ctl" 2> "$T/e-ctl"; then
  nm -D "$T/ctl" | grep -qw fma && ok "control: an extern fma shows as a libm symbol (the symbol check can fail)" || no "control: no fma symbol, the check cannot fail"
else no "control build failed: $(head -c 300 "$T/e-ctl")"; fi
# 5. muladd and the dispatch
if FIB_TARGET_CPU=x86-64 "$s2" build "$T/m.fib" -o "$T/m-v1" 2> "$T/e-m"; then
  [ -z "$(warnings "$T/e-m")" ] && ok "x86-64: simd/muladd and a (has-fma) dispatch do not warn" || no "x86-64 muladd warned: $(warnings "$T/e-m" | head -2)"
  out=$("$T/m-v1" 2>&1); [ "$out" = "6.5 6.5 6.5" ] && ok "x86-64: muladd and the dispatch run: $out" || no "x86-64 muladd run: [$out]"
else no "x86-64 muladd build failed: $(head -c 300 "$T/e-m")"; fi
if "$s2" build --target wasm32-wasip1 "$T/m.fib" -o "$T/m-w.o" --emit obj 2> "$T/e-mw"; then
  [ -z "$(warnings "$T/e-mw")" ] && ok "wasm32: simd/muladd and a (has-fma) dispatch do not warn" || no "wasm32 muladd warned: $(warnings "$T/e-mw" | head -2)"
else no "wasm32 muladd emit failed: $(head -c 300 "$T/e-mw")"; fi
[ $bad -eq 0 ] && echo "no-fma: every check holds"
exit $bad
