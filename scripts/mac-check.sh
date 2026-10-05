#!/bin/bash
# Runs on an Apple Silicon Mac: builds stage 2 from a seed and runs the smoke list of docs/design/aarch64.md 6, printing the exact failures.
#   scripts/mac-check.sh [--quick]
# Needs: Xcode command line tools (cc), Homebrew `llvm@21` (keg-only, /opt/homebrew/opt/llvm@21; MAC_LLVM names another prefix), and a seed `fibc` for arm64 macOS:
#   FIBC=/path/to/fibc     a native arm64 fibc. Until a release has one, make it on an x86-64 Linux box that has a stage 2: compiler/tests/native/a64-fibc-on-mac.sh
#                          cross-compiles compiler/fibc.fib to a Mach-O object, copies it over and links it here (that is "the cross seed"); copy the result's
#                          `fibc` to the Mac and name it in FIBC. Without FIBC the script looks for ./fibc beside the checkout, then for `fibc` on PATH.
# Steps (each prints `ok` or the exact failure; the exit status is the number of failed steps, 0 when all hold):
#   1  the machine: arm64, cc, LLVM 21 dylib, the seed runs (`fibc --version`)
#   2  stage 2 (F) built by the seed; 3  the fixed point: F builds F3 and `F emit --exe compiler/fibc.fib` equals `F3 emit --exe` (lIR of the host target)
#   4  hello world: fibc build (the AOT path, links with cc) and fibc run (the ORC JIT: no entitlements, ad-hoc signed by ld)
#   5  fibc cases cases/ownership -j N (310 of 311 pass natively with spec/method.md absent; case 188 reads a repo file by a relative path)
#   6  fibc cases cases/stdlib --only PREFIXES (SIMD 62, tensors 70, OS 74, fmt 42, mkdir 36); the allowed failures are listed in KNOWN below
#   7  JIT at every -O level and AOT at -O 0 and -O 2 on the case that crashed at -O 0 (6223), and compiler/tests/native/a64-o0.sh: all must exit 0 (A64-1)
# Writes under $HOME/fibber-a64-scratch only (override with MAC_SCRATCH). Never installs anything. Written for, and first run on, an M1 Ultra (macOS 27).
set -u
root=$(cd "$(dirname "$0")/.." && pwd); cd "$root"
llvm=${MAC_LLVM:-/opt/homebrew/opt/llvm@21}; scratch=${MAC_SCRATCH:-$HOME/fibber-a64-scratch/check}; mkdir -p "$scratch/tmp"
export FIB_LIB=$root/lib TMPDIR=$scratch/tmp
jobs=${JOBS:-8}; quick=; [ "${1:-}" = --quick ] && quick=1
bad=0
step() { local n=$1; shift; if "$@"; then echo "ok   step $n"; else echo "FAIL step $n"; bad=$((bad+1)); fi; }
# KNOWN: cases that fail on the Mac for a named reason (docs/design/aarch64.md 6.3); anything else that fails is new.
KNOWN='1707'   # also expected to fail on x86 (scripts/ci-stage2.expected); A64-1 fixed 3607 4205 6223 7077 7078 7402 7404 (docs/design/aarch64.md 7)

seed() { for c in "${FIBC:-}" "$root/../fibc" "$scratch/../fibc" "$(command -v fibc || true)"; do [ -n "$c" ] && [ -x "$c" ] && { echo "$c"; return; }; done; }
s1() {
  [ "$(uname -m)" = arm64 ] || { echo "  not arm64: $(uname -m)"; return 1; }
  command -v cc >/dev/null || { echo "  no cc: xcode-select --install"; return 1; }
  [ -e "$llvm/lib/libLLVM-21.dylib" ] || { echo "  no $llvm/lib/libLLVM-21.dylib: brew install llvm@21 (not llvm: that is 22)"; return 1; }
  SEED=$(seed); [ -n "$SEED" ] || { echo "  no seed fibc: set FIBC (see the header of this script)"; return 1; }
  "$SEED" --version || { echo "  the seed does not run"; return 1; }
}
build_with() { "$1" build compiler/fibc.fib -I compiler -I lib -L "$llvm/lib" -l LLVM-21 -o "$2"; }
s2() { build_with "$SEED" "$scratch/F" 2> "$scratch/build.err" || { tail -5 "$scratch/build.err"; return 1; }; "$scratch/F" --version; }
s3() {
  build_with "$scratch/F" "$scratch/F3" 2> "$scratch/build3.err" || { tail -5 "$scratch/build3.err"; return 1; }
  "$scratch/F" emit --exe -I compiler -I lib compiler/fibc.fib > "$scratch/e2.lir" && "$scratch/F3" emit --exe -I compiler -I lib compiler/fibc.fib > "$scratch/e3.lir" || return 1
  cmp "$scratch/e2.lir" "$scratch/e3.lir" || { echo "  F and F3 emit different lIR"; return 1; }
}
s4() {
  F=$scratch/F; echo '(defun main () -> i64 (do (println "hello from fibber on aarch64") 0))' > "$scratch/hello.fib"
  "$F" build "$scratch/hello.fib" -o "$scratch/hello" 2>&1 && [ "$("$scratch/hello")" = "hello from fibber on aarch64" ] || { echo "  AOT hello failed"; return 1; }
  [ "$("$F" run "$scratch/hello.fib" | head -1)" = "hello from fibber on aarch64" ] || { echo "  JIT hello failed"; return 1; }
}
cases() { # DIR [--only ..]: the `fibc cases` table; the lines that are not pass/open are printed; fails on a FAIL line not in KNOWN
  "$scratch/F" cases "$@" -j "$jobs" > "$scratch/cases.out" 2>&1
  local fails new= f n
  fails=$(grep -E ' FAIL ' "$scratch/cases.out" | awk '{print $1}')
  for f in $fails; do n=${f%%-*}; case " $KNOWN " in *" $n "*) ;; *) new="$new $f" ;; esac; done
  grep -E '^[0-9]+ cases: ' "$scratch/cases.out"
  [ -z "$new" ] || { echo "  new failures:"; for f in $new; do grep "^$f " "$scratch/cases.out" | cut -c1-220; done; return 1; }
}
s5() { cases cases/ownership; }
s6() { if [ -n "$quick" ]; then cases cases/stdlib --only 62 70 74 42 36; else cases cases/stdlib; fi; }
s7() {
  local c=cases/stdlib/6223-vectors-cross-calls-and-loops-at-odd-widths.fib o st r=0
  for o in 0 1 2; do "$scratch/F" run -O $o -I cases/stdlib/support "$c" > /dev/null 2>&1; st=$?; echo "  JIT -O $o: exit $st"; [ $st -eq 0 ] || r=1; done
  for o in 0 2; do
    "$scratch/F" build -I cases/stdlib/support "$c" -o "$scratch/v6223" -O $o > /dev/null 2>&1 && "$scratch/v6223" > /dev/null 2>&1; st=$?; echo "  AOT -O $o: exit $st"; [ $st -eq 0 ] || r=1
  done
  F="$scratch/F" compiler/tests/native/a64-o0.sh || r=1
  return $r
}
step 1 s1 || exit 1
step 2 s2; [ -x "$scratch/F" ] || exit 1
step 3 s3; step 4 s4; step 5 s5; step 6 s6; step 7 s7
echo "mac-check: $bad step(s) failed"
exit $bad
