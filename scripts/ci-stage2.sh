#!/bin/bash
# The stage 2 truth about main, for CI: runs stage 2's `cases` over cases/ownership, cases/modules and cases/stdlib and fails unless
# the set of cases that do not pass equals the set in scripts/ci-stage2.expected (the known gaps, compiler/mirror-pending/H1-gaps.md).
# A new failure fails; so does an expected failure that now passes (remove its line then). The expected file has one line per case,
# `DIR/CASE STATUS`, status as the table prints it (FAIL, PENDING, HEADER); an OPEN case is one whose own header says it fails
# today (a known gap kept honest in the case itself: if it starts to pass the harness reports FAIL), so it is not listed; lines starting with # and blank lines are ignored.
# usage: scripts/ci-stage2.sh F      F is a stage 2 fibc; FIB_LIB must name lib/ (no liblair.so: F links LLVM; with `-l LLVM-21` shared, LLVM's directory is F's rpath)
# Limits (seconds), each the time a directory may take: LIMIT_OWNERSHIP, LIMIT_MODULES, LIMIT_STDLIB.
# Also (scripts/gate.sh sets them; CI does not): CI_STAGE2_JOBS=N runs N cases at a time (`cases -j N`); CI_STAGE2_ONLY=FILE holds the
# names (one per line, as `cases --only` takes them) of the stdlib cases to run, and the expected set is then the lines of the expected
# file for the cases that were run.
set -u
root=${GATE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
f=${1:?usage: ci-stage2.sh F}
out=${CI_STAGE2_OUT:-$(mktemp -d)}
mkdir -p "$out"
cd "$root"
bad=0
: > "$out/actual"
for pair in "ownership:${LIMIT_OWNERSHIP:-400}" "modules:${LIMIT_MODULES:-120}" "stdlib:${LIMIT_STDLIB:-1800}"; do
  name=${pair%%:*}; limit=${pair##*:}; dir=cases/$name
  echo "== $f cases $dir (limit ${limit}s)"
  start=$(date +%s)
  only=(); jflag=()
  [ -n "${CI_STAGE2_JOBS:-}" ] && jflag=(-j "$CI_STAGE2_JOBS")
  if [ "$name" = stdlib ] && [ -n "${CI_STAGE2_ONLY:-}" ]; then only=(--only $(cat "$CI_STAGE2_ONLY")); fi
  timeout "$limit" "$f" cases "$dir" "${only[@]}" "${jflag[@]}" > "$out/$name.txt" 2> "$out/$name.err"
  code=$?
  echo "exit $code after $(( $(date +%s) - start ))s"
  tail -n 6 "$out/$name.txt"
  awk '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN"' "$out/$name.txt" | cut -c1-1500
  if [ "$code" -eq 124 ]; then echo "ci-stage2: $dir timed out"; bad=1; continue; fi
  total=$(sed -n 's/^\([0-9][0-9]*\) cases: .*/\1/p' "$out/$name.txt" | tail -n 1)
  if [ -z "$total" ] || [ "$total" -eq 0 ]; then echo "ci-stage2: $dir printed no case count (exit $code)"; tail -n 20 "$out/$name.err"; bad=1; continue; fi
  awk -v d="$dir" '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN" { print d "/" $1 " " $2 }' "$out/$name.txt" >> "$out/actual"
done
sort -o "$out/actual" "$out/actual"
grep -v -e '^#' -e '^[[:space:]]*$' scripts/ci-stage2.expected | sort > "$out/expected"
if [ -n "${CI_STAGE2_ONLY:-}" ]; then   # a sample: only the expected lines of the stdlib cases that ran, all of the other directories'
  awk '$1 ~ /\.fib$/ { print "cases/stdlib/" $1 }' "$out/stdlib.txt" | sort > "$out/ran"
  awk 'NR==FNR { ran[$1]=1; next } $1 !~ /^cases\/stdlib\// || ($1 in ran)' "$out/ran" "$out/expected" > "$out/expected.sel"
  mv "$out/expected.sel" "$out/expected"
fi
if ! diff -u "$out/expected" "$out/actual"; then
  echo "ci-stage2: the cases that do not pass differ from scripts/ci-stage2.expected (- expected only, + actual only)"
  bad=1
fi
if [ "$bad" -ne 0 ]; then echo "ci-stage2: FAILED"; exit 1; fi
echo "ci-stage2: ok ($(wc -l < "$out/actual") expected non-passing)"
