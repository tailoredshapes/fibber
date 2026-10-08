#!/bin/bash
# DARWIN-2: a `tailcc` function (the callee pops its stack arguments) of ten arguments that ends in a plain call of a `ccc` function must not be compiled
# to a sibling call on AArch64: LLVM 21 emitted `b callee` after popping too little of the caller's argument area, so the caller found its stack pointer
# 16 bytes off and returned to a stack address (lir2c on macOS: `fatal signal 10` at the program counter). native.lower.calls marks such a call `notail`.
#   1. (any host) the assembly for arm64-apple-macosx of cases/lir/instr/stackargs-sibcall.lir at -O 2 has no `b _srand` (a sibling branch) in `_pop` or `_fpop`,
#      and has the call `bl _srand` in both;
#   2. (any host) the planted fault, the assembly with `bl _srand` turned back into `b _srand`, is caught by the same check;
#   3. (an arm64 Mac only) the program runs at -O 0 to -O 3 and prints the sums (before the change: exit 138 or 139 from -O 1 up).
# usage: a64-sibcall.sh [FIBC]    FIBC or F or `fibc`; L=lairf (LAIRF also read; unset: built from compiler/lairf.fib), LLVMLIB=directory of libLLVM-21
# Exit 0 all hold, 1 a check failed, 2 no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root" || exit 2
# shellcheck disable=SC1091
[ -f scripts/portable/env.sh ] && . scripts/portable/env.sh
F=${1:-${F:-${FIBC:-fibc}}}; L=${L:-${LAIRF:-}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "a64-sibcall: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/a64-sibcall.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
lib=${LLVMLIB:-${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}}
if [ -z "$L" ]; then
  "$F" build compiler/lairf.fib -I compiler -I lib -L "$lib" -l LLVM-21 -o "$T/lairf" > "$T/build.log" 2>&1 || { cat "$T/build.log"; echo "a64-sibcall: lairf did not build" >&2; exit 2; }
  L=$T/lairf
fi
export LD_LIBRARY_PATH=$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
prog=cases/lir/instr/stackargs-sibcall.lir
if ! "$L" build "$prog" --target arm64-apple-macosx13.0.0 --emit asm -O 2 -o "$T/s.s" > "$T/asm.log" 2>&1; then
  cat "$T/asm.log"; no "could not generate the arm64 assembly"; echo "a64-sibcall: $bad failure(s)"; exit 1
fi
# the body of a function of the assembly: from its label to the next .globl
body() { awk -v f="$2" '$0 == f ":" { m = 1; next } m && /\.globl/ { exit } m { print }' "$1"; }
check() {   # check ASM: prints "ok" or what is wrong
  local fn
  for fn in _pop _fpop; do
    if body "$1" $fn | grep -qE '^[[:space:]]b[[:space:]]+_srand$'; then echo "$fn ends in a sibling branch to _srand"; return; fi
    if ! body "$1" $fn | grep -qE '^[[:space:]]bl[[:space:]]+_srand$'; then echo "$fn has no call of _srand"; return; fi
  done
  echo ok
}
r=$(check "$T/s.s"); if [ "$r" = ok ]; then ok "the assembly of _pop and _fpop calls _srand (bl), no sibling branch (b)"; else no "$r"; fi
sed 's/^\([[:space:]]*\)bl\([[:space:]]*_srand\)$/\1b\2/' "$T/s.s" > "$T/bad.s"
r=$(check "$T/bad.s"); case $r in *sibling*) ok "planted sibling branch is caught ($r)" ;; *) no "planted fault not caught: [$r]" ;; esac
if [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then
  for o in 0 1 2 3; do
    out=$("$L" run -O $o "$prog" 2>&1); st=$?
    if [ $st -eq 0 ] && [ "$out" = "$(printf 'stackargs 509500\nstackargs-fp 20.5')" ]; then ok "run -O $o: the sums"; else no "run -O $o: exit $st, output [$out]"; fi
  done
else echo "skip run at -O 0 to 3: not an arm64 Mac"; fi
echo "a64-sibcall: $bad failure(s)"; exit $bad
