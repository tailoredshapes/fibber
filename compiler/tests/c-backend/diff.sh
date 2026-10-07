#!/bin/bash
# The differential check of the C backend (docs/design/lir2c.md section 4): every accepted lIR program runs natively (`lairf run`,
# LLVM's JIT) and through C (`lir2c`, the C compiler, the executable), and the two runs must agree on standard output, standard error
# and the exit status (a fatal signal counts as 128 + N on both sides).
#   compiler/tests/c-backend/diff.sh [-j N] [--cc CC] [--emit DIR..] [--expect FILE] [--keep DIR] [--reject] [FILE.lir|DIR]..
#   FILE.lir | DIR     lIR programs; a directory gives its *.lir files with `;; expect: accept` (default: cases/lir and compiler/tests/c-backend/cases)
#   --emit DIR         also every fibber program DIR/*.fib with `;; expect: accept` or `trap`, through `$FIBC emit` (the lIR stage 2 makes)
#   --expect FILE      the programs that may end other than `pass`, one `VERDICT NAME` per line; any other ending fails the run
#   --keep DIR         keep the C and both runs' output there
#   --reject           also the `;; expect: reject` cases: lir2c must refuse each with lairf's exit status and text
#   --cc CC            the C compiler (default: $CC, else gcc); its flags are those of `fibc build --via c` (CFLAGS below)
# Environment: LAIRF, LIR2C, FIBC (for --emit), CC, DIFF_TIMEOUT (seconds, default 60), LIR2C_FLAGS (e.g. --allow-plain-tail-calls), CFLAGS
# (the whole C compiler flag list, in place of the default below: scripts/mutant-lir2c.sh musttail compiles at -O0).
# A row per program: `pass NAME`, `differ NAME: WHAT`, `refused NAME: WHAT` (lir2c reported an error), `cc-error NAME: WHAT` (the C
# compiler refused the C: a bug of the printer), `timeout NAME`, `skip NAME: WHY` (the native run timed out, or fibc could not emit);
# then the counts. Exit 0 when every ending is pass or listed in --expect, 1 otherwise, 2 for a usage error.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jobs=4; emits=(); expect=""; keep=""; inputs=(); kinds=accept; cc=${CC:-gcc}
while [ $# -gt 0 ]; do
  case $1 in
    -j) jobs=$2; shift ;;
    --cc) cc=$2; shift ;;
    --emit) emits+=("$2"); shift ;;
    --expect) expect=$2; shift ;;
    --keep) keep=$2; shift ;;
    --reject) kinds="accept|reject" ;;
    -h|--help) sed -n 2,18p "$0"; exit 0 ;;
    *) inputs+=("$1") ;;
  esac
  shift
done
: "${LAIRF:?set LAIRF to a lairf binary}" "${LIR2C:?set LIR2C to a lir2c binary}"
TMO=${DIFF_TIMEOUT:-60}
# The flags of `fibc build --via c` (compiler/driver/viac.fib `cc-flags`): the semantics need -ffp-contract=off (only an explicit fma fuses)
# and -fno-math-errno (sqrt is an instruction); -fno-strict-aliasing and -fno-delete-null-pointer-checks are belt and braces (the printer
# uses may_alias for every typed access, so no -fno-strict-aliasing: scripts/mutant-lir2c.sh strict-alias depends on that; and uintptr_t arithmetic). -Wno-unused: lIR binds names it never reads and the runtime defines what a program never calls.
CFLAGS=${CFLAGS:-"-std=gnu11 -O2 -march=native -ffp-contract=off -fno-math-errno -fno-delete-null-pointer-checks -Wall -Wextra -Wno-unused -Wno-psabi -Wno-stringop-overflow -Wno-array-bounds -Wno-tautological-compare -Wno-maybe-musttail-local-addr -Wno-maybe-uninitialized"}
[ ${#inputs[@]} -eq 0 ] && [ ${#emits[@]} -eq 0 ] && inputs=("$root/cases/lir" "$root/compiler/tests/c-backend/cases")
work=$(mktemp -d "${TMPDIR:-/tmp}/cdiff.XXXXXX"); trap 'rm -rf "$work"' EXIT
list=$work/list; : > "$list"
for x in "${inputs[@]}"; do
  if [ -d "$x" ]; then grep -lrE --include='*.lir' "^;; expect: ($kinds)" "$x" | sort >> "$list"; else echo "$x" >> "$list"; fi
done
for d in "${emits[@]}"; do
  : "${FIBC:?set FIBC for --emit}"
  if [ -f "$d" ]; then echo "$(cd "$(dirname "$d")" && pwd)/$(basename "$d")" >> "$list"; continue; fi
  ad=$(cd "$d" && pwd)
  grep -lE '^;; expect: +(accept|trap)' "$ad"/*.fib | sort >> "$list"
done

one() {
  f=$1; name=${f#"$root"/}; d=$work/run/$(echo "$name" | tr '/' '_'); mkdir -p "$d"
  ulimit -c 0; exec 2> /dev/null
  if [ "${f%.fib}" != "$f" ]; then
    inc=(); for r in $(sed -n 's/^;; roots: *//p' "$f" | head -1); do inc+=(-I "$(dirname "$f")/$r"); done
    (cd "$root" && timeout "$TMO" "$FIBC" emit "$f" "${inc[@]}") > "$d/e.lir" 2> "$d/e.err" || { echo "skip $name: fibc emit failed"; return; }
    f=$d/e.lir
  fi
  (timeout "$TMO" "$LAIRF" run "$f" > "$d/n.out" 2> "$d/n.err0") 2> /dev/null; ns=$?
  if [ $ns -eq 124 ]; then echo "skip $name: the native run timed out"; return; fi
  # `fatal signal N at 0x..` is written by lairf's own runtime (rt/thread.lir, the JIT shares its process), not by the program
  grep -v '^fatal signal [0-9]* at 0x' "$d/n.err0" > "$d/n.err"
  if grep -q '^;; expect: reject' "$f"; then
    "$LIR2C" "$f" -o "$d/p.c" > /dev/null 2> "$d/t.err"; ts=$?
    if [ $ts -eq $ns ] && cmp -s "$d/n.err" "$d/t.err"; then echo "pass $name"; else echo "differ $name: rejected with exit $ts, native $ns: $(head -c 150 "$d/t.err")"; fi
    return
  fi
  # shellcheck disable=SC2086
  if ! "$LIR2C" "$f" -o "$d/p.c" --cc "$cc" ${LIR2C_FLAGS:-} 2> "$d/t.err"; then echo "refused $name: $(grep -m1 'error:' "$d/t.err" | head -c 200)"; return; fi
  # shellcheck disable=SC2086
  if ! "$cc" $CFLAGS -o "$d/p" "$d/p.c" -lm -lpthread 2> "$d/cc.err"; then echo "cc-error $name: $(grep -m1 'error' "$d/cc.err" | head -c 200)"; return; fi
  (timeout "$TMO" "$d/p" > "$d/c.out" 2> "$d/c.err") 2> /dev/null; cs=$?
  if [ $cs -eq 124 ] && [ $ns -ne 124 ]; then echo "timeout $name: the C run took more than $TMO s"; return; fi
  what=""
  [ $ns -ne $cs ] && what="exit $cs, native $ns"
  cmp -s "$d/n.out" "$d/c.out" || what="${what:+$what; }stdout differs at $(cmp "$d/n.out" "$d/c.out" 2>&1 | grep -o 'byte [0-9]*' | head -1)"
  cmp -s "$d/n.err" "$d/c.err" || what="${what:+$what; }stderr differs: $(diff "$d/n.err" "$d/c.err" | head -3 | tr '\n' ' ' | head -c 200)"
  [ -s "$d/cc.err" ] && what="${what:+$what; }cc warned: $(grep -m1 'warning' "$d/cc.err" | head -c 160)"
  if [ -z "$what" ]; then echo "pass $name"; else echo "differ $name: $what"; fi
  [ -n "$keep" ] && mkdir -p "$keep" && cp -r "$d" "$keep/"
}
export -f one; export LAIRF LIR2C TMO root work keep FIBC cc CFLAGS LIR2C_FLAGS
xargs -P "$jobs" -I{} bash -c 'one "$@"' _ {} < "$list" > "$work/rows"
sort -k2 "$work/rows" > "$work/sorted"; cat "$work/sorted"
for v in pass differ refused cc-error timeout skip; do printf '%s %s  ' "$v" "$(grep -c "^$v " "$work/sorted")"; done; echo
if [ -n "$expect" ]; then
  grep -v '^pass ' "$work/sorted" | awk '{print $1, $2}' | sed 's/:$//' | sort > "$work/got"
  grep -v '^#' "$expect" | grep -v '^$' | sort > "$work/want"
  if ! cmp -s "$work/got" "$work/want"; then echo "FAIL: the endings other than pass differ from $expect:"; diff "$work/want" "$work/got"; exit 1; fi
  echo "OK: every ending other than pass is listed in $expect"; exit 0
fi
grep -qE '^(differ|refused|cc-error) ' "$work/sorted" && exit 1
exit 0
