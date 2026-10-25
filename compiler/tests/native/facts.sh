#!/bin/bash
# The facts of lIR (spec/lir.md 6.13: `nsw`/`nuw`, `assume`, `(load (range LO HI) ..)`) through every backend:
#   1. `lairf cases cases/lir/facts cases/lir/facts-verify`: parse, check (each rejection), print, and lowering in the three check modes
#      (`use` puts LLVM's flags, `llvm.assume` and `!range` in the IR; `trap` traps on a false fact with a message naming it and its place;
#      `ignore` runs the plain program), each case with its verdict in its header;
#   2. the controls of the check mode, outside the harness: trap-nsw.lir traps under FIB_LIR_FACTS=trap and runs to its wrapped answer under
#      ignore; an unknown FIB_LIR_FACTS value is refused;
#   3. the other backends take lIR that carries facts: lir2c and lir2js translate cases/lir/facts/use.lir (dropping the facts) and the C
#      program and the ES module print what lair's does.
# usage: facts.sh FIBC     L=lairf (LAIRF also read; unset: built from compiler/lairf.fib by FIBC). Exit 0 all hold, 1 a check failed, 2 no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root" || exit 2
F=${1:-${F:-${FIBC:-fibc}}}; L=${L:-${LAIRF:-}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "facts: no fibc: set FIBC" >&2; exit 2; }
export FIB_LIB=$root/lib
lib=${LLVMLIB:-${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}}
T=$(mktemp -d "${TMPDIR:-/tmp}/facts.XXXXXX"); trap 'rm -rf "$T"' EXIT
ulimit -v 16000000 2>/dev/null; ulimit -c 0
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
if [ -z "$L" ]; then
  timeout 900 "$F" build compiler/lairf.fib -I compiler -I lib -L "$lib" -l LLVM-21 -o "$T/lairf" > "$T/lairf.log" 2>&1 || { tail -5 "$T/lairf.log"; echo "facts: lairf did not build" >&2; exit 2; }
  L=$T/lairf
fi
export LD_LIBRARY_PATH=$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
unset FIB_LIR_FACTS

# 1. the cases
if FIB_LAIR_WORKERS=4 timeout 900 "$L" cases cases/lir/facts cases/lir/facts-verify > "$T/cases.out" 2>&1; then
  ok "lIR fact cases: $(grep -c '^ok ' "$T/cases.out") pass"
else
  grep '^FAIL' "$T/cases.out" | head -20; no "lIR fact cases ($(tail -1 "$T/cases.out"))"
fi

# 2. the check mode's controls
FIB_LIR_FACTS=trap timeout 60 "$L" run cases/lir/facts/trap-nsw.lir > "$T/t.out" 2> "$T/t.err"; st=$?
if [ "$st" -ge 128 ] && grep -q 'lIR fact violated: (add nsw) in @main' "$T/t.err"; then ok "trap: a false nsw traps (status $st) with its message"
else no "trap: a false nsw gave status $st, stderr: $(head -c 300 "$T/t.err")"; fi
got=$(FIB_LIR_FACTS=ignore timeout 60 "$L" run cases/lir/facts/trap-nsw.lir 2>&1); st=$?
if [ "$st" -eq 0 ] && [ "$got" = "-9223372036854775808" ]; then ok "ignore: the same program runs to the wrapped sum"
else no "ignore: status $st, output $got"; fi
got=$(FIB_LIR_FACTS=sometimes timeout 60 "$L" emit-llvm cases/lir/facts/use.lir 2>&1); st=$?
if [ "$st" -ne 0 ] && printf '%s' "$got" | grep -q 'FIB_LIR_FACTS must be use, trap or ignore'; then ok "an unknown check mode is refused"
else no "FIB_LIR_FACTS=sometimes: status $st: $(printf '%s' "$got" | head -c 200)"; fi

# 3. the other backends drop the facts
want=$(sed -n 's/^;; out: //p' cases/lir/facts/use.lir)
if timeout 900 "$F" build compiler/lir2c.fib -I compiler -I lib -o "$T/lir2c" > "$T/lir2c.log" 2>&1; then
  if "$T/lir2c" cases/lir/facts/use.lir -o "$T/use.c" > "$T/c.log" 2>&1 && cc -O1 -w "$T/use.c" -o "$T/use-c" -lm 2>> "$T/c.log"; then
    got=$("$T/use-c"); [ "$got" = "$want" ] && ok "lir2c: use.lir translated and prints $got" || no "lir2c: use.lir printed $got, wanted $want"
    grep -q 'assume\|range' "$T/use.c" && no "lir2c: a fact reached the C" || ok "lir2c: no fact in the C"
  else no "lir2c: use.lir: $(head -c 300 "$T/c.log")"; fi
else tail -5 "$T/lir2c.log"; no "lir2c did not build"; fi
if command -v "${NODE:-node}" > /dev/null 2>&1; then
  if timeout 900 "$F" build compiler/lir2js.fib -I compiler -I lib -o "$T/lir2js" > "$T/lir2js.log" 2>&1; then
    if "$T/lir2js" cases/lir/facts/use.lir -o "$T/use.mjs" --rt compiler/js/rt > "$T/js.log" 2>&1; then
      got=$(timeout 60 "${NODE:-node}" "$T/use.mjs" 2>&1); [ "$got" = "$want" ] && ok "lir2js: use.lir translated and prints $got" || no "lir2js: use.lir printed $got, wanted $want"
    else no "lir2js: use.lir: $(head -c 300 "$T/js.log")"; fi
  else tail -5 "$T/lir2js.log"; no "lir2js did not build"; fi
else echo "skip lir2js: no node"; fi
[ $bad -eq 0 ] && echo "facts: all hold" || echo "facts: FAILED"
exit $bad
