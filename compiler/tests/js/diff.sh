#!/bin/bash
# The differential check of the JavaScript backend (docs/design/js-backend.md, "The differential harness"): every accepted lIR program runs
# natively (`lairf run`, LLVM's JIT) and as JavaScript (`lir2js`, then node), and the two runs must agree on standard output, standard error
# and the exit status (a fatal signal counts as 128 + N on both sides).
#   compiler/tests/js/diff.sh [-j N] [--emit DIR..] [--expect FILE] [--keep DIR] [FILE.lir|DIR]..
#   FILE.lir | DIR     lIR programs; a directory gives its *.lir files with `;; expect: accept` (default: cases/lir and compiler/tests/js/cases)
#   --emit DIR         also every fibber program DIR/*.fib with `;; expect: accept` or `trap`, through `$FIBC emit` (the lIR stage 2 makes)
#   --expect FILE      the programs that may end other than `pass`, one `VERDICT NAME` per line; any other ending fails the run
#   --keep DIR         keep the translated .mjs and both runs' output there
#   --reject           also the `;; expect: reject` cases of the directories: lir2js must refuse each with lairf's exit status and text
# Environment: LAIRF, LIR2JS, FIBC (for --emit), NODE (default node), DIFF_TIMEOUT (seconds, default 30), RT (the runtime dir).
# A row per program: `pass NAME`, `differ NAME: WHAT`, `unsupported NAME: WHAT` (the JS run stopped at `lir2js: unsupported`), `timeout NAME`
# (the JS run alone exceeded the time limit),
# `skip NAME: WHY` (the native run itself timed out, or fibc could not emit); then the counts. Exit 0 when every ending is pass or listed in
# --expect, 1 otherwise, 2 for a usage error.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jobs=4; emits=(); expect=""; keep=""; inputs=(); kinds=accept
while [ $# -gt 0 ]; do
  case $1 in
    -j) jobs=$2; shift ;;
    --emit) emits+=("$2"); shift ;;
    --expect) expect=$2; shift ;;
    --keep) keep=$2; shift ;;
    --reject) kinds="accept|reject" ;;
    -h|--help) sed -n 2,15p "$0"; exit 0 ;;
    *) inputs+=("$1") ;;
  esac
  shift
done
: "${LAIRF:?set LAIRF to a lairf binary}" "${LIR2JS:?set LIR2JS to a lir2js binary}"
NODE=${NODE:-node}; TMO=${DIFF_TIMEOUT:-30}; RT=${RT:-$root/compiler/js/rt}
[ ${#inputs[@]} -eq 0 ] && [ ${#emits[@]} -eq 0 ] && inputs=("$root/cases/lir" "$root/compiler/tests/js/cases")
work=$(mktemp -d "${TMPDIR:-/tmp}/jsdiff.XXXXXX"); trap 'rm -rf "$work"' EXIT
list=$work/list; : > "$list"
for x in "${inputs[@]}"; do
  if [ -d "$x" ]; then grep -lrE --include='*.lir' "^;; expect: ($kinds)" "$x" | sort >> "$list"; else echo "$x" >> "$list"; fi
done
# The fibber programs: each is emitted by its own job (one() below); a failed emit is a skip row.
for d in "${emits[@]}"; do
  : "${FIBC:?set FIBC for --emit}"
  if [ -f "$d" ]; then echo "$(cd "$(dirname "$d")" && pwd)/$(basename "$d")" >> "$list"; continue; fi
  ad=$(cd "$d" && pwd)
  grep -lE '^;; expect: +(accept|trap)' "$ad"/*.fib | sort >> "$list"
done

# One program: both runs, compared. Prints its row.
one() {
  f=$1; name=${f#"$root"/}; d=$work/run/$(echo "$name" | tr '/' '_'); mkdir -p "$d"
  ulimit -c 0; exec 2> /dev/null   # bash reports a child killed by a signal on its standard error; both runs' own stderr go to files
  if [ "${f%.fib}" != "$f" ]; then
    inc=(); for r in $(sed -n 's/^;; roots: *//p' "$f" | head -1); do inc+=(-I "$(dirname "$f")/$r"); done   # the case's module roots
    (cd "$root" && timeout "$TMO" "$FIBC" emit "$f" "${inc[@]}") > "$d/e.lir" 2> "$d/e.err" || { echo "skip $name: fibc emit failed"; return; }
    f=$d/e.lir
  fi
  (timeout "$TMO" "$LAIRF" run "$f" > "$d/n.out" 2> "$d/n.err") 2> /dev/null; ns=$?
  if [ $ns -eq 124 ]; then echo "skip $name: the native run timed out"; return; fi
  if grep -q '^;; expect: reject' "$f"; then   # both must refuse it with the same diagnostics: lir2js reads it with lair's reader and checker
    "$LIR2JS" "$f" -o "$d/p.mjs" --rt "$RT" > /dev/null 2> "$d/t.err"; ts=$?
    if [ $ts -eq $ns ] && cmp -s "$d/n.err" "$d/t.err"; then echo "pass $name"; else echo "differ $name: rejected with exit $ts, native $ns: $(head -c 150 "$d/t.err")"; fi
    return
  fi
  if ! "$LIR2JS" "$f" -o "$d/p.mjs" --rt "$RT" 2> "$d/t.err"; then echo "differ $name: lir2js failed: $(head -c 200 "$d/t.err")"; return; fi
  (timeout "$TMO" "$NODE" --stack-size=7000 "$d/p.mjs" > "$d/j.out" 2> "$d/j.err") 2> /dev/null; js=$?
  if [ $js -eq 124 ] && [ $ns -ne 124 ]; then echo "timeout $name: the JS run took more than $TMO s"; return; fi
  if grep -q 'lir2js: unsupported' "$d/j.err"; then echo "unsupported $name: $(grep -m1 -o 'lir2js: unsupported.*' "$d/j.err" | head -c 160)"; return; fi
  what=""
  [ $ns -ne $js ] && what="exit $js, native $ns"
  cmp -s "$d/n.out" "$d/j.out" || what="${what:+$what; }stdout differs at $(cmp "$d/n.out" "$d/j.out" 2>&1 | grep -o 'byte [0-9]*' | head -1)"
  cmp -s "$d/n.err" "$d/j.err" || what="${what:+$what; }stderr differs: $(diff "$d/n.err" "$d/j.err" | head -3 | tr '\n' ' ' | head -c 200)"
  if [ -z "$what" ]; then echo "pass $name"; else echo "differ $name: $what"; fi
  [ -n "$keep" ] && mkdir -p "$keep" && cp -r "$d" "$keep/"
}
export -f one; export LAIRF LIR2JS NODE TMO RT root work keep FIBC
xargs -P "$jobs" -I{} bash -c 'one "$@"' _ {} < "$list" > "$work/rows"
[ -f "$work/skips" ] && cat "$work/skips" >> "$work/rows"
sort -k2 "$work/rows" > "$work/sorted"; cat "$work/sorted"
for v in pass differ unsupported timeout skip; do printf '%s %s  ' "$v" "$(grep -c "^$v " "$work/sorted")"; done; echo
if [ -n "$expect" ]; then
  grep -v '^pass ' "$work/sorted" | awk '{print $1, $2}' | sed 's/:$//' | sort > "$work/got"
  grep -v '^#' "$expect" | grep -v '^$' | sort > "$work/want"
  if ! cmp -s "$work/got" "$work/want"; then echo "FAIL: the endings other than pass differ from $expect:"; diff "$work/want" "$work/got"; exit 1; fi
  echo "OK: every ending other than pass is listed in $expect"; exit 0
fi
grep -q '^differ ' "$work/sorted" && exit 1
exit 0
