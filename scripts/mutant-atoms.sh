#!/bin/bash
# scripts/mutant-atoms.sh: planted faults for the lock-free scalar atoms, the lock of the object atoms and the adder (P-count-a,
# docs/design/parallelism.md 3.6). Copies the tree to a scratch directory, breaks ONE rule in the copy, regenerates compiler/emit/runtime.fib
# from rt/, builds a stage 2 from the copy and runs the cases that pin the rule: every one must FAIL under the mutant (a wrong answer, a
# hang cut by the timeout, an aborted run, an audit leak or error all count). A case that still passes survived and is a bad case.
# usage: scripts/mutant-atoms.sh MODE
#   MODE             what is broken                                                              cases that must fail
#   cas-blind        a scalar compare-and-swap stores without comparing and says true            7620- 7622- 7623-
#   cas-says-true    the compare-and-swap is real but its answer is always true                  7620- 7623-
#   swap-no-retry    a scalar swap! that lost the race does not retry (takes the lost swap)      7622-
#   read-first       a read of a scalar atom answers 0 (the load is dropped)                    7620-
#   cas-obj-leak     compare-and-set! of an object atom does not release `new` when it fails     7621-
#   cas-obj-keep-old compare-and-set! of an object atom does not release the old content        7621-
#   cas-obj-bits     compare-and-set! of an object atom says true without comparing              7621-
#   lock-none        fib.lock does not lock (object atoms, weak boxes, task results)             7624-
#   unlock-none      fib.unlock does not release the lock word                                   7624- (hangs: cut by the timeout)
#   adder-lost       `add!` reads the stripe and writes it back (no atomic add)                  7625-
#   adder-one        `sum` adds the first stripe only                                            7625-
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-atoms-MODE), MUT_J (cases at once, default 1), MUT_TIMEOUT (seconds per case, default 300).
#   The cases run under `ulimit -v 16000000`. That the cases pass UNmutated is shown by an ordinary `F cases cases/stdlib --only 7620- ..`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-atoms.sh cas-blind|cas-says-true|swap-no-retry|read-first|cas-obj-leak|cas-obj-keep-old|cas-obj-bits|lock-none|unlock-none|adder-lost|adder-one}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-atoms-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-atoms: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  cas-blind)        CASES=(7620- 7622- 7623-) ;;
  cas-says-true)    CASES=(7620- 7623-) ;;
  swap-no-retry)    CASES=(7622-) ;;
  read-first)       CASES=(7620-) ;;
  cas-obj-leak|cas-obj-keep-old) CASES=(7621-) ;;
  cas-obj-bits)     CASES=(7621-) ;;
  lock-none)        CASES=(7624-) ;;
  unlock-none)      CASES=(7624-) ;;
  adder-lost|adder-one) CASES=(7625-) ;;
  *) echo "mutant-atoms: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-atoms: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
A=compiler/emit/lower/atoms.fib
C=compiler/emit/lower/cells.fib
case $MODE in
  cas-blind)
    mut $A 's/\(fb-val \(\. cx b\) \(str-join \["\(extractvalue \(cmpxchg seq_cst seq_cst " vp " " \(v-text o\) " " \(v-text n\) "\) 1\)"\]\) LirI1\)/(do (fb-stmt (. cx b) (str-join ["(atomic-store seq_cst " (v-text n) " " vp ")"])) (fb-val (. cx b) "(icmp eq (i1 1) (i1 1))" LirI1))/' ;;
  cas-says-true)
    mut $A 's/"\(extractvalue \(cmpxchg seq_cst/"(or (i1 1) (extractvalue (cmpxchg seq_cst/; s/"\) 1\)"\]\) LirI1\)/") 1))"]) LirI1)/' ;;
  swap-no-retry)
    mut $A 's/\(str-join \["\(br " \(v-text ok\) " " done " " head "\)"\]\)/(str-join ["(br " (v-text ok) " " done " " done ")"])/' ;;
  read-first)
    mut $A 's/\(atomic-load seq_cst " \(lir-ty-text it\) " " vp "\)"/(add (" (lir-ty-text it) " 0) (" (lir-ty-text it) " 0))"/' ;;
  cas-obj-leak)
    mut $C 's/\(lcx-release cx new\)\n(\s*\(fb-term b \(str-join \["\(br " ljoin)/$1/' ;;
  cas-obj-keep-old)
    mut $C 's/\(lcx-release cx cur\)\n//' ;;
  cas-obj-bits)
    mut $C 's/\(v-text \(same-bits cx cur old\)\) " " lyes/"(i1 1)" " " lyes/' ;;
  lock-none)
    mut rt/atom.lir 's/\(define internal \(fib\.lock void\) \(\(ptr lockp\)\)\n  \(block entry \(br spin\)\)/(define internal (fib.lock void) ((ptr lockp))\n  (block entry (ret))/' ;;
  unlock-none)
    mut rt/atom.lir 's/\(block entry \(atomic-store release \(i32 0\) lockp\) \(ret\)\)\)/(block entry (ret)))/' ;;
  adder-lost)
    mut lib/fib/adder.fib 's/\(do \(swap! \(nth \(\. a cells\) \(\* 2 \(bit-and \(sys-thread-stripe\) 31\)\)\) \(fn \(v\) \(\+ v n\)\)\)\n      \(\)\)\)/(let ((c (nth (. a cells) (* 2 (bit-and (sys-thread-stripe) 31))))) (reset! c (+ @c n))))/' ;;
  adder-one)
    mut lib/fib/adder.fib 's/\(if \(< i \(count cells\)\) \(recur \(\+ i 1\) \(\+ s \@\(nth cells i\)\)\) s\)/(if (< i 1) (recur (+ i 1) (+ s \@(nth cells i))) s)/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-atoms[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-atoms: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
export MALLOC_ARENA_MAX=2
for p in "${CASES[@]}"; do
  res=$(timeout "${MUT_TIMEOUT:-300}" "$OUT/F" cases cases/stdlib --only "$p" -j "${MUT_J:-1}" 2>&1)
  rc=$?
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-220)
  if [ $rc = 124 ]; then echo "killed    $p (hung: cut by the ${MUT_TIMEOUT:-300} s timeout)"
  elif echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1
  else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-atoms[$MODE]: every case failed under the mutant"; else echo "mutant-atoms[$MODE]: a case survived: rewrite it"; fi
exit $survived
