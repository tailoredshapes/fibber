#!/bin/bash
# h1-ast-vs-text.sh: the gate of the in-memory hand-over (docs/design/lair-in-fibber.md 1.4, stage 9): the lIR trees the emitter gives a
# `def`'s JIT session (emit.defs.astmod) print, byte for byte, as the text it renders, over the emit corpus.
#   compiler/tests/native/h1-ast-vs-text.sh FIBC [--fault] [-j N] [UNIT..]
#   FIBC     a stage 2 (`fibc build compiler/fibc.fib ...`), with FIB_LIB set as for `run`
#   UNIT     a .fib program (a directory stands for its main.fib); default: cases/ownership, cases/modules, every 10th of cases/stdlib
#            and compiler/fibc.fib
#   --fault  FIB_AST_VERIFY=fault: the tree builder leaves the runtime out; every unit that emits must then be REJECTED by the
#            comparison (a check that cannot fail is worse than none)
# Per unit, `FIBC emit UNIT` is run twice: plain (the text path, no trees compared) and with FIB_AST_VERIFY=1, which makes every
# session, and the final module, compare `lir.print` of its trees with `lir.print` of the whole text read again (emit.defs.astmod
# `verify-module`: the pieces also join to the text of the parts). SAME: the same status and standard output, no FIB_AST_VERIFY
# message. DIFF otherwise. NOEMIT: the unit does not emit (a case that is meant to be rejected): both runs agree on that and it is
# not counted as a pass. Exit 0 only when there is at least one SAME and no DIFF (with --fault: at least one DETECTED and no MISSED).
set -u
fibc=${1:?usage: h1-ast-vs-text.sh FIBC [--fault] [-j N] [UNIT..]}; shift
fault=; jobs=4
while [ $# -gt 0 ]; do
  case $1 in --fault) fault=1 ;; -j) jobs=${2:?}; shift ;; *) break ;; esac
  shift
done
[ "$jobs" -gt 4 ] && jobs=4
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
cd "$root" || exit 2
export FIB_LIB=${FIB_LIB:-$root/lib}
T=$(mktemp -d "${TMPDIR:-/tmp}/h1-ast.XXXXXX"); trap 'rm -rf "$T"' EXIT
if [ $# -gt 0 ]; then units=("$@"); else
  units=(compiler/fibc.fib cases/ownership/*.fib cases/modules/*/main.fib)
  i=0; for f in cases/stdlib/*.fib; do i=$((i + 1)); [ $((i % 10)) -eq 0 ] && units+=("$f"); done
fi
one() {
  local u=$1 n; n=$(echo "$u" | tr '/ ' '__'); [ -d "$u" ] && u=$u/main.fib
  ( ulimit -v 12000000; timeout 600 "$fibc" emit "$u" > "$T/$n.plain" 2> "$T/$n.plain.err"; echo $? > "$T/$n.plain.st" )
  ( ulimit -v 12000000; FIB_AST_VERIFY=${FAULT:-1} timeout 900 "$fibc" emit "$u" > "$T/$n.ver" 2> "$T/$n.ver.err"; echo $? > "$T/$n.ver.st" )
  local ps vs; ps=$(cat "$T/$n.plain.st"); vs=$(cat "$T/$n.ver.st")
  if [ "$ps" != 0 ]; then
    [ "$ps" = "$vs" ] && echo "NOEMIT $u" || echo "DIFF $u: plain status $ps, verifying status $vs"
  elif [ -n "${FAULT:-}" ]; then
    if [ "$vs" != 0 ] && grep -q 'FIB_AST_VERIFY' "$T/$n.ver.err"; then echo "DETECTED $u"; else echo "MISSED $u: status $vs"; fi
  elif [ "$ps" != "$vs" ] || ! cmp -s "$T/$n.plain" "$T/$n.ver" || grep -q 'FIB_AST_VERIFY' "$T/$n.ver.err"; then
    echo "DIFF $u: status $ps/$vs; $(grep -m1 FIB_AST_VERIFY "$T/$n.ver.err" | cut -c1-300)"
  else echo "SAME $u"; fi
}
export -f one; export T fibc FAULT=${fault:+fault}
printf '%s\n' "${units[@]}" | xargs -P "$jobs" -I{} bash -c 'one "$1"' _ {} > "$T/results"
grep -v '^\(SAME\|DETECTED\|NOEMIT\)' "$T/results" | head -20
same=$(grep -c '^SAME' "$T/results"); det=$(grep -c '^DETECTED' "$T/results"); noemit=$(grep -c '^NOEMIT' "$T/results")
bad=$(grep -c '^\(DIFF\|MISSED\)' "$T/results")
echo "units $(wc -l < "$T/results"): SAME $same, DETECTED $det, NOEMIT $noemit, DIFF/MISSED $bad"
if [ -n "$fault" ]; then [ "$det" -gt 0 ] && [ "$bad" -eq 0 ]; else [ "$same" -gt 0 ] && [ "$bad" -eq 0 ]; fi
