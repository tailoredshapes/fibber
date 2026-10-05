#!/bin/bash
# Cross-compiles ownership cases (accept and trap verdicts) to Mach-O arm64 objects on this machine (F emit --target, lairf build --emit obj) and
# runs them on an Apple Silicon Mac over ssh (scripts/mac-run-objects.sh), under the Mac's ~/fibber-a64-scratch only (removed afterwards).
# usage: a64-cross-mac.sh [--host HOST] [--max N] [CASE.fib..]       F=stage-2 fibc, L=lairf, LD_LIBRARY_PATH reaching libLLVM-21
#   default cases: the first --max (default 40) accept/trap cases of cases/ownership, sorted. TRIPLE overrides arm64-apple-macosx13.0.0.
# Exit: 0 every run passes; 1 a case fails to compile or run; 2 no Mac or no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
host=192.168.7.254; max=40; triple=${TRIPLE:-arm64-apple-macosx13.0.0}
while [ $# -gt 0 ]; do case $1 in --host) host=$2; shift 2 ;; --max) max=$2; shift 2 ;; *) break ;; esac; done
: "${F:?F names the stage-2 fibc}" "${L:?L names lairf}"
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/a64x.XXXXXX"); trap 'rm -rf "$T"' EXIT
ssh -o BatchMode=yes -o ConnectTimeout=10 "$host" 'mkdir -p ~/fibber-a64-scratch' || { echo "a64-cross-mac: no ssh to $host" >&2; exit 2; }
if [ $# -gt 0 ]; then cases=("$@"); else cases=($(grep -l -E '^;; expect: (accept|trap)' cases/ownership/*.fib | LC_ALL=C sort | head -n "$max")); fi
n=0; xfail=0
for f in "${cases[@]}"; do
  name=$(basename "$f" .fib)
  if "$F" emit --target "$triple" -I compiler -I lib "$f" > "$T/$name.lir" 2> "$T/$name.err" && "$L" build "$T/$name.lir" --target "$triple" -o "$T/$name.o" -O 2 --emit obj 2>> "$T/$name.err"; then
    verdict=$(sed -n 's/^;; expect: *//p' "$f" | head -1)
    if [ "$verdict" = accept ]; then echo "result $(sed -n 's/^;; result: *//p' "$f" | head -1)" > "$T/$name.expect"
    else echo "trap $(sed -n 's/^;; trap: *//p' "$f" | head -1)" > "$T/$name.expect"; fi
    n=$((n+1))
  else echo "XFAIL $name (cross compile): $(head -c 160 "$T/$name.err" | tr '\n' ' ')"; xfail=$((xfail+1)); fi
  rm -f "$T/$name.lir"
done
echo "cross-compiled $n, failed to compile $xfail"
scp -q "$root/scripts/mac-run-objects.sh" "$host:fibber-a64-scratch/mac-run-objects.sh" || exit 2
tar -C "$T" -cf - --exclude='*.err' . | ssh -o BatchMode=yes "$host" 'rm -rf ~/fibber-a64-scratch/run && mkdir -p ~/fibber-a64-scratch/run && tar -C ~/fibber-a64-scratch/run -xf -' || exit 2
ssh -o BatchMode=yes "$host" 'bash ~/fibber-a64-scratch/mac-run-objects.sh ~/fibber-a64-scratch/run; st=$?; rm -rf ~/fibber-a64-scratch/run; exit $st'
st=$?
[ "$xfail" -eq 0 ] && exit $st
exit 1
