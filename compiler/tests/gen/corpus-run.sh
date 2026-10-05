#!/bin/bash
# Runs programs of the golden corpus (compiler/tests/golden/gen/programs.txt.gz, pipelines.txt.gz: the Rust generator's output) through a stage 2 F as
# cases whose header is the Rust model's verdict, with `F cases`, and prints its tally. This is rule 5 with the Rust generator and today's compiler: it
# needs no part of the port, so it is the first useful thing G0 gives (it tests F), and it is the oracle the port's own `emit` must reproduce.
# usage: corpus-run.sh F [KIND=programs|pipelines] [FROM=1] [TO=40] [JOBS=2]     (FIB_LIB names the library; the tree's lib/ by default)
set -u
fc=${1:?usage: corpus-run.sh F [KIND] [FROM] [TO] [JOBS]}
kind=${2:-programs}; from=${3:-1}; to=${4:-40}; jobs=${5:-2}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
g=$here/../golden/gen
dir=$(mktemp -d); trap 'rm -rf "$dir"' EXIT
zcat "$g/$kind.txt.gz" | awk -v dir="$dir" -v from="$from" -v to="$to" -v kind="$kind" '
  /^;;;; seed / { if (out) close(out); seed = $3; size = $5; out = ""; if (seed >= from && seed <= to) { out = sprintf("%s/gen-%08d-%s.fib", dir, seed, size); sel[out] = seed } ; next }
  out { print > out }'
while IFS=$'\t' read -r seed size bytes hash model; do
  case $seed in '#'*) continue ;; esac
  f=$(printf '%s/gen-%08d-%s.fib' "$dir" "$seed" "$size")
  [ -f "$f" ] || continue
  case $model in
    'Ok('*) r=${model#Ok(}; r=${r%)}; head=";; expect: accept\n;; result: $r\n;; audit:  clean" ;;
    'Err(Trap("'*) m=${model#Err(Trap(\"}; m=${m%\"))}; head=";; expect: trap\n;; trap:   $m" ;;
    *) rm -f "$f"; continue ;;
  esac
  { printf ';; spec:   method.md rule 5 (seed %s size %s kind %s, golden corpus)\n' "$seed" "$size" "$kind"; printf "$head\n\n"; cat "$f"; } > "$f.new" && mv "$f.new" "$f"
done < "$g/manifest-$kind.tsv"
"$fc" cases "$dir" -j "$jobs"
