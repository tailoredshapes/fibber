#!/bin/bash
# Package P0's planted faults (SIMD wave 2): lairf is built as it is and must pass cases/lir/simd and cases/lir/simd-verify; then once per fault with
# one module of compiler/ changed in a shadow directory that comes first on the module path, and the same cases must FAIL (a FAIL line, exit 1). A fault
# the cases do not catch is a hole in them. One group per instruction family: float functions, min and max, reductions, fast-math flags, conversions,
# masked memory and gather/scatter, vector getelementptr, the target form.
# usage: p0-faults.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; FIB_LIB the library; LLVM_LIBDIR the directory of libLLVM-21.so; JOBS builds at once, default 3)
# Exit: 0 the clean run passes and every fault is caught; 1 not; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "p0-faults: no fibc: set FIBC" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/p0-faults-$$}
jobs_max=${JOBS:-3}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
dirs="cases/lir/simd cases/lir/simd-verify"
build() { # SHADOWDIR OUT: the entry file is a copy of lairf.fib inside the shadow directory, because the directory of the entry file is searched first
  mkdir -p "$1"; cp compiler/lairf.fib "$1/lairf.fib"
  "$fibc" build "$1/lairf.fib" -I "$1" -I compiler -I lib -L "$libdir" -l LLVM-21 -o "$2"
}
mkdir -p "$work/clean"
build "$work/clean" "$work/clean/lairf" || { echo "p0-faults: FAILED to build the clean lairf" >&2; exit 1; }
if "$work/clean/lairf" cases $dirs -j 4 > "$work/clean/out" 2>&1; then echo "clean: $(grep -c '^ok ' "$work/clean/out") cases ok"; else echo "clean run FAILS:"; grep '^FAIL' "$work/clean/out" | head; exit 1; fi
fault() { # NAME FILE(under compiler/) FROM TO [target]
  local name=$1 file=$2 from=$3 to=$4 d="$work/$1"
  mkdir -p "$d/$(dirname "$file")"
  python3 - "$root/compiler/$file" "$d/$file" "$from" "$to" <<'EOM' || { echo "p0-faults: $name: the text to change is not in $file" > "$d/result"; return; }
import sys
src, dst, a, b = sys.argv[1:5]
s = open(src).read()
if a not in s:
    sys.exit(1)
open(dst, "w").write(s.replace(a, b, 1))
EOM
  build "$d" "$d/lairf" > "$d/build.log" 2>&1 || { echo "p0-faults: $name: FAILED to build: $(head -c 200 "$d/build.log")" > "$d/result"; return; }
  if [ "${5:-}" = target ]; then # a fault of the machine the code is generated for: p0-target.sh looks at the assembly
    if LAIRF="$d/lairf" "$root/compiler/tests/native/p0-target.sh" > "$d/out" 2>&1; then echo "NOT CAUGHT  $name" > "$d/result"; else echo "caught      $name: $(grep -c '^FAIL' "$d/out") FAIL lines, first: $(grep -m1 '^FAIL' "$d/out" | cut -c1-120)" > "$d/result"; fi
    return
  fi
  if "$d/lairf" cases $dirs -j 2 > "$d/out" 2>&1; then echo "NOT CAUGHT  $name" > "$d/result"; else echo "caught      $name: $(grep -c '^FAIL' "$d/out") FAIL lines, first: $(grep -m1 '^FAIL' "$d/out" | cut -c1-120)" > "$d/result"; fi
}
names=""
run() { # NAME FILE FROM TO [target]: in the background, at most $jobs_max at once
  while [ "$(jobs -rp | wc -l)" -ge "$jobs_max" ]; do sleep 0.1; done
  names="$names $1"
  fault "$@" &
}
# float functions and min/max: the table maps a word to the wrong intrinsic
run fma-is-fmuladd lir/intrin.fib '"fma" (IntrinInfo "f3" "llvm.fma")' '"fma" (IntrinInfo "f3" "llvm.fmuladd")'
run fsqrt-is-fabs lir/intrin.fib '(IntrinInfo "f1" "llvm.sqrt")' '(IntrinInfo "f1" "llvm.fabs")'
run fround-is-roundeven lir/intrin.fib '"fround" (IntrinInfo "f1" "llvm.round")' '"fround" (IntrinInfo "f1" "llvm.roundeven")'
run ffloor-is-ftrunc lir/intrin.fib '"ffloor" (IntrinInfo "f1" "llvm.floor")' '"ffloor" (IntrinInfo "f1" "llvm.trunc")'
run fcopysign-is-fmin lir/intrin.fib '(IntrinInfo "f2" "llvm.copysign")' '(IntrinInfo "f2" "llvm.minimum")'
run fmin-is-minnum lir/intrin.fib '"fmin" (IntrinInfo "f2" "llvm.minimum")' '"fmin" (IntrinInfo "f2" "llvm.minnum")'
run fmaxnum-is-maximum lir/intrin.fib '"fmaxnum" (IntrinInfo "f2" "llvm.maxnum")' '"fmaxnum" (IntrinInfo "f2" "llvm.maximum")'
run smin-is-umin lir/intrin.fib '"smin" (IntrinInfo "i2" "llvm.smin")' '"smin" (IntrinInfo "i2" "llvm.umin")'
run umax-is-smax lir/intrin.fib '"umax" (IntrinInfo "i2" "llvm.umax")' '"umax" (IntrinInfo "i2" "llvm.smax")'
# reductions
run reduce-add-is-mul lir/intrin.fib '(IntrinInfo "red-i" "llvm.vector.reduce.add")' '(IntrinInfo "red-i" "llvm.vector.reduce.mul")'
run reduce-umin-is-smin lir/intrin.fib '"reduce-umin" (IntrinInfo "red-i" "llvm.vector.reduce.umin")' '"reduce-umin" (IntrinInfo "red-i" "llvm.vector.reduce.smin")'
run reduce-fminnum-is-fminimum lir/intrin.fib '"reduce-fminnum" (IntrinInfo "red-m" "llvm.vector.reduce.fmin")' '"reduce-fminnum" (IntrinInfo "red-m" "llvm.vector.reduce.fminimum")'
run reduce-fadd-start-dropped native/lower/intrin.fib '(= c "red-f") (some (set-flags (call-intrinsic fx llvm [(type-of (nth vs 1))] vs) flags))' '(= c "red-f") (some (set-flags (call-intrinsic fx llvm [(type-of (nth vs 1))] [(nth vs 0) (nth vs 1)]) fm-reassoc))'
run reduce-fadd-checks-start-type lir/check/intrin.fib '(_ (fcx-same fcx name 1 ta el pos))' '(_ (Ok ()))'
run reduce-accepts-float-for-int lir/check/intrin.fib '(vector-elem name "an integer" type-int? tv pos)' '(vector-elem name "an integer" type-float? tv pos)'
# fast-math flags
run flags-never-set native/lower/intrin.fib '(do (if (= flags 0) () (unsafe (core/LLVMSetFastMathFlags v (trunc i32 flags))))' '(do (if true () (unsafe (core/LLVMSetFastMathFlags v (trunc i32 flags))))'
run flag-reassoc-is-nsz lir/ast.fib '(def fm-reassoc: i64 1)' '(def fm-reassoc: i64 8)'
run flag-nnan-accepted lir/parse/intrin.fib '(or (= w "nnan") (= w "ninf") (= w "fast"))' '(= w "fast")'
run flag-duplicates-accepted lir/parse/intrin.fib '(if (= (bit-and acc bit) 0)' '(if true'
run flag-fadd-drops-flags native/lower/intrin.fib '(= c "fbin") (some (set-flags (b-binop (. fx b) (fbin-opcode name) (nth vs 0) (nth vs 1)) flags))' '(= c "fbin") (some (set-flags (b-binop (. fx b) (fbin-opcode name) (nth vs 0) (nth vs 1)) 0))'
# conversions
run cast-shape-unchecked lir/check/arith.fib '(if (= (type-lanes v) (type-lanes t))
      (Ok ())' '(if true
      (Ok ())'
run sat-lowered-as-plain native/lower/arith.fib '((CFpToSiSat) (lower-sat fx true t x))' '((CFpToSiSat) (try-let ((v (fx-val fx x))) (Ok (b-cast (. fx b) (cast-opcode CFpToSi) v (lx-ty (. fx lx) t)))))'
run vector-cast-class-scalar-only lir/check/arith.fib '(not (type-int-like? t)) (err pos (str n " needs an integer result type, found " (type-str t)))
        (not (type-float-like? v))' '(not (type-int? t)) (err pos (str n " needs an integer result type, found " (type-str t)))
        (not (type-float-like? v))'
# masked memory, gather, scatter
run masked-load-swaps-mask-passthru native/lower/intrin.fib '[(nth vs 0) (align-of fx align vt) (nth vs 1) (nth vs 2)])))
          (= c "mstore")' '[(nth vs 0) (align-of fx align vt) (nth vs 2) (nth vs 1)])))
          (= c "mstore")'
run scatter-swaps-ptrs-and-value native/lower/intrin.fib '[(nth vs 0) (nth vs 1) (align-of fx align (elem-type vt)) (nth vs 2)]) nil)))))' '[(nth vs 1) (nth vs 0) (align-of fx align (elem-type vt)) (nth vs 2)]) nil)))))'
run mask-type-unchecked lir/check/intrin.fib '(defun mask-of :private (t: LType) -> LType
  (match t ((TVector n _) (TVector n ty-i1)) (_ ty-i1)))' '(defun mask-of :private (t: LType) -> LType t)'
run masked-accepts-pointer-vectors lir/check/intrin.fib '(if (or (type-int? el) (type-float? el))' '(if true'
run gather-ptrs-unchecked lir/check/intrin.fib '(defun ptrs-of :private (t: LType) -> LType
  (match t ((TVector n _) (TVector n ty-ptr)) (_ ty-ptr)))' '(defun ptrs-of :private (t: LType) -> LType t)'
run masked-align-default-zero native/lower/intrin.fib '(nil (zext i64 (unsafe (target/LLVMABIAlignmentOfType (. (. fx lx) td) ty)))))))' '(nil 1))))'
# vector getelementptr
run gep-vector-with-more-indices lir/check/memory.fib '(not (= (count idx) 1)) (err pos "getelementptr: a vector index must be the only index")' 'false (err pos "getelementptr: a vector index must be the only index")'
run gep-vector-result-scalar lir/check/memory.fib ':else (recur (+ k 1) cur (TVector (unwrap-or (type-lanes ti) 1) ty-ptr)))' ':else (recur (+ k 1) cur ty-ptr))'
# the target form
run target-attribute-cpu native/lower.fib '(add-string-attribute lx f "target-cpu" (. t cpu))' '(add-string-attribute lx f "target-cpu" "generic")'
run target-attribute-features native/lower.fib '(add-string-attribute lx f "target-features" (. t features))' '()'
run target-host-always-supports native/target.fib '(match (lt/host-lacks (. t features))' '(match (lt/host-lacks "")'
run target-twice-accepted lir/check/env.fib '(recur (+ i 1) true (acc-err a (Diagnostic (. t pos) "a module has at most one target form")))' '(recur (+ i 1) true a)'
run target-features-unchecked lir/parse/items.fib '(defun bad-feature :private (bs: (Array i8)) -> (Option str)
  (let ((n (array-len bs)))' '(defun bad-feature :private (bs: (Array i8)) -> (Option str)
  (let ((n 0))'
run target-cpu-chars-unchecked lir/parse/items.fib '(and (> (array-len bs) 0) (name-bytes? bs 0 (array-len bs)))' '(> (array-len bs) 0)'
wait
bad=0
for n in $names; do
  cat "$work/$n/result"
  case $(head -c 6 "$work/$n/result") in caught) ;; *) bad=1 ;; esac
done
[ $bad -eq 0 ] && echo "p0-faults: every planted fault is caught"
exit $bad
