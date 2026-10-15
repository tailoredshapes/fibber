#!/bin/bash
# compiler/tests/live/quotes.sh [FIBC]: a swap of a function whose program holds statics numbered per program (a quoted form `@q.N` of the
# `expect` macro, a closure constant, a vtable) leaves every later input of the live session right (serve.livesplit `split`, serve.livesess,
# emit.statics `program-static?`; docs/design/live-meshql.md 10):
#   1. unit-livetable.fib: the names (a nested literal carries its generation and family; a quoted form is a program's own static), built
#      with FIBC without LLVM
#   2. quotes.py over a real socket against test-app.fib, one app per scenario (a spec function defined and swapped at the REPL, then the
#      base's scenarios run; `specs` itself swapped between three bindings and one; the `test` op after the swap), about a minute
# Exit 0 when every check holds. The mutant of scripts/mutant-live.sh (quote-shared) must make it fail: before the fix the base scenario's
# run after the swap died with `fatal signal 11 at 0xe`.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-}}
[ -x "$fibc" ] || { echo "usage: quotes.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_LIVE_WITH
out=${TMPDIR:-/tmp}/live-quotes-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
if (ulimit -v 16000000; "$fibc" build "$here/unit-livetable.fib" -I compiler -I lib -o "$out/unit") > "$out/unit.build" 2>&1 && "$out/unit" > "$out/unit.log" 2>&1; then
  echo "ok unit-livetable: $(tail -n 1 "$out/unit.log")"
else echo "FAIL unit-livetable"; tail -5 "$out/unit.build" "$out/unit.log" 2>/dev/null; bad=1; fi
python3 "$here/quotes.py" "$fibc" "$root" || bad=1
exit "$bad"
