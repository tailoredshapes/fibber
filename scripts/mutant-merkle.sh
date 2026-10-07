#!/bin/bash
# scripts/mutant-merkle.sh: planted faults in fib.merkle (docs/design/merkle.md) and the durability calls of fib.os.files. Copies lib/ of this
# tree to a scratch directory, applies ONE mutant, runs cases 8320 to 8322 with a stage 2 (FIBC; default the gate's F of this tree) pointing
# FIB_LIB at the copy: a case must FAIL. A mutant under which every case passes is a part no test can fail.
# usage: scripts/mutant-merkle.sh [MUTANT..]        (default: all)
#   leaf-tag        node: a leaf is written with the branch's tag            branch-swapped   tree: a carry puts the new subtree on the left
#   no-carry        tree: equal heights are never merged                      fold-range       tree: the folded root's range starts at the right
#   proof-side      tree: a right sibling is reported on the left             verify-blind     tree: verify answers true without hashing
#   order-unchecked tree: an offset that is not after the last is taken      leaf-offset      node: a leaf's offset is not part of its bytes
#   lock-waits      os: flock without LOCK_NB                                 rename-swapped   os: rename moves `to` over `from`
#   truncate-noop   os: ftruncate is not called
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-merkle: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-merkle}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(leaf-tag branch-swapped no-carry fold-range proof-side verify-blind order-unchecked leaf-offset lock-waits
                                     rename-swapped truncate-noop)

sub() { perl -0pi -e "$2" "$1"; cmp -s "$1" "$1.orig" && { echo "mutant-merkle: pattern not found in $1: $2" >&2; return 1; }; return 0; }
mutate() { # mutate NAME TREE
  local f d=$2/lib/fib/merkle o=$2/lib/fib/os/files.fib
  case $1 in
    leaf-tag)        f=$d/node.fib; cp $f $f.orig; sub $f 's/\(concat-bytes \[\(le 0 4\)/(concat-bytes [(le 1 4)/' ;;
    leaf-offset)     f=$d/node.fib; cp $f $f.orig; sub $f 's/\(raw item\) \(le offset 8\)/(raw item) (le 0 8)/' ;;
    branch-swapped)  f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(MerkleBranch \(\. top digest\) d /(MerkleBranch d (. top digest) /' ;;
    no-carry)        f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(\(some top\) \(= \(\. top height\) height\)\)/((some top) false)/' ;;
    fold-range)      f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(MerkleBranch \(\. p digest\) \(\. acc digest\) \(\. p first\) \(\. acc last\)\)/(MerkleBranch (. p digest) (. acc digest) (. acc first) (. acc last))/' ;;
    proof-side)      f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(recur l \(conj path \(sibling-of rn r OnRight\)\)\)/(recur l (conj path (sibling-of rn r OnLeft)))/' ;;
    verify-blind)    f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(= \(\. end digest\) \(\. p root\)\)\)\)/true))/' ;;
    order-unchecked) f=$d/tree.fib; cp $f $f.orig; sub $f 's/\(if \(<= offset \(\. top last\)\) \(trap/(if false (trap/' ;;
    lock-waits)      f=$o; cp $f $f.orig; sub $f 's/\(flock \(trunc i32 fd\) 6i32\)/(flock (trunc i32 fd) 2i32)/' ;;
    rename-swapped)  f=$o; cp $f $f.orig; sub $f 's/\(rename a b\)/(rename b a)/' ;;
    truncate-noop)   f=$o; cp $f $f.orig; sub $f 's/\(c-unit \(unsafe \(sext i64 \(ftruncate \(trunc i32 fd\) size\)\)\) "ftruncate"\)/(Ok ())/' ;;
    *) echo "mutant-merkle: unknown mutant $1" >&2; return 1 ;;
  esac
}

survived=0
for m in "${MUTANTS[@]}"; do
  t=$OUT/$m; rm -rf "$t"; mkdir -p "$t/cases/stdlib"; cp -r "$R/lib" "$t/lib"
  # the cases find the library by their `roots: ../../lib`, so they run from beside the copy
  cp -r "$R/cases/stdlib/support" "$R"/cases/stdlib/832[012]-*.fib "$t/cases/stdlib/"
  mutate "$m" "$t" || { echo "SETUP ERROR $m"; exit 2; }
  log=$t/cases.log
  # lock-waits would block on the second lock: the timeout kills it, which counts as a failure.
  (cd "$t" && FIB_LIB=$t/lib timeout 600 "$FIBC" cases cases/stdlib > "$log" 2>&1) && rc=0 || rc=$?
  if [ $rc -ne 0 ] || ! grep -q "0 fail, 0 pending, 0 header error" "$log"; then echo "killed   $m   ($(grep -m1 ' FAIL ' "$log" | cut -c1-110))"
  else echo "SURVIVED $m"; survived=1; fi
  rm -rf "$t/lib" "$t/cases"
done
[ $survived = 0 ]
