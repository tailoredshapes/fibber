#!/usr/bin/env bash
# Records the golden corpus of the fibgen port (docs/design/fibgen-port.md, section 3)
# from the RUST generator at tag seed-1. Run once; the files it writes are committed, and the
# fibber port is checked against them by compare.sh.
# Build the Rust generator outside the tree: extract the seed-1 tag into SCRATCH/seed1, then
# CARGO_TARGET_DIR=SCRATCH/target nice -n 10 cargo build --release -j 4 -p fibgen.
#
# usage: record.sh FIBGEN OUTDIR START END      records seeds START..END of both kinds, appending
#        record.sh --gzip OUTDIR                after the last batch: compresses the full-program files
# Run it in batches of a few hundred seeds (project rule: never thousands in one run).
# Every seed runs alone, under `ulimit -v 6000000` and `timeout 60`: a seed that dies is
# written to blowups.tsv (seed, size, kind, exit code) and is NOT in the corpus. Seed 1409 of
# kind programs is one (the model asks for a 2.3 GB allocation; section 3.4 of the design).
# Program i of `fibgen run --seed 1` has seed 1+i and size 1+(i mod 6): seed s has size
# 1+((s-1) mod 6).
set -u
if [ "$1" = --gzip ]; then
  for f in "$2"/programs.txt "$2"/pipelines.txt; do gzip -9nf "$f"; done
  exit 0
fi
fg=$1; out=$2; a=$3; b=$4; fp=300; fl=150
mkdir -p "$out"
for kind in programs pipelines; do
  m="$out/manifest-$kind.tsv"
  [ -f "$m" ] || printf '# seed\tsize\tbytes\tsha256-16\tmodel (fibgen show, kind %s)\n' "$kind" > "$m"
  lim=$fp; [ "$kind" = pipelines ] && lim=$fl
  for s in $(seq "$a" "$b"); do
    size=$((1 + (s - 1) % 6))
    ( ulimit -v 6000000; exec timeout 60 "$fg" show --seed "$s" --size "$size" --kind "$kind" ) \
      > "$out/.one" 2>/dev/null
    rc=$?
    if [ $rc -ne 0 ]; then
      printf '%s\t%s\t%s\t%s\n' "$s" "$size" "$kind" "$rc" >> "$out/blowups.tsv"
      continue
    fi
    model=$(tail -n 1 "$out/.one" | sed 's/^;; model: //')
    sed '$d' "$out/.one" > "$out/.prog"
    h=$(sha256sum < "$out/.prog" | cut -c1-16)
    printf '%s\t%s\t%s\t%s\t%s\n' "$s" "$size" "$(wc -c < "$out/.prog")" "$h" "$model" >> "$m"
    if [ "$s" -le "$lim" ]; then
      printf ';;;; seed %s size %s kind %s\n' "$s" "$size" "$kind" >> "$out/$kind.txt"
      cat "$out/.prog" >> "$out/$kind.txt"
    fi
  done
done
rm -f "$out/.one" "$out/.prog"
