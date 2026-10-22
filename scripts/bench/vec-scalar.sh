#!/bin/bash
# The Vec-of-scalars micro-benchmark (scripts/bench/vec-scalar.fib, docs/design/vec-performance.md): builds it with FIBC (or BENCH_FIBC, or
# `fibc` on PATH), runs it RUNS times (default 3, -n), prints per row the minimum picoseconds per element over the runs and the checksum, and
# fails (exit 1) when a persistent row and its array twin print different checksums, or two runs print different checksums for a row.
#   scripts/bench/vec-scalar.sh [-n RUNS] [SECTION]      SECTION is nth|assoc|conj|mapv|pipe (default all)
# Rows are pairs: nth/T with array-get/T, assoc/T with array-set!/T, conj/T with array-push!/T, mapv/i64 with array-loop/i64; the pipe rows
# pipe/reduce-mapv-range, pipe/reduce-map-range, pipe/reduce-hand-loop must agree, and pipe/some-first-filter-range with pipe/some-hand-loop.
# BENCH_BUILD_FLAGS is passed to `fibc build` (for example --whole-program).
# Machine load changes the times, not the checksums: run it on a quiet machine, or with taskset, and read the minimum.
set -u
here=$(cd "$(dirname "$0")" && pwd)
runs=3
if [ "${1:-}" = "-n" ]; then runs=${2:?-n needs a number}; shift 2; fi
sect=${1:-}
fibc=${BENCH_FIBC:-${FIBC:-$(command -v fibc || true)}}
[ -x "$fibc" ] || { echo "vec-scalar.sh: set FIBC (or BENCH_FIBC) to a fibc" >&2; exit 2; }
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
(ulimit -v 16000000; "$fibc" build ${BENCH_BUILD_FLAGS:-} "$here/vec-scalar.fib" -o "$work/vs") > "$work/build.log" 2>&1 || { cat "$work/build.log"; exit 1; }
: > "$work/rows"
for i in $(seq "$runs"); do (ulimit -v 16000000; timeout 600 "$work/vs" $sect) >> "$work/rows" || { echo "vec-scalar.sh: run failed"; exit 1; }; done
awk '
  { n=$1; t=$2; c=$3
    if (!(n in best) || t < best[n]) best[n]=t
    if ((n in chk) && chk[n] != c) { print "DIFFERENT CHECKSUMS between runs: " n; bad=1 }
    chk[n]=c; if (!(n in ord)) { ord[n]=++k; name[k]=n } }
  END {
    split("nth/i8:array-get/i8 nth/i64:array-get/i64 nth/f64:array-get/f64 assoc/i64:array-set!/i64 assoc/i8:array-set!/i8 assoc/f64:array-set!/f64 conj/i64:array-push!/i64 conj/i8:array-push!/i8 mapv/i64:array-loop/i64 pipe/reduce-mapv-range:pipe/reduce-hand-loop pipe/reduce-map-range:pipe/reduce-hand-loop pipe/some-first-filter-range:pipe/some-hand-loop", pairs, " ")
    for (p in pairs) { split(pairs[p], ab, ":"); if ((ab[1] in chk) && (ab[2] in chk) && chk[ab[1]] != chk[ab[2]]) { print "WRONG ANSWER: " ab[1] " " chk[ab[1]] " vs " ab[2] " " chk[ab[2]]; bad=1 } }
    printf "%-32s %12s  %s\n", "row", "ps/element", "checksum"
    for (i=1;i<=k;i++) printf "%-32s %12d  %s\n", name[i], best[name[i]], chk[name[i]]
    exit bad }' "$work/rows"
