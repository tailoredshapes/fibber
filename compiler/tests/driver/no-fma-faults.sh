#!/bin/bash
# The planted faults of no-fma.sh (docs/adr/0008): fibc is built once per fault with emit.lower.simdfn changed in a shadow directory that comes first
# on the module path, and no-fma.sh run with it must FAIL. A fault it does not catch is a hole in it.
#   no-warning   simd/fma without FMA records no warning (the trap stays)
#   libm-call    simd/fma without FMA lowers to `fma` (a libm call per lane) instead of the trap (the warning stays)
#   no-trap      the trap is left out and the multiply-then-add stand-in computed (the warning stays, no libm call)
# Not in the gate (three compiler builds, about four minutes); run it after a change of simdfn.fib or of no-fma.sh.
# usage: no-fma-faults.sh [WORKDIR]     (FIBC names the fibc that builds, default `fibc`; FIB_LIB the library; LLVM_LIBDIR libLLVM-21's directory)
# Exit: 0 every fault is caught; 1 not; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "no-fma-faults: no fibc: set FIBC" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/no-fma-faults-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
fault() { # NAME FROM TO [FROM2 TO2]: build fibc with the first FROM of simdfn.fib replaced by TO (and FROM2 by TO2), run no-fma.sh with it
  local name=$1 d="$work/$1"
  mkdir -p "$d/emit/lower"; cp compiler/fibc.fib "$d/fibc.fib"
  python3 - compiler/emit/lower/simdfn.fib "$d/emit/lower/simdfn.fib" "$2" "$3" "${4:-}" "${5:-}" <<'EOM' || { echo "FAIL $name: the text to change is not in simdfn.fib"; return 1; }
import sys
src, dst, a, b, a2, b2 = sys.argv[1:7]
s = open(src).read()
if a not in s or (a2 and a2 not in s):
    sys.exit(1)
s = s.replace(a, b, 1)
open(dst, "w").write(s.replace(a2, b2, 1) if a2 else s)
EOM
  "$fibc" build "$d/fibc.fib" -I "$d" -I compiler -I lib -L "$libdir" -l LLVM-21 -o "$d/fibc" > "$d/build.log" 2>&1 || { echo "FAIL $name: the fault does not build: $(head -c 200 "$d/build.log")"; return 1; }
  if "$here/no-fma.sh" "$d/fibc" > "$d/out" 2>&1; then echo "FAIL $name: NOT CAUGHT"; return 1; fi
  echo "ok   $name caught: $(grep -c '^FAIL' "$d/out") FAIL lines, first: $(grep -m1 '^FAIL' "$d/out" | cut -c1-110)"
}
bad=0
fault no-warning '((some e) (g-warn (program-g (. cx p)) (str-join [(pos-text (. e pos)) ": warning: " m])))' '((some e) ())' || bad=1
trap_line='(lcx-trap-if cx (v-int LirI1 1) m))))'
fault libm-call "$trap_line" '())))' '(if (. (g-target (program-g (. cx p))) has-fma) "fma" "fmuladd")' '"fma"' || bad=1
fault no-trap "$trap_line" '())))' || bad=1
[ $bad -eq 0 ] && echo "no-fma-faults: every planted fault is caught"
exit $bad
