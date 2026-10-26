#!/bin/bash
# compiler/tests/live/defs.sh [FIBC] [--socket-only|--tsan-only] [--quick]: `def`s redefined in a live session (package LT4 of
# docs/design/live-meshql.md; docs/design/live-repl.md 9; serve.livedefs, emit.program `live-def-slot`). Three parts:
#   guard   live mode only: a plain emit of defs-app.fib has no def slot (`live.d.`), the live emit has one, and a second `def` of a
#           name is still the error it was (`x is already defined`)
#   socket  defs.py over a real socket against defs-app.fib with defs-cfg.fib (its docstring lists the checks; 1000 redefinitions under
#           8 reading threads, about two minutes; --quick is 100)
#   tsan    scripts/tsan.sh --check over defs-tsan.fib emitted in live mode with defs-swapper.lir appended (TSAN_APPEND): the readers'
#           acquire load of `live.d.limit` against the setter's release store, and of the boxed `live.d.pair` (an Option: a new box per
#           store) against its setter's; clean is no report beyond scripts/tsan-baseline.txt in TSAN_RUNS runs (2 here)
# Exit 0 when every check holds. The mutants of scripts/mutant-live.sh: def-slot-plain-store and boxed-in-place (tsan), old-def-freed,
# init-run-twice, retype-no-readers and unswappable-reader-accepted (socket) must make it fail.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-}; part=both; quick=
while [ $# -gt 0 ]; do
  case $1 in --socket-only) part=socket ;; --tsan-only) part=tsan ;; --quick) quick=1 ;; *) fibc=$1 ;; esac
  shift
done
[ -x "$fibc" ] || { echo "usage: defs.sh FIBC [--socket-only|--tsan-only] [--quick] (or FIBC in the environment)"; exit 2; }
fibc=$(cd "$(dirname "$fibc")" && pwd)/$(basename "$fibc")
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS
cd "$root" || exit 2
bad=0
tmp=$(mktemp -d "${TMPDIR:-/tmp}/live-defs-guard.XXXXXX")
mkdir -p "$tmp/src"; cp "$here/defs-cfg.fib" "$tmp/src/cfg.fib"
if "$fibc" emit -I "$tmp/src" "$here/defs-app.fib" > "$tmp/plain.lir" 2> "$tmp/plain.err" && ! grep -q 'live\.d\.' "$tmp/plain.lir" \
   && FIB_LIVE=1 "$fibc" emit -I "$tmp/src" "$here/defs-app.fib" 2> /dev/null | grep -q 'ATOMIC-LOAD ACQUIRE I64 (GLOBAL live.d.limit)'
then echo "ok guard: a plain emit has no def slot (the live emit reads live.d.limit)"
else echo "FAIL guard: a def slot in a plain emit, or none in a live one ($(head -2 "$tmp/plain.err"))"; bad=1; fi
printf '(def x: i64 1)\n(def x: i64 2)\n(defun main () -> i64 x)\n' > "$tmp/dup.fib"
if out=$("$fibc" run "$tmp/dup.fib" 2>&1); then echo "FAIL guard: a duplicate def ran: $out"; bad=1
elif echo "$out" | grep -q 'x is already defined'; then echo "ok guard: a duplicate def is still an error"
else echo "FAIL guard: a duplicate def: $out"; bad=1; fi
rm -rf "$tmp"
if [ $part != tsan ]; then
  n=1000; [ -n "$quick" ] && n=100
  python3 "$here/defs.py" "$fibc" "$root" --redefs $n || bad=1
fi
if [ $part != socket ]; then
  out=${TSAN_OUT:-$HOME/.cache/fibber-scratch/live-defs-tsan}
  if F=$fibc TSAN_OUT=$out TSAN_RUNS=${TSAN_RUNS:-2} FIB_LIVE=1 TSAN_APPEND="$here/defs-swapper.lir" "$root/scripts/tsan.sh" --check "$here/defs-tsan.fib" 300 20000 > "$out.log" 2>&1
  then echo "ok tsan: $(tail -1 "$out.log")"
  else echo "FAIL tsan: $(grep -E '^NEW|^exit|^tsan' "$out.log" | head -5 | tr '\n' ' ')"; tail -3 "$out.log"; bad=1; fi
fi
[ $bad = 0 ] && echo "live defs: ok" || echo "live defs: FAILED"
exit $bad
