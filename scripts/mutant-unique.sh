#!/bin/bash
# scripts/mutant-unique.sh: the mutation review of the in-place update (docs/design/in-place-update.md §6, "Mutation").
# Copies compiler/, lib/ and rt of this tree to a scratch directory, makes `fib.unique?` answer true where it must
# not, builds a stage 2 from the copy (nothing in the tree changes), and runs the persistence cases against it: every one of
# them must FAIL (a wrong answer, a trap or a crash all count). A case that still passes survived the mutant and is a bad case.
#
# usage: scripts/mutant-unique.sh [MODE] [CASE-PREFIX..]
#   MODE     always   (default) `fib.unique?` is true for every object: shared, immortal and stack ones as well
#            nocount  the flag test stays, the count test goes: an object with count 2 or more counts as unique. Case 4005 (tasks,
#                     atoms) is exempt here: what a task or an atom holds is SHARED, the flag test refuses it, and surviving
#                     this mutant is the flag test doing its work; `always` is what kills 4005.
#   MUT_DEMO=1   before the library does updates in place (L3, L4), nothing calls `fib.unique?` on a Vec or a Map, so the mutant
#            changes nothing and every case would survive. With MUT_DEMO=1 the scratch copy of lib/prelude.fib has every
#            `array-with` replaced by `awx`, a write through a cell with `array-set!` (correct today: the cell's content has a count
#            above 1, so it copies; in place exactly when `fib.unique?` says so, which is what the library levers do). The
#            cases must then pass unmutated and fail under the mutant. Drop it once the library is in place itself.
#   PREFIX   cases of cases/stdlib to run (default 4000- .. 4007-, the persistence cases T1..T7); `ownership/264-` names a case of
#            cases/ownership (default also 264-, 266-: the shell reuse and the steal through an Option payload)
# environment: FIBC (the fibc that builds the mutant; default the gate's F of this tree, else target/debug/fibc of the main
#   checkout), LAIR_DIR (the directory with liblair.so), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-unique-MODE),
#   MUT_J (cases at once, default 2).
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error. That the cases pass UNmutated is
#   shown by an ordinary `F cases cases/stdlib --only 4000- ..`; this script does not repeat it.
set -uo pipefail
MODE=always
case "${1:-}" in always|nocount) MODE=$1; shift;; esac
CASES=("$@")
[ ${#CASES[@]} -gt 0 ] || CASES=(4000- 4001- 4002- 4003- 4004- 4005- 4006- 4007- ownership/264- ownership/266-)
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-unique-$MODE}
main_target=$(cd "$R" && git rev-parse --git-common-dir | sed 's|/\.git$||')/target/debug
gate_f=$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F
if [ -z "${FIBC:-}" ]; then if [ -x "$gate_f" ]; then FIBC=$gate_f; else FIBC=$main_target/fibc; fi; fi
LAIR_DIR=${LAIR_DIR:-$main_target}
[ -x "$FIBC" ] || { echo "mutant-unique: no fibc to build with: set FIBC" >&2; exit 2; }
[ -e "$LAIR_DIR/liblair.so" ] || { echo "mutant-unique: no liblair.so in $LAIR_DIR: set LAIR_DIR" >&2; exit 2; }
export LD_LIBRARY_PATH=$LAIR_DIR

rm -rf "$OUT/tree"
mkdir -p "$OUT/tree" "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"
cp -r "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$R/cases/ownership" "$OUT/tree/cases/"
if [ -n "${MUT_DEMO:-}" ]; then
  perl -0pi -e 's/\(array-with /(awx /g; s/(\(defun vec-empty \(\) VecEmpty\))/$1\n\n(defun awx :private (items: (Array a) i: i64 x: a) -> (Array a)\n  (let ((c (cell items))) (do (array-set! &c i x) \@c)))/' "$OUT/tree/lib/prelude.fib"
  grep -c '(awx ' "$OUT/tree/lib/prelude.fib"
fi

# The two lines of fib.unique? (rt/core.lir, and its copy in the generated runtime.fib), each unique in its file:
#   (br (icmp ne (and f (i32 15)) (i32 0)) no test)               the flag test: a flagged object answers false at `no`
#   (block test (ret (icmp eq (load i64 ..) (i64 1))))            the count test: count is 1
mutate() { # mutate FILE
  local f=$1
  perl -0pi -e 's/\(i32 15\)\) \(i32 0\)\) no test\)/(i32 15)) (i32 0)) test test)/' "$f"
  perl -0pi -e 's/^  \(block test \(ret \(icmp eq \(load i64 .*$/  (block test (ret (i1 1))))/m' "$f"
  if [ "$MODE" = nocount ]; then   # undo the flag half: keep the flag test
    perl -0pi -e 's/\(i32 15\)\) \(i32 0\)\) test test\)/(i32 15)) (i32 0)) no test)/' "$f"
  fi
  grep -q '(block test (ret (i1 1))))' "$f" || { echo "mutant-unique: fib.unique? not found in $f (the runtime changed?)" >&2; exit 2; }
}
mutate "$OUT/tree/rt/core.lir"
mutate "$OUT/tree/compiler/emit/runtime.fib"
grep -A5 'define internal (fib.unique? i1)' "$OUT/tree/rt/core.lir"

cd "$OUT/tree" || exit 2
echo "mutant-unique[$MODE]: building the mutant stage 2 with $FIBC"
"$FIBC" build compiler/fibc.fib -I compiler -I lib -L "$LAIR_DIR" -l lair -o "$OUT/F" || { echo "mutant-unique: the mutant did not build" >&2; exit 2; }

export FIB_LIB=$OUT/tree/lib
survived=0
# the control: the same cases, the same library, the unmutated compiler: they must pass, or a "kill" proves nothing
for p in "${CASES[@]}"; do
  dir=cases/stdlib; q=$p; case $p in ownership/*) dir=cases/ownership; q=${p#ownership/};; esac
  res=$("$FIBC" cases "$dir" --only "$q" -j "${MUT_J:-2}" 2>&1)
  if echo "$res" | grep -q " 0 fail"; then echo "control   $p passes unmutated"; else echo "CONTROL FAILED $p: the case fails without the mutant"; echo "$res" | tail -5; exit 2; fi
done
for p in "${CASES[@]}"; do
  dir=cases/stdlib; q=$p; case $p in ownership/*) dir=cases/ownership; q=${p#ownership/};; esac
  res=$("$OUT/F" cases "$dir" --only "$q" -j "${MUT_J:-2}" 2>&1)
  rows=$(echo "$res" | grep -E "^$q" | cut -c1-150)
  if echo "$res" | grep -q " 0 fail"; then
    if [ "$MODE" = nocount ] && [ "$p" = 4005- ]; then echo "exempt    $rows (SHARED objects are refused by the flag test)"; continue; fi
    echo "SURVIVED  $rows"; survived=1; else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-unique[$MODE]: every case failed under the mutant"; else echo "mutant-unique[$MODE]: a case survived: rewrite it"; fi
exit $survived
