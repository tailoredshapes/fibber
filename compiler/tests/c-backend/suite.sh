#!/bin/bash
# The case suites through the C route (docs/design/lir2c.md section 4): `FIB_VIA=c FIB_CC=CC F cases DIR` judges every case of each directory
# by its header against a program built through the C compiler (driver.viac), the audit traces included, and the set of cases that do
# not pass must equal the lines of the expected file.
#   compiler/tests/c-backend/suite.sh --fibc F [--cc CC] [-j N] [--expected FILE] [DIR..]
#   DIR         default: cases/ownership cases/modules cases/stdlib
#   --expected  default: compiler/tests/c-backend/expected-<cc>.txt; one `DIR/CASE STATUS REASON` per line, as scripts/ci-stage2.expected
#               (the first two words are compared; an OPEN case is excused by its own header and not listed)
# Prints each directory's count line, the non-passing rows, and the diff against the expected set. Exit 0 when the sets match, 1 otherwise.
# Environment: TMPDIR (the C and the executables go there), FIB_ALLOW_PLAIN_TAIL_CALLS, LIMIT (seconds per directory, default 3600).
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
fibc=""; cc=${FIB_CC:-gcc}; jobs=4; expected=""; dirs=()
while [ $# -gt 0 ]; do
  case $1 in
    --fibc) fibc=$2; shift ;;
    --cc) cc=$2; shift ;;
    -j) jobs=$2; shift ;;
    --expected) expected=$2; shift ;;
    -h|--help) sed -n 2,11p "$0"; exit 0 ;;
    *) dirs+=("$1") ;;
  esac
  shift
done
: "${fibc:?--fibc F}"
[ ${#dirs[@]} -eq 0 ] && dirs=(cases/ownership cases/modules cases/stdlib)
[ -z "$expected" ] && expected=$here/expected-$(basename "$cc").txt
out=$(mktemp -d "${TMPDIR:-/tmp}/csuite.XXXXXX")
export FIB_VIA=c FIB_CC=$cc FIB_LIB=$root/lib
cd "$root" || exit 2
bad=0; : > "$out/actual"
for d in "${dirs[@]}"; do
  name=$(basename "$d"); start=$(date +%s)
  timeout "${LIMIT:-3600}" "$fibc" cases "$d" -j "$jobs" > "$out/$name.txt" 2> "$out/$name.err"; code=$?
  echo "== $fibc cases $d via $cc: exit $code after $(( $(date +%s) - start ))s"
  grep -E '^[0-9]+ cases: ' "$out/$name.txt"
  awk '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN"' "$out/$name.txt" | cut -c1-400
  [ "$code" -eq 124 ] && { echo "suite: $d timed out"; bad=1; }
  awk -v d="$d" '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN" { print d "/" $1 " " $2 }' "$out/$name.txt" >> "$out/actual"
done
sort -o "$out/actual" "$out/actual"
if [ -f "$expected" ]; then grep -v -e '^#' -e '^[[:space:]]*$' "$expected" | awk '{print $1, $2}' | sort > "$out/expected"; else : > "$out/expected"; fi
if ! diff -u "$out/expected" "$out/actual"; then echo "suite: the cases that do not pass differ from $expected (- expected only, + actual only)"; bad=1
else echo "suite: the non-passing set is exactly $expected"; fi
rm -rf "$out"
exit $bad
