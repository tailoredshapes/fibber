#!/bin/bash
# compiler/tests/specs/plant-vec.sh: the Vec contract (specs/vec-spec.fib) can fail. It runs `fibc test` over a COPY of the Vec spec files (specs/vec-*.fib and listvec.fib: other specs are not its business) (the tree's is untouched):
#   1. as committed: exit 0, 22 scenarios pass (the contract against the library's Vec and against ListVec);
#   2. with one fault planted in ListVec, `init` (pop) drops the FIRST element instead of the last: exit 1, exactly one scenario fails,
#      and it is `VecContract[ListVec]/pop-removes-the-last-element`, none in the library's Vec;
#   3. with a different fault planted, `put` (assoc) writes one index too far: exit 1, exactly the scenarios that read the replaced element fail,
#      in ListVec only.
# usage: compiler/tests/specs/plant-vec.sh STAGE2        (run from the repository root; FIB_LIB defaults to the tree's lib)
set -uo pipefail
s2=${1:?usage: plant-vec.sh STAGE2}
root=$PWD
export FIB_LIB=${FIB_LIB:-$root/lib}
ulimit -v 16000000
t=$(mktemp -d "$HOME/.cache/fibber-scratch/plant-vec.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
failing() { grep -o '"id":"[^"]*","text":"[^"]*","covers":\[[^]]*\],"status":"fail"' | sed 's/","text".*//; s/"id":"//' | tr '\n' ' '; }

mkdir "$t/clean"; cp specs/vec-*.fib specs/listvec.fib "$t/clean/"
out=$("$s2" test "$t/clean" --format json 2>/dev/null); st=$?
ck "1 as committed: exit" "$st" 0
ck "1 as committed: summary" "$(echo "$out" | grep -o '"summary":{[^}]*}')" '"summary":{"total":22,"pass":22,"fail":0,"trap":0,"timeout":0}'

mkdir "$t/p1"; cp specs/vec-*.fib specs/listvec.fib "$t/p1/"
sed -i 's/((Cons h t) (match t ((Empty) Empty) (_ (Cons h (init t)))))))/((Cons h t) t)))/' "$t/p1/listvec.fib"
ck "2 the plant is one line of the copy" "$(diff specs/listvec.fib "$t/p1/listvec.fib" | grep -c '^>')" 1
out=$("$s2" test "$t/p1" --format json 2>/dev/null); st=$?
ck "2 pop drops the first: exit" "$st" 1
ck "2 pop drops the first: one failure" "$(echo "$out" | grep -o '"summary":{[^}]*}')" '"summary":{"total":22,"pass":21,"fail":1,"trap":0,"timeout":0}'
ck "2 pop drops the first: it is the pop scenario of ListVec" "$(echo "$out" | failing)" "VecContract[ListVec]/pop-removes-the-last-element "

mkdir "$t/p2"; cp specs/vec-*.fib specs/listvec.fib "$t/p2/"
sed -i 's/((Cons h t) (if (= i 0) (Cons x t) (Cons h (put t (- i 1) x))))))/((Cons h t) (if (= i 1) (Cons x t) (Cons h (put t (- i 1) x))))))/' "$t/p2/listvec.fib"
ck "3 the plant is one line of the copy" "$(diff specs/listvec.fib "$t/p2/listvec.fib" | grep -c '^>')" 1
out=$("$s2" test "$t/p2" --format json 2>/dev/null); st=$?
ck "3 assoc one index too far: exit" "$st" 1
ck "3 assoc one index too far: only ListVec, only scenarios that read the new element" "$(echo "$out" | failing)" "VecContract[ListVec]/assoc-replaces-the-element-at-an-index VecContract[ListVec]/assoc-at-the-count-appends "
echo "plant-vec: $([ $fail = 0 ] && echo ok || echo FAILED)"
exit $fail
