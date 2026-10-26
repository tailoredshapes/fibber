#!/bin/bash
# compiler/tests/emit/facts/check.sh FIBC: the lIR facts the emitter proves (compiler/emit/lower/facts.fib, spec/lir.md 6.13).
#   1. the lIR of cases/stdlib/8852 has its five bounded steps as `ADD NSW`/`SUB NSW` and of vec.fib the tail read of vec-nth as
#      `(ASSUME (ICMP ULT ..))` with no `fib.trap-index` in vec-nth's body; the stage 2 before the facts emits none of them;
#   2. the boundary cases 8852 (steps up to the maximum and down to the minimum) and 8853 (a step under `<=`: no fact, the overflow
#      trap) and the vec-nth edge cases 8849 to 8851 pass in the default mode and under FIB_LIR_FACTS=trap;
#   3. --plant: two false facts are planted in copies of the emitter, built by FIBC: (a) a step under `<=` taken as bounded, (b) the
#      tail fact of vec-nth applied to every array read. Under FIB_LIR_FACTS=trap the planted compilers must trap with
#      `lIR fact violated` on 8853 and on 8845 (an aget past the end): the check mode catches a false fact the emitter makes.
# usage: check.sh FIBC [--plant]     run from anywhere; FIB_LIB defaults to the tree's lib. Exit 1 on any difference.
fibc=${1:?usage: check.sh FIBC [--plant]}
plant=${2:-}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
out=$(mktemp -d "${TMPDIR:-/tmp}/facts-emit.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null; ulimit -c 0
cd "$root" || exit 2
unset FIB_LIR_FACTS
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
lib=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}

# 1. the facts in the lIR
timeout 300 "$fibc" emit -I cases/stdlib/support cases/stdlib/8852-a-counted-step-at-the-edge-of-its-type.fib > "$out/steps.lir" 2> "$out/steps.err" || no "emit 8852: $(head -c 300 "$out/steps.err")"
n=$(grep -o '(ADD NSW [^ ]* (I\(64\|32\) 1))\|(ADD NSW (I64 1) [^ )]*)\|(SUB NSW [^ ]* (I64 1))' "$out/steps.lir" | wc -l)
[ "$n" -eq 5 ] && ok "8852: the 5 bounded steps are nsw facts" || no "8852: $n bounded steps are nsw facts, wanted 5"
timeout 300 "$fibc" emit "$here/vec.fib" > "$out/vec.lir" 2> "$out/vec.err" || no "emit vec.fib: $(head -c 300 "$out/vec.err")"
python3 - "$out/vec.lir" <<'PY' && ok "vec-nth: the tail read is an assume, with no index trap" || no "vec-nth: not the assume (see above)"
import re, sys
lines = open(sys.argv[1]).read().split('\n')
body = [l for l in lines if re.match(r'\(DEFINE [A-Z ]*\(f\.fib\.prelude\.vec-nth\.', l)]
if not body: print('no vec-nth body'); sys.exit(1)
b = body[0]
if '(ASSUME (ICMP ULT' not in b: print('no assume in vec-nth'); sys.exit(1)
if 'fib.trap-index' in b or 'fib.raise-index' in b: print('vec-nth still has the index trap'); sys.exit(1)
PY

# 2. the boundary cases, both modes
cases="--only 8852-a-counted-step-at-the-edge-of-its-type.fib 8853-trap-a-step-past-the-maximum-under-a-non-strict-test.fib 8849-nth-at-the-edges-of-flat-trie-and-shared-vecs.fib 8850-trap-nth-at-the-count-of-a-shared-trie.fib 8851-trap-nth-with-a-negative-index-on-a-shared-trie.fib"
for m in use trap; do
  r=$(FIB_LIR_FACTS=$m timeout 900 "$fibc" cases cases/stdlib $cases -j 2 2>&1 | grep 'cases:')
  case "$r" in "5 cases: 5 pass, 0 fail"*) ok "boundary cases under $m: $r" ;; *) no "boundary cases under $m: $r" ;; esac
done

# 3. planted false facts
if [ "$plant" = --plant ]; then
  for p in a b; do
    d=$out/plant-$p; mkdir -p "$d"; cp -r compiler "$d/"
    f=$d/compiler/emit/lower/facts.fib
    if [ $p = a ]; then sed -i 's/(if (and (or (= m "<") (= m ">")) (= (count args) 2))/(if (and (or (= m "<") (= m ">") (= m "<=")) (= (count args) 2))/; s/((some y) (into (side cx x (= m "<")) (side cx y (= m ">"))))/((some y) (into (side cx x (or (= m "<") (= m "<="))) (side cx y (= m ">"))))/' "$f"
    else sed -i 's/^    ((BodyFun f) (and (= (. (g-fun g f) name) "vec-nth")/    ((BodyFun f) (or true (= (. (g-fun g f) name) "vec-nth")/' "$f"; fi
    cmp -s "$f" compiler/emit/lower/facts.fib && { no "plant $p: the sed did not apply"; continue; }
    (cd "$d" && FIB_LIRO_PARTITION=1 timeout 1200 "$fibc" build compiler/fibc.fib -I compiler -I lib -L "$lib" -l LLVM-21 -o "$d/F" > "$d/build.log" 2>&1) || { tail -5 "$d/build.log"; no "plant $p: did not build"; continue; }
    c=$([ $p = a ] && echo 8853-trap-a-step-past-the-maximum-under-a-non-strict-test.fib || echo 8845-trap-aget-out-of-range.fib)
    FIB_LIR_FACTS=trap timeout 600 "$d/F" run -I lib -I cases/stdlib/support "cases/stdlib/$c" > "$d/run.out" 2> "$d/run.err"; st=$?
    if grep -q 'lIR fact violated' "$d/run.err"; then ok "plant $p: the false fact traps under trap (status $st): $(grep -m1 'lIR fact violated' "$d/run.err")"
    else no "plant $p: no fact trap (status $st): $(head -c 300 "$d/run.err")"; fi
  done
fi
exit $bad
