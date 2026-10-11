#!/bin/bash
# Builds and runs the unit programs of the language server's modules (lsp.json, lsp.text, lsp.analysis, lsp.scope) and checks each against the
# number of checks it must make. A program that fails a check, makes fewer checks than expected or does not run fails the script.
# usage: run.sh [NAME..]   (names without .fib: json text scope analysis); FIBC names the fibc/F to build with (default `fibc` on the PATH);
#        FIB_LIB the library (default the tree's lib/); LSP_UNIT_OUT holds native executables and build logs (default a private directory under $TMPDIR).
# Builds at -O2, as the shipped server does. Large analysis units exhaust the hosted Mac timeout in LLVM's Mach-O JIT linker.
# Exit: 0 all ok; 1 a failure; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "lsp tests: no fibc: set FIBC" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
outdir=${LSP_UNIT_OUT:-}
if [ -z "$outdir" ]; then
  outdir=$(mktemp -d "${TMPDIR:-/tmp}/lsp-unit.XXXXXX") || exit 2
fi
mkdir -p "$outdir" || exit 2
outdir=$(cd "$outdir" && pwd) || exit 2
cd "$root" || exit 2
want_of() { case $1 in json) echo 69 ;; text) echo 61 ;; scope) echo 62 ;; analysis) echo 46 ;; esac; }
names=("$@"); [ ${#names[@]} -gt 0 ] || names=(json text scope analysis)
rc=0
for n in "${names[@]}"; do
  f=compiler/tests/lsp/unit-$n.fib
  [ -f "$f" ] || { echo "$n: no $f" >&2; rc=1; continue; }
  if ! "$fibc" build "$f" -O2 -I compiler -I lib -o "$outdir/$n" > "$outdir/$n.build.log" 2>&1; then
    echo "$n: FAILED (build; log: $outdir/$n.build.log)" >&2
    tail -5 "$outdir/$n.build.log" >&2; rc=1; continue
  fi
  out=$("$outdir/$n" 2>&1); st=$?
  last=$(printf '%s\n' "$out" | grep '^checks ' | tail -1)
  printf '%s\n' "$out" | grep '^FAIL' >&2
  if [ "$st" -eq 0 ] && [ "$last" = "checks $(want_of "$n") failed 0" ]; then echo "$n: ok ($last)"
  else echo "$n: FAILED (exit $st; expected 'checks $(want_of "$n") failed 0', got '$last')" >&2; printf '%s\n' "$out" | tail -5 >&2; rc=1; fi
done
exit $rc
