#!/bin/bash
# Builds and runs the unit programs of the language server's modules (lsp.json, lsp.text, lsp.analysis, lsp.scope) and checks each against the
# number of checks it must make. A program that fails a check, makes fewer checks than expected or does not run fails the script.
# usage: run.sh [NAME..]   (names without .fib: json text scope analysis); FIBC names the fibc/F to build with (default `fibc` on the PATH);
#        FIB_LIB the library (default the tree's lib/); `run` goes through the JIT, so LAIR_DIR/LD_LIBRARY_PATH must be set as for the compiler's own tools
# Exit: 0 all ok; 1 a failure; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "lsp tests: no fibc: set FIBC" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
declare -A want=([json]=69 [text]=56 [scope]=62 [analysis]=39)
names=("$@"); [ ${#names[@]} -gt 0 ] || names=(json text scope analysis)
rc=0
for n in "${names[@]}"; do
  f=compiler/tests/lsp/unit-$n.fib
  [ -f "$f" ] || { echo "$n: no $f" >&2; rc=1; continue; }
  out=$("$fibc" run "$f" -I compiler -I lib 2>&1); st=$?
  last=$(printf '%s\n' "$out" | grep '^checks ' | tail -1)
  printf '%s\n' "$out" | grep '^FAIL' >&2
  if [ "$last" = "checks ${want[$n]} failed 0" ]; then echo "$n: ok ($last)"
  else echo "$n: FAILED (exit $st; expected 'checks ${want[$n]} failed 0', got '$last')" >&2; printf '%s\n' "$out" | tail -5 >&2; rc=1; fi
done
exit $rc
