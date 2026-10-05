#!/bin/bash
# scripts/mutant-impl-escape.sh: the mutation review of static dispatch on the implementation's escape facts (spec/types.md §6.4,
# lever C of batch 4). Copies compiler/, lib/ and rt of this tree to a scratch directory, breaks one rule of
# compiler/own/walk/callee.fib or compiler/own/top.fib, builds a stage 2 from the copy (nothing in the tree changes), and runs the
# cases that rule is for against it: every one of them must FAIL (a wrong answer, an audit failure or a crash all count). A case that
# still passes survived the mutant and is a bad case.
#
# usage: scripts/mutant-impl-escape.sh MODE [CASE-PREFIX..]     (prefixes of cases/ownership)
#   MODE  noescape       every implementation's parameter is taken as not escaping, whatever its body does: 281, 282 (a receiver or an
#                        argument the implementation stores must stay on the heap) fail
#         wrong-instance a call reads the facts of the first instance of the protocol, not the one it resolves to: 283 fails
#         not-ready      an impl method is decided ahead of its caller even when a `defun` it names has no summary yet: 284 fails
# environment: FIBC (the fibc that builds the mutant; default the gate's F of this tree),
#   MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-impl-escape-MODE), MUT_J (cases at once, default 2).
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is shown by
#   an ordinary `F cases cases/ownership --only 280- ..`; this script does not repeat it.
set -uo pipefail
MODE=${1:?usage: mutant-impl-escape.sh noescape|wrong-instance|not-ready [CASE-PREFIX..]}; shift
case $MODE in
  noescape) DEFAULT=(281- 282-) ;;
  wrong-instance) DEFAULT=(283-) ;;
  not-ready) DEFAULT=(284-) ;;
  *) echo "mutant-impl-escape: unknown mode $MODE" >&2; exit 2 ;;
esac
CASES=("$@"); [ ${#CASES[@]} -gt 0 ] || CASES=("${DEFAULT[@]}")
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-impl-escape-$MODE}
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
if [ -z "${FIBC:-}" ]; then if [ -x "$gate_f" ]; then FIBC=$gate_f; else echo "mutant-impl-escape: no fibc: set FIBC (a stage 2, or the seed scripts/fetch-seed.sh fetches) or run scripts/gate.sh first" >&2; exit 2; fi; fi
[ -x "$FIBC" ] || { echo "mutant-impl-escape: no fibc to build with: set FIBC" >&2; exit 2; }
unset LD_LIBRARY_PATH

rm -rf "$OUT/tree"
mkdir -p "$OUT/tree" "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"
cp -r "$R/rt" "$OUT/tree/"
cp -r "$R/cases/ownership" "$OUT/tree/cases/"
callee=$OUT/tree/compiler/own/walk/callee.fib
top=$OUT/tree/compiler/own/top.fib
replace() { # replace FILE OLD NEW: the one occurrence of the literal OLD becomes NEW
  OLD=$2 NEW=$3 perl -0pi -e 'BEGIN { $o = $ENV{OLD}; $n = $ENV{NEW}; $c = 0 } $c += s/\Q$o\E/$n/g; END { exit($c == 1 ? 0 : 1) }' "$1" \
    || { echo "mutant-impl-escape: the text to mutate is not in $1 (the rule moved?): $2" >&2; exit 2; }
}
case $MODE in
  noescape)
    replace "$callee" '(and (. (nth ps j) escapes) (. (nth (. b params) j) escapes))' 'false' ;;
  wrong-instance)
    # the walker and the order of the units (own.refs) both take the first instance of the protocol for the one the call resolves to
    refs=$OUT/tree/compiler/own/refs.fib
    replace "$callee" '(match (get (. (w-p w) resolutions) (. head id))
       ((some (ResInstance k _))' '(match (first-instance-of (w-p w) c)
       ((some (ResInstance k _))'
    replace "$refs" '(:use types.ast' '(:use own.program types.ast'
    replace "$refs" '((EGlobal (GMethod _ i))
                (match (get (. p resolutions) (. e id))' '((EGlobal (GMethod pr i))
                (match (first-instance-of p (CalMethod pr i))'
    cat >> "$refs" <<'EOF'

(defun first-instance-of (p: TypedProgram c: Callee) -> (Option Resolution)
  (match c
    ((CalMethod pr _)
     (let [insts (table-rows (. (. p globals) instances))]
       (match (find-first (fn (k: i64) (= (. (nth insts k) proto) pr)) (range (count insts)))
         ((some k) (some (ResInstance k [])))
         (_ nil))))
    (_ nil)))
EOF
    ;;
  not-ready)
    replace "$top" '(every? (fn (f: i64) (contains? (. (. t prog) summaries) f)) (vec (. r funs)))' 'true' ;;
esac
cd "$OUT/tree" || exit 2
echo "mutant-impl-escape: building the $MODE mutant"
if ! "$FIBC" build compiler/fibc.fib -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$OUT/F" > "$OUT/build.log" 2>&1; then
  tail -n 20 "$OUT/build.log"; echo "mutant-impl-escape: the mutant did not build"; exit 2
fi
export FIB_LIB=$OUT/tree/lib
survived=0
for c in "${CASES[@]}"; do
  out=$("$OUT/F" cases cases/ownership --only "$c" -j "${MUT_J:-2}" 2>&1)
  if echo "$out" | grep -qE '^[^ ]+ +pass *$'; then echo "SURVIVED  $c"; survived=1; else echo "killed    $c"; fi
done
[ $survived = 0 ] && echo "mutant-impl-escape: $MODE killed every case" || echo "mutant-impl-escape: $MODE survived"
exit $survived
