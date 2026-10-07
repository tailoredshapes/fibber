#!/bin/bash
# scripts/mutant-alloc-floor.sh: planted faults for the large-block cache's floor on musl (docs/design/allocator.md, section 9). Copies the tree to a scratch
# directory, breaks ONE rule in the copy, regenerates compiler/emit/runtime.fib from rt/, builds a stage 2 from the copy and runs the cases that pin the rule as
# STATIC executables (FIB_STATIC=1 FIB_STATIC_CASES=1, the musl pieces named by FIB_MUSL_DIR): every one must FAIL under the mutant. A case that still passes survived.
# usage: scripts/mutant-alloc-floor.sh MODE
#   MODE         what is broken                                                                          cases that must fail
#   floor-128k   the musl floor goes back to 128 KiB (the strings benchmark: 24,339 mmap calls again)    8060-
#   class-up     a freed block is filed in the class ABOVE its size: a reused block is too small        8061- 7794-
#   no-floor-op  `cache-floor` answers 131072 whatever the runtime holds                                8060-
# environment: FIBC (a fibc that builds the mutant; required), FIB_MUSL_DIR (required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-alloc-floor-MODE),
#   MUT_J (cases at once, default 1), MUT_TIMEOUT (seconds per case, default 300). The cases run under `ulimit -v 16000000`.
# exit: 0 when every case failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-alloc-floor.sh floor-128k|class-up|no-floor-op}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-alloc-floor-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-alloc-floor: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
[ -n "${FIB_MUSL_DIR:-}" ] || { echo "mutant-alloc-floor: set FIB_MUSL_DIR to the musl pieces (scripts/build-musl.sh)" >&2; exit 2; }
unset LD_LIBRARY_PATH
case $MODE in
  floor-128k)  CASES=(8060-) ;;
  class-up)    CASES=(8061- 7794-) ;;
  no-floor-op) CASES=(8060-) ;;
  *) echo "mutant-alloc-floor: unknown MODE $MODE" >&2; exit 2 ;;
esac

rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$R/rt" "$OUT/tree/"
cp -r "$R/cases/stdlib" "$OUT/tree/cases/"

# mut FILE PERL-EXPRESSION: apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-alloc-floor: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}
case $MODE in
  floor-128k)  mut compiler/emit/os.fib 's/\(i64 4096\)\)"\)/(i64 131072))")/' ;;
  class-up)    mut rt/alloc.lir 's/\(sub k \(i64 5\)\)/(sub k (i64 4))/' ;;
  no-floor-op) mut rt/alloc.lir 's/\(block floor \(ret \(load i64 \@fib\.lc-floor\)\)\)/(block floor (ret (i64 131072)))/' ;;
esac

cd "$OUT/tree" || exit 2
export FIB_LIB=$OUT/tree/lib
echo "mutant-alloc-floor[$MODE]: regenerating runtime.fib and building the mutant stage 2 with $FIBC"
{ "$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$OUT/gen-runtime" \
    && "$OUT/gen-runtime" rt > compiler/emit/runtime.fib \
    && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F"; } > "$OUT/build.log" 2>&1 \
  || { echo "mutant-alloc-floor: the mutant did not build"; tail -5 "$OUT/build.log"; exit 2; }
survived=0
ulimit -v 16000000
export MALLOC_ARENA_MAX=2
export FIB_STATIC=1 FIB_STATIC_CASES=1
for p in "${CASES[@]}"; do
  res=$(timeout "${MUT_TIMEOUT:-300}" "$OUT/F" cases cases/stdlib --only "$p" -j "${MUT_J:-1}" 2>&1)
  rc=$?
  rows=$(echo "$res" | grep -E "^$p" | cut -c1-220)
  if [ $rc = 124 ]; then echo "killed    $p (hung: cut by the ${MUT_TIMEOUT:-300} s timeout)"
  elif echo "$res" | grep -q " 0 fail"; then echo "SURVIVED  $rows"; survived=1
  else echo "killed    ${rows:-$p ($(echo "$res" | tail -1))}"; fi
done
if [ $survived = 0 ]; then echo "mutant-alloc-floor[$MODE]: every case failed under the mutant"; else echo "mutant-alloc-floor[$MODE]: a case survived: rewrite it"; fi
exit $survived
