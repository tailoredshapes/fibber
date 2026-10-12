#!/bin/bash
# Compiler unit modules export suites, compiled together by emit/own/types/rest. The
# generated executables support fib.test's --only, --list, JSON/TAP and isolation flags.
# FIBC names stage 2. Usage: units.sh [OUTDIR] [FILE..]; UNITS_DIR selects a group.
# UNITS_DIR=run is every unit of units.run (no pending ones); UNITS_MERGE=1 builds the selected units as ONE executable (group "all");
# UNITS_OPT=N builds with -O N (the gate uses 1: the same results, a cold LLVM cache costs half as much as at -O2, docs/design/test-pyramid.md).
# Every unit-*.fib must be in units.run, units.pending or units.elsewhere.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=${1:-${TMPDIR:-/tmp}/units-$$}; [ $# -gt 0 ] && shift
fibc=${FIBC:?units.sh: set FIBC to a stage 2}
export FIB_LIB=${FIB_LIB:-$root/lib}
llvm=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
mkdir -p "$out"; out=$(cd "$out" && pwd)
cd "$root" || exit 2
names() { grep -v '^#' "$here/$1" | grep -v '^$' | sed 's/[[:space:]].*//'; }
part=${UNITS_DIR:-all}; files=("$@")
if [ ${#files[@]} -eq 0 ]; then
  case $part in
    all|run) while IFS= read -r f; do files+=("$f"); done < <(names units.run) ;;
    pending) ;;
    rest) while IFS= read -r f; do files+=("$f"); done < <(names units.run | grep -v -e '^compiler/tests/emit/' -e '^compiler/tests/own/' -e '^compiler/tests/types/') ;;
    emit|own|types) while IFS= read -r f; do files+=("$f"); done < <(names units.run | grep "^compiler/tests/$part/") ;;
    *) echo "units: unknown UNITS_DIR=$part" >&2; exit 2 ;;
  esac
fi
# Host-specific units retain their documented exclusions; they are never reported as passes.
selected=()
for f in ${files[@]+"${files[@]}"}; do
  if [ "$(uname -s)" = Darwin ] && why=$(grep "^$f[[:space:]]" "$here/units.darwin-skip"); then
    echo "SKIPPED $f: ${why#*[[:space:]]}"
  else selected+=("$f"); fi
done
python3 "$here/unit-groups.py" "$out" ${selected[@]+"${selected[@]}"} > "$out/groups" || exit 1
bad=0
run_group() {
  local group=$1 t0 rc
  t0=$(date +%s)
  if ! "$fibc" build "$out/$group.fib" -I compiler -I lib -L "$llvm" -l LLVM-21 ${UNITS_OPT:+-O "$UNITS_OPT"} -o "$out/$group" > "$out/$group.build.log" 2>&1; then
    echo "FAIL units/$group build (log: $out/$group.build.log)"; tail -n 12 "$out/$group.build.log"; return 1
  fi
  echo "units/$group build $(( $(date +%s) - t0 )) s"
  t0=$(date +%s)
  "$root/scripts/lib/slots.sh" timeout 600 "$out/$group" --format json > "$out/$group.json" 2> "$out/$group.err"; rc=$?
  echo "units/$group run $(( $(date +%s) - t0 )) s"
  python3 "$here/unit-report.py" "$out/$group.json" "$out/$group.files" || return 1
  [ "$rc" -eq 0 ] || { echo "FAIL units/$group exit $rc"; cat "$out/$group.err"; return 1; }
}
# Make schedules groups separately in the gate. A standalone all-groups run is serial by default.
jobs=${UNITS_JOBS:-1}
case $jobs in ''|*[!0-9]*|0) echo "units: UNITS_JOBS must be positive" >&2; exit 2 ;; esac
while IFS= read -r group; do
  (run_group "$group" > "$out/$group.log" 2>&1; echo $? > "$out/$group.rc") &
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do sleep 0.1; done
done < "$out/groups"
wait
while IFS= read -r group; do
  cat "$out/$group.log"; [ "$(cat "$out/$group.rc")" = 0 ] || bad=1
done < "$out/groups"

# Pending units keep their independent legacy checks. A passing one is a stale entry. The builds are independent, so they run
# UNITS_PENDING_JOBS at a time (default 5; the report keeps the order of units.pending).
check_pending() {
  local f=$1 n exe
  n=$(echo "${f#compiler/tests/}" | tr / _); exe=$out/$n
  if "$fibc" build "$f" -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$exe" > "$exe.log" 2>&1 \
     && timeout 300 "$exe" > "$exe.out" 2>&1; then   # the runner of fib.test exits 0 only when every scenario held
    echo "FAIL $f passes now: move it from units.pending to units.run"; return 1
  fi
  echo "PENDING $f"
}
if [ $# -eq 0 ] && { [ "$part" = all ] || [ "$part" = pending ]; }; then
  pjobs=${UNITS_PENDING_JOBS:-5}; k=0
  while IFS= read -r f; do
    (check_pending "$f" > "$out/pending.$k.log" 2>&1; echo $? > "$out/pending.$k.rc") &
    k=$((k + 1))
    while [ "$(jobs -rp | wc -l)" -ge "$pjobs" ]; do sleep 0.1; done
  done < <(names units.pending)
  wait
  for ((i = 0; i < k; i++)); do cat "$out/pending.$i.log"; [ "$(cat "$out/pending.$i.rc")" = 0 ] || bad=1; done
fi
echo "${#selected[@]} unit modules run"
exit "$bad"
