#!/bin/bash
# Package F's planted faults: f-unit.fib is built and run as it is (it must pass), then once per fault with one module of native/fuzz/ changed in a
# shadow directory that comes first on the module path; each must make f-unit.fib fail (exit 1 with a FAIL line). A fault that does not fail is a hole.
# usage: f-faults.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; FIB_LIB the library; LLVM_LIBDIR the directory of libLLVM-21.so)
# Exit: 0 the clean run passes and every fault is caught; 1 not; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "f-faults: no fibc: set FIBC" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/f-faults-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
build() { # SHADOWDIR OUT
  # shellcheck disable=SC2086
  "$fibc" build compiler/tests/native/f-unit.fib -I "$1" -I compiler -I compiler/tests/native -I lib -L "$libdir" -l LLVM-21 -o "$2"
}
mkdir -p "$work/clean"
build "$work/clean" "$work/clean/f-unit" || { echo "f-faults: FAILED to build the clean test" >&2; exit 1; }
if "$work/clean/f-unit" > "$work/clean/out" 2>&1; then echo "clean: $(grep -c '^ok ' "$work/clean/out") checks ok"; else echo "clean run FAILS:"; grep -v '^ok ' "$work/clean/out" | head; exit 1; fi
bad=0
fault() { # NAME FILE FROM TO
  local name=$1 file=$2 from=$3 to=$4 d="$work/$1"
  mkdir -p "$d/native/fuzz"
  python3 - "$root/compiler/native/fuzz/$file" "$d/native/fuzz/$file" "$from" "$to" <<'EOM' || { echo "f-faults: $name: the text to change is not in $file" >&2; bad=1; return; }
import sys
src, dst, a, b = sys.argv[1:5]
s = open(src).read()
if a not in s:
    sys.exit(1)
open(dst, "w").write(s.replace(a, b, 1))
EOM
  build "$d" "$d/f-unit" || { echo "f-faults: $name: FAILED to build" >&2; bad=1; return; }
  if "$d/f-unit" > "$d/out" 2>&1; then echo "NOT CAUGHT  $name"; bad=1; else echo "caught      $name: $(grep -c '^FAIL' "$d/out") FAIL lines, first: $(grep -m1 '^FAIL' "$d/out" | cut -c1-110)"; fi
}
fault rng-gamma rng.fib '(wadd @(. r state) -7046029254386353131)' '(wadd @(. r state) -7046029254386353130)'
fault rng-no-carry rng.fib '(recur (+ k 1) (shr t 16)' '(recur (+ k 1) 0'
fault rng-signed-remainder rng.fib '(rem (+ (* 2 (rem (shr x 1) m)) (bit-and x 1)) m))' '(let ((q (rem x m))) (if (< q 0) (- 0 q) q)))'
fault rng-below-draws-at-zero rng.fib '(if (= n 0) 0 (urem (rng-next r) n))' '(urem (rng-next r) (if (= n 0) 1 n))'
fault tree-escape tree.fib '(= b 10) "\\n"' '(= b 11) "\\n"'
fault tree-bracket tree.fib '((SxBracket items _) (write-seq items "[" "]" out))' '((SxBracket items _) (write-seq items "(" ")" out))'
fault tree-preorder tree.fib '(m (- n 1)))
                (if (< m inner)
                    (match (items-of f) ((some v) (nth-node v m))' '(m (- n 0)))
                (if (< m inner)
                    (match (items-of f) ((some v) (nth-node v m))'
fault words-keyword-order words.fib '"ashr"
   "fneg"' '"fneg"
   "ashr"'
fault grammar-draw-order grammar.fib '(let ((ty (random-type r c))
        (k (rng/rng-below r 24)))' '(let ((k (rng/rng-below r 24))
        (ty (random-type r c)))'
fault mutate-wanted mutate.fib '(+ 1 (rng/rng-below r 3))' '(+ 2 (rng/rng-below r 3))'
fault deep-first-block-copied deep.fib '(nth idx (- (count idx) 1))' '(nth idx 0)'
fault verdict-rejected verdict.fib '(and (not (= (. o status) 0)) (contains? stderr "error")) FzRejected' '(not (= (. o status) 0)) FzRejected'
fault verdict-trap verdict.fib '(trapped? stderr) (FzFinding (str "panic: " tail))' '(trapped? stderr) FzRejected'
exit $bad
