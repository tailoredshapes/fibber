#!/bin/bash
# The stage 2 truth about main, for CI: runs stage 2's `cases` over cases/ownership, cases/modules and cases/stdlib and fails unless
# the set of cases that do not pass equals the set in scripts/ci-stage2.expected (the known gaps, compiler/mirror-pending/H1-gaps.md).
# A new failure fails; so does an expected failure that now passes (remove its line then). The expected file has one line per case,
# `DIR/CASE STATUS`, status as the table prints it; lines starting with # and blank lines are ignored.
# usage: scripts/ci-stage2.sh F      F is a stage 2 fibc; FIB_LIB must name lib/ and LD_LIBRARY_PATH liblair.so's directory
# Limits (seconds), each the time a directory may take: LIMIT_OWNERSHIP, LIMIT_MODULES, LIMIT_STDLIB.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
f=${1:?usage: ci-stage2.sh F}
out=${CI_STAGE2_OUT:-$(mktemp -d)}
mkdir -p "$out"
cd "$root"
bad=0
: > "$out/actual"
for pair in "ownership:${LIMIT_OWNERSHIP:-900}" "modules:${LIMIT_MODULES:-900}" "stdlib:${LIMIT_STDLIB:-7200}"; do
  name=${pair%%:*}; limit=${pair##*:}; dir=cases/$name
  echo "== $f cases $dir (limit ${limit}s)"
  start=$(date +%s)
  timeout "$limit" "$f" cases "$dir" > "$out/$name.txt" 2> "$out/$name.err"
  code=$?
  echo "exit $code after $(( $(date +%s) - start ))s"
  tail -n 6 "$out/$name.txt"
  if [ "$code" -eq 124 ]; then echo "ci-stage2: $dir timed out"; bad=1; continue; fi
  total=$(sed -n 's/^\([0-9][0-9]*\) cases: .*/\1/p' "$out/$name.txt" | tail -n 1)
  if [ -z "$total" ] || [ "$total" -eq 0 ]; then echo "ci-stage2: $dir printed no case count (exit $code)"; tail -n 20 "$out/$name.err"; bad=1; continue; fi
  awk -v d="$dir" '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" { print d "/" $1 " " $2 }' "$out/$name.txt" >> "$out/actual"
done
sort -o "$out/actual" "$out/actual"
grep -v -e '^#' -e '^[[:space:]]*$' scripts/ci-stage2.expected | sort > "$out/expected"
if ! diff -u "$out/expected" "$out/actual"; then
  echo "ci-stage2: the cases that do not pass differ from scripts/ci-stage2.expected (- expected only, + actual only)"
  bad=1
fi
if [ "$bad" -ne 0 ]; then echo "ci-stage2: FAILED"; exit 1; fi
echo "ci-stage2: ok ($(wc -l < "$out/actual") expected non-passing)"
