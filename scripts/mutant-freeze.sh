#!/bin/bash
# scripts/mutant-freeze.sh: planted faults for `freeze` and `frozen?` (P-count-b, docs/design/parallelism.md 3.6). Copies the tree to a scratch
# directory, breaks ONE rule in the copy, regenerates compiler/emit/runtime.fib from rt/, builds a stage 2 from the copy and runs the cases that
# pin the rule: every one must FAIL under the mutant (a wrong answer, a hang cut by the timeout, an aborted run, an audit leak or error all
# count). A case that still passes survived and is a bad case.
# usage: scripts/mutant-freeze.sh MODE
#   MODE            what is broken                                                                  cases that must fail
#   skip-children   freeze marks the root and not what is reachable from it                         7680- 7685- 7686-
#   keep-count      freeze sets IMMORTAL but leaves the count word as it was                        7680- 7685- 7686-
#   shared-mask     the walk stops at an object that is SHARED, as share-marking does               7686-
#   write-in-place  keep-count, and fib.unique? no longer refuses IMMORTAL: a frozen Vec is written 7680-
#   no-check        freeze does not look for a cell, atom, weak reference or task                   7682- 7683- 7684-
#   frozen-false    (frozen? x) always says false                                                   7680- 7685- 7686-
#   no-send         freeze loses its (Send a) bound: a cell inside is no longer refused by type     7681-
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default
#   ~/.cache/fibber-scratch/mutant-freeze-MODE), MUT_J (cases at once, default 1), MUT_TIMEOUT (seconds per case, default 300).
#   The cases run under `ulimit -v 16000000`. That the cases pass UNmutated is shown by an ordinary `F cases cases/stdlib --only 7680- ..`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-freeze.sh skip-children|keep-count|shared-mask|write-in-place|no-check|frozen-false|no-send}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-freeze-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-freeze: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  skip-children|keep-count|frozen-false) CASES=(7680- 7685- 7686-) ;;
  shared-mask)    CASES=(7686-) ;;
  write-in-place) CASES=(7680-) ;;
  no-check)       CASES=(7682- 7683- 7684-) ;;
  no-send)        CASES=(7681-) ;;
  *) echo "mutant-freeze: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-freeze: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
A=rt/atom.lir
case $MODE in
  skip-children)
    mut $A 's/\(block push\n    \(call \@fib\.wl-push wl p\)/(block push\n    (br (icmp eq mode (i64 3)) done pushreal))\n  (block pushreal\n    (call \@fib.wl-push wl p)/' ;;
  keep-count)
    mut $A 's/\(store \(i64 0\) \(getelementptr %struct\.fib\.hdr p \(i32 0\) \(i32 0\)\)\)\n    \(br \(icmp eq mode/(store (i64 1) (getelementptr %struct.fib.hdr p (i32 0) (i32 0)))\n    (br (icmp eq mode/' ;;
  shared-mask)
    mut $A 's/\(i32 12\) \(i32 8\)\)\)\)\)/(i32 13) (i32 9)))))/' ;;
  write-in-place)
    mut $A 's/\(store \(i64 0\) \(getelementptr %struct\.fib\.hdr p \(i32 0\) \(i32 0\)\)\)\n    \(br \(icmp eq mode/(store (i64 1) (getelementptr %struct.fib.hdr p (i32 0) (i32 0)))\n    (br (icmp eq mode/'
    mut rt/core.lir 's/\(br \(icmp ne \(and f \(i32 15\)\) \(i32 0\)\) no test\)/(br (icmp ne (and f (i32 7)) (i32 0)) no test)/' ;;
  no-check)
    mut $A 's/\(br \(icmp eq k \(i8 0\)\) done refuse\)/(br (icmp eq (i8 0) (i8 0)) done refuse)/' ;;
  frozen-false)
    mut $A 's/\(block entry \(ret \(icmp ne \(and \(call \@fib\.flags p\) \(i32 8\)\) \(i32 0\)\)\)\)\)/(block entry (ret (i1 0))))/' ;;
  no-send)
    mut compiler/types/builtins.fib 's/"\(\(Object a\) \(Send a\)\)"/"((Object a))"/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-freeze[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-freeze: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
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
if [ $survived = 0 ]; then echo "mutant-freeze[$MODE]: every case failed under the mutant"; else echo "mutant-freeze[$MODE]: a case survived: rewrite it"; fi
exit $survived
