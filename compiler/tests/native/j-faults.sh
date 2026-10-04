#!/bin/bash
# Package J's gate 5: planted faults. j-unit.fib is built and run as it is (it must pass), then once per fault with one module of native/jit/ changed in a
# shadow directory that comes first on the module path; each must make j-unit.fib fail (exit 1 with a FAIL line). A fault that does not fail is a hole in the test.
# usage: j-faults.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; FIB_LIB the library; LLVM_LIBDIR the directory of libLLVM-21.so)
# Exit: 0 the clean run passes and every fault is caught; 1 not; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "j-faults: no fibc: set FIBC" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/j-faults-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
build() { # SHADOWDIR OUT
  "$fibc" build compiler/tests/native/j-unit.fib -I "$1" -I compiler -I compiler/tests/native -I lib -L "$libdir" -l LLVM-21 -o "$2"
}
mkdir -p "$work/clean"
build "$work/clean" "$work/clean/j-unit" || { echo "j-faults: FAILED to build the clean test" >&2; exit 1; }
if "$work/clean/j-unit" > "$work/clean/out" 2>&1; then echo "clean: $(grep -c '^ok ' "$work/clean/out") checks ok"; else echo "clean run FAILS:"; grep -v '^ok ' "$work/clean/out" | head; exit 1; fi
bad=0
fault() { # NAME FILE FROM TO
  local name=$1 file=$2 from=$3 to=$4 d="$work/$1"
  mkdir -p "$d/native/jit"
  python3 - "$root/compiler/native/jit/$file" "$d/native/jit/$file" "$from" "$to" <<'EOM' || { echo "j-faults: $name: the text to change is not in $file" >&2; bad=1; return; }
import sys
src, dst, a, b = sys.argv[1:5]
s = open(src).read()
if a not in s:
    sys.exit(1)
open(dst, "w").write(s.replace(a, b, 1))
EOM
  build "$d" "$d/j-unit" || { echo "j-faults: $name: FAILED to build" >&2; bad=1; return; }
  if "$d/j-unit" > "$d/out" 2>&1; then echo "NOT CAUGHT  $name"; bad=1; else echo "caught      $name: $(grep -c '^FAIL' "$d/out") FAIL lines, first: $(grep -m1 '^FAIL' "$d/out" | cut -c1-110)"; fi
}
# the name table
fault names-record-exported-as-hidden names.fib '(JitNames (assoc (. names exported) name (Exported def module)) (. names hidden))' '(JitNames (. names exported) (assoc (. names hidden) name module))'
fault names-no-duplicate-check names.fib '(nil (some (Diagnostic pos (str "duplicate definition of @" name " (first in module " (. first module) ")"))))' '(nil nil)'
fault names-declaration-always-matches names.fib '(if (def-matches? (. first def) w)' '(if true'
fault names-undefined-symbol-accepted names.fib '(some _) (if (in-process name)' '(some _) (if true'
# prune
fault prune-ignores-direct-calls prune.fib '((CalDirect n) (reach r n))' '((CalDirect n) ())'
fault prune-keeps-everything prune.fib '(or (item-exported? it) (contains? @(. r live) n))' 'true'
fault prune-drops-initialisers prune.fib '((ItGlobal g) (visit r (. g init)))' '((ItGlobal g) ())'
# the trampoline
fault trampoline-void-without-ret trampoline.fib '(nil (str call " (ret)"))' '(nil call)'
fault trampoline-name trampoline.fib '(str name ".tramp")' '(str name ".t")'
fault trampoline-argument-order trampoline.fib '(str "a" i)' '(str "a" (- (count (. ty params)) i 1))'
exit $bad
