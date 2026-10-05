#!/bin/bash
# The full-text half of the determinism check (docs/design/fibgen-port.md section 3.2): the program text the port prints for seeds 1..N of KIND, with the
# `;;;; seed S size K kind KIND` line before each, equals compiler/tests/golden/gen/KIND.txt.gz (150 pipelines, 300 programs recorded). Prints `ok N programs`
# or the first lines of the diff. compare.sh checks hashes; this shows WHERE a text differs.
# usage: texts-check.sh GEN [KIND=pipelines] [N=150]       each seed runs alone under `ulimit -v 6000000` and `timeout 60`.
set -u
gen=${1:?usage: texts-check.sh GEN [KIND] [N]}
kind=${2:-pipelines}; n=${3:-150}
here=$(cd "$(dirname "$0")" && pwd)
gz=$here/../golden/gen/$kind.txt.gz
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
: > "$tmp/got"
for s in $(seq 1 "$n"); do
  size=$((1 + (s - 1) % 6))
  ( ulimit -v 6000000; exec timeout 60 "$gen" show --seed "$s" --size "$size" --kind "$kind" ) > "$tmp/one" 2>/dev/null || { echo "seed $s: failed"; exit 1; }
  printf ';;;; seed %s size %s kind %s\n' "$s" "$size" "$kind" >> "$tmp/got"
  sed '$d' "$tmp/one" >> "$tmp/got"
done
# the recorded file may hold more programs than N: compare the first N
zcat "$gz" | awk -v n="$n" '/^;;;; seed /{c++} c<=n' > "$tmp/want"
if diff "$tmp/got" "$tmp/want" > "$tmp/diff"; then echo "ok $n programs"; exit 0; fi
head -20 "$tmp/diff"; echo "texts-check: FAILED"; exit 1
