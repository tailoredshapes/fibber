#!/bin/bash
# Package A64-1: `-O 0` on AArch64 (docs/design/aarch64.md 2.3). LLVM 21's FastISel + RegAllocFast at code generation level None miscompiles a call to a
# `tailcc` function whose stack arguments the callee pops: a spilled value is reloaded sp-relative before the `sub sp, sp, #N` that restores the popped bytes.
# Since A64-1 an AArch64 machine at -O 0 generates at level Less (compiler/llvm/target.fib `machine-level`).
#   1. (any host) JIT `run -O 0` (prints 0) and AOT `build -O 0` (exits 0) of a64/o0-callee-pop.fib (on arm64 macOS they crashed with 139 before the change);
#   2. (any host) the program with a planted wrong expected value prints 1, not 0 and not a crash: the check can fail;
#   3. (any host, with L) the assembly for arm64-apple-macosx at -O 0 has no sp-relative access between the `bl` of `mix` and the `sub sp, sp, #N` that restores the
#      popped arguments; the planted fault inserts such a reload and must be caught.
# usage: a64-o0.sh    F=stage-2 fibc (FIBC also read), L=lairf (LAIRF also read; unset skips 3). Exit 0 all hold, 1 a check failed, 2 no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}; L=${L:-${LAIRF:-}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "a64-o0: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/a64-o0.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
prog=compiler/tests/native/a64/o0-callee-pop.fib; I="-I cases/stdlib/support"
out=$("$F" run -O 0 $I "$prog" 2>&1); st=$?   # `run` prints main's result and exits 0; a crash is the shell's 128+signal
[ $st -eq 0 ] && [ "$out" = 0 ] && ok "JIT run -O 0: prints 0, exit 0" || no "JIT run -O 0: exit $st, output [$out]"
if "$F" build $I "$prog" -o "$T/p" -O 0 > /dev/null 2>&1; then "$T/p" > /dev/null 2>&1; st=$?; else st=build-failed; fi
[ "$st" = 0 ] && ok "AOT build -O 0: exit 0" || no "AOT build -O 0: $st"
sed 's/47\.0/48.0/' "$prog" > "$T/wrong.fib"
out=$("$F" run -O 0 $I "$T/wrong.fib" 2>&1); st=$?
[ $st -eq 0 ] && [ "$out" = 1 ] && ok "planted wrong expected value: prints 1 (the check fires, no crash)" || no "planted wrong value: exit $st, output [$out], want 1"
if [ -n "$L" ]; then
  if "$F" emit --exe --target arm64-apple-macosx13.0.0 $I "$prog" > "$T/m.lir" 2>/dev/null && "$L" build "$T/m.lir" --target arm64-apple-macosx13.0.0 -o "$T/m.s" -O 0 --emit asm 2>/dev/null; then
    # the first sp-relative access after `bl _f.mix` and before the `sub sp, sp, #N`: there must be none
    skew() { awk '/^_f\.main:/{m=1} m&&/bl\t_f\.mix$/{b=1;next} b&&/sub\tsp, sp, #[0-9]+$/{print "none";exit} b&&/(ldr|str|ldp|stp)\t.*\[sp/{print "skewed: "$0;exit}' "$1"; }
    r=$(skew "$T/m.s"); [ "$r" = none ] && ok "-O 0 assembly: no sp-relative access between bl _f.mix and its sub sp" || no "-O 0 assembly: $r"
    awk '/^_f\.main:/{m=1} m&&/bl\t_f\.mix$/{print; print "\tldr\tx0, [sp, #264]"; next} {print}' "$T/m.s" > "$T/bad.s"
    r=$(skew "$T/bad.s"); case $r in skewed*) ok "planted sp-relative reload after the call is caught ($r)" ;; *) no "planted fault not caught: $r" ;; esac
  else no "could not generate the aarch64 assembly"; fi
fi
echo "a64-o0: $bad failure(s)"; exit $bad
