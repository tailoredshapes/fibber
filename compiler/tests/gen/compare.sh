#!/bin/bash
# The determinism check of the fibgen port (docs/design/fibgen-port.md section 3): for each row of compiler/tests/golden/gen/manifest-KIND.tsv, the
# program the port prints for (seed, size, kind) has the recorded byte count and sha256-16, and its `;; model:` line is the recorded model column.
# A seed in blowups.tsv is not in a manifest and is not checked (it is the size contract of section 3.4).
# usage: compare.sh GEN [KIND=programs|pipelines] [FROM=1] [TO=300]
#   GEN is the port's `show`: a program run as `GEN show --seed S --size K --kind KIND` (the tool built from compiler/fibgen.fib, or a script).
# Each seed runs alone under `ulimit -v 6000000` and `timeout 60`. Batches of a few hundred seeds: never thousands in one run.
# Exit 0 and `ok N rows`; 1 on the first 10 differences (printed), 2 on usage.
set -u
gen=${1:?usage: compare.sh GEN [KIND] [FROM] [TO]}
kind=${2:-programs}; from=${3:-1}; to=${4:-300}
here=$(cd "$(dirname "$0")" && pwd)
man=$here/../golden/gen/manifest-$kind.tsv
[ -f "$man" ] || { echo "compare: no manifest for $kind" >&2; exit 2; }
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
bad=0; n=0
while IFS=$'\t' read -r seed size bytes hash model; do
  case $seed in '#'*) continue ;; esac
  [ "$seed" -ge "$from" ] && [ "$seed" -le "$to" ] || continue
  n=$((n + 1))
  ( ulimit -v 6000000; exec timeout 60 "$gen" show --seed "$seed" --size "$size" --kind "$kind" ) > "$tmp/one" 2> "$tmp/err"
  rc=$?
  if [ $rc -ne 0 ]; then echo "seed $seed: exit $rc"; bad=$((bad + 1)); [ $bad -ge 10 ] && break; continue; fi
  got_model=$(tail -n 1 "$tmp/one" | sed 's/^;; model: //')
  sed '$d' "$tmp/one" > "$tmp/prog"
  got_hash=$(sha256sum < "$tmp/prog" | cut -c1-16); got_bytes=$(wc -c < "$tmp/prog" | tr -d " ")
  if [ "$got_hash" != "$hash" ] || [ "$got_bytes" != "$bytes" ] || [ "$got_model" != "$model" ]; then
    echo "seed $seed size $size: want $bytes $hash $model; got $got_bytes $got_hash $got_model"
    bad=$((bad + 1)); [ $bad -ge 10 ] && break
  fi
done < "$man"
[ $bad -eq 0 ] && { echo "ok $n rows"; exit 0; }
exit 1
