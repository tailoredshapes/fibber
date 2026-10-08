#!/bin/bash
# The unit programs of the compiler's passes (compiler/tests/*/unit-*.fib), run so that none rots unseen: every one is built with FIBC and
# run, and must exit 0, print no line that starts with FAIL and print at least one `ok` line (a program that checks nothing is no test).
#   compiler/tests/units.sh [OUTDIR] [FILE..]     FIBC names the stage 2 (required); FIB_LIB the library (default lib/); LLVM_LIBDIR (default /usr/lib/llvm-21/lib)
#   UNITS_DIR=emit|own|types|rest|pending runs only that part (emit, own, types: the programs of that directory; rest: every other directory;
#   pending: only the check that no pending program passes), so that the gate can run the parts side by side; unset, all of them
# The programs it runs are the lines of compiler/tests/units.run. Every unit-*.fib under compiler/tests/ must be named in one of three lists, or this
# script fails: units.run (run here), units.pending (known to fail today, with the reason; a pending program that passes fails the script, so that
# it moves to units.run) and units.elsewhere (run by another script, which the line names). A new unit program therefore has to be put somewhere.
# Output: `ok FILE N s` or `FAIL FILE N s (reason)` for each program, then a count; exit 0 when all hold.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=${1:-${TMPDIR:-/tmp}/units-$$}; [ $# -gt 0 ] && shift
fibc=${FIBC:?units.sh: set FIBC to a stage 2}
export FIB_LIB=${FIB_LIB:-$root/lib}
llvm=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
mkdir -p "$out"
cd "$root" || exit 2
names() { grep -v '^#' "$here/$1" | grep -v '^$' | sed 's/[[:space:]].*//'; }
bad=0
# every unit program is accounted for
listed=$( { names units.run; names units.pending; names units.elsewhere; } | LC_ALL=C sort)
present=$(ls compiler/tests/*/unit-*.fib | LC_ALL=C sort)
for f in $(comm -13 <(echo "$listed") <(echo "$present")); do echo "FAIL $f is in none of units.run, units.pending, units.elsewhere"; bad=1; done
for f in $(comm -23 <(echo "$listed") <(echo "$present")); do echo "FAIL $f is listed but does not exist"; bad=1; done
# build and run one program: prints its verdict line, returns 0 when it passes
judge() {
  local f=$1 n t0 o rc why
  # macOS: a unit program that tests an x86-64 host is skipped, with its reason (compiler/tests/units.darwin-skip: FILE then the reason)
  if [ "$(uname -s)" = Darwin ] && why=$(grep "^$f[[:space:]]" "$here/units.darwin-skip" 2> /dev/null); then echo "ok $f 0 s (skipped on macOS: ${why#*[[:space:]]})"; return 0; fi
  n=$(echo "${f#compiler/tests/}" | tr / _); o=$out/$n; t0=$(date +%s)
  if ! "$fibc" build "$f" -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$o" > "$o.log" 2>&1; then
    echo "FAIL $f $(( $(date +%s) - t0 )) s (does not build: $(tail -n 1 "$o.log" | cut -c1-160))"; return 1; fi
  "$here/../../scripts/lib/slots.sh" timeout 300 "$o" > "$o.out" 2>&1; rc=$?
  if [ $rc -ne 0 ]; then echo "FAIL $f $(( $(date +%s) - t0 )) s (exit $rc, $(grep -c '^FAIL' "$o.out") FAIL lines: $(grep -m1 '^FAIL' "$o.out" | cut -c1-120))"; return 1; fi
  if grep -q '^FAIL' "$o.out"; then echo "FAIL $f $(( $(date +%s) - t0 )) s ($(grep -m1 '^FAIL' "$o.out" | cut -c1-140))"; return 1; fi
  grep -q '^ok' "$o.out" || { echo "FAIL $f $(( $(date +%s) - t0 )) s (it printed no ok line)"; return 1; }
  echo "ok $f $(( $(date +%s) - t0 )) s"
}
part=${UNITS_DIR:-all}
files=("$@")
if [ ${#files[@]} -eq 0 ]; then
  case $part in
    all) files=(); while IFS= read -r l; do files+=("$l"); done < <(names units.run) ;;
    pending) files=() ;;
    rest) files=(); while IFS= read -r l; do files+=("$l"); done < <(names units.run | grep -v -e '^compiler/tests/emit/' -e '^compiler/tests/own/' -e '^compiler/tests/types/') ;;
    *) files=(); while IFS= read -r l; do files+=("$l"); done < <(names units.run | grep "^compiler/tests/$part/") ;;
  esac
fi
# The programs are independent: up to UNITS_JOBS are built and run at a time (default 1, or 16 under the gate, whose slots limit the heavy
# processes: scripts/lib/slots.sh), each verdict line going to a file and printed in the order of the list, so the output does not depend on
# which finished first. A pending program that passes is a stale entry.
jobs=${UNITS_JOBS:-$([ -n "${GATE_SLOTS:-}" ] && echo 16 || echo 1)}
pend=()
if [ $# -eq 0 ] && { [ "$part" = all ] || [ "$part" = pending ]; }; then while IFS= read -r l; do pend+=("$l"); done < <(names units.pending); fi
n=0; i=0
for f in ${files[@]+"${files[@]}"}; do
  n=$((n+1)); i=$((i+1))
  ( judge "$f" > "$out/line.$i" 2>&1; echo $? > "$out/line.$i.rc" ) &
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do sleep 0.1; done
done
for f in ${pend[@]+"${pend[@]}"}; do
  i=$((i+1))
  ( if judge "$f" > "$out/pending.line.$i" 2>&1; then echo "FAIL $f passes now: move it from units.pending to units.run" > "$out/line.$i"; echo 1 > "$out/line.$i.rc"
    else : > "$out/line.$i"; echo 0 > "$out/line.$i.rc"; fi ) &
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do sleep 0.1; done
done
wait
for k in $(seq 1 "$i"); do cat "$out/line.$k"; [ "$(cat "$out/line.$k.rc")" = 0 ] || bad=1; done
echo "$n unit programs run"
exit $bad
