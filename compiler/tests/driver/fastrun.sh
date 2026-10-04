#!/bin/bash
# Differential check of `fibc run -O 0` (the FastISel session, docs/design/dev-loop.md DV1) against `fibc run -O 2` (the full selector):
# every program of the sample must print the same standard output and exit with the same status at both levels. A miscompile by the
# fast code generator shows as a differing row and a nonzero exit.
# usage: compiler/tests/driver/fastrun.sh STAGE2     FIB_LIB must name lib/ (default: lib of this repository)
# Sample: the stdlib cases of the quick gate (every 10th by name, plus the expected failures: scripts/gate.sh), and the n-body shootout.
# FASTRUN_JOBS=N programs at a time (default 8); FASTRUN_ONLY=FILE names (one per line) instead of the gate's sample; scratch under
# $HOME/.cache/fibber-scratch/. Prints `ok NAME` or `FAIL NAME: ...`; exit 0 only if every program agreed and at least one ran.
f=${1:?usage: fastrun.sh STAGE2}
root=$(cd "$(dirname "$0")/../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
jobs=${FASTRUN_JOBS:-8}
mkdir -p "$HOME/.cache/fibber-scratch"
t=$(mktemp -d "$HOME/.cache/fibber-scratch/fastrun.XXXXXX"); trap 'rm -rf "$t"' EXIT
cd "$root" || exit 2

# The sample, as scripts/gate.sh builds it.
if [ -n "${FASTRUN_ONLY:-}" ]; then cp "$FASTRUN_ONLY" "$t/sample"; else
  names=$(cd cases/stdlib && for e in $(LC_ALL=C ls); do
            if [ -f "$e" ]; then case $e in *.fib) echo "$e" ;; esac; elif [ -f "$e/main.fib" ]; then echo "$e"; fi; done)
  step=$(( $(echo "$names" | wc -l) / 100 )); [ "$step" -lt 1 ] && step=1
  { echo "$names" | awk -v k="$step" 'NR % k == 0'
    sed -n 's|^cases/stdlib/\([^ ]*\) .*|\1|p' scripts/ci-stage2.expected | sed 's|/main.fib$||'; } | LC_ALL=C sort -u > "$t/sample"
fi
{ sed 's|^|cases/stdlib/|' "$t/sample"; echo scripts/shootout/n-body/n-body.fib; } | while read -r p; do
  if [ -d "$p" ]; then echo "$p/main.fib"; else echo "$p"; fi
done > "$t/progs"

# One program: stdout and status at -O 0 and at -O 2 (stderr is not compared: a trace or a message may name the level's timing).
one() {
  local f=$1 p=$2 t=$3 k o0 o2 s0 s2
  args=; case $p in *n-body.fib) args="-- 20000" ;; esac
  k=$(echo "$p" | tr '/' '_')
  (ulimit -v 16000000; timeout 300 "$f" run -O 0 -I cases/stdlib/support $p $args > "$t/$k.o0" 2> /dev/null; echo $? > "$t/$k.s0")
  (ulimit -v 16000000; timeout 300 "$f" run -O 2 -I cases/stdlib/support $p $args > "$t/$k.o2" 2> /dev/null; echo $? > "$t/$k.s2")
  s0=$(cat "$t/$k.s0"); s2=$(cat "$t/$k.s2")
  if [ "$s0" != "$s2" ]; then echo "FAIL $p: exit status -O 0 = $s0, -O 2 = $s2"
  elif ! cmp -s "$t/$k.o0" "$t/$k.o2"; then echo "FAIL $p: standard output differs ($(wc -c < "$t/$k.o0") and $(wc -c < "$t/$k.o2") bytes)"
  elif [ "$s0" = 124 ]; then echo "FAIL $p: timed out at both levels"
  else echo "ok $p (exit $s0, $(wc -c < "$t/$k.o0") bytes)"; fi
}
export -f one
xargs -a "$t/progs" -P "$jobs" -I{} bash -c 'one "$1" "$2" "$3"' _ "$f" {} "$t" | LC_ALL=C sort > "$t/result"
cat "$t/result"
n=$(wc -l < "$t/result"); bad=$(grep -c "^FAIL" "$t/result")
if grep -q "n-body.fib (exit [0-9]*, 0 bytes)" "$t/result"; then echo "FAIL n-body printed nothing: the check is vacuous"; bad=$((bad + 1)); fi
echo "fastrun: $n programs, $bad differ"
[ "$n" -gt 0 ] && [ "$bad" -eq 0 ]
