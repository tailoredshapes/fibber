#!/bin/bash
# Gathers the lIR corpus the lair-in-fibber comparison scripts run over (docs/design/lair-in-fibber.md section 2.3) into one directory
# with a manifest. Nothing here is committed; the default directory is on the big disk, never /tmp.
#
#   lir/CATEGORY/NAME.lir    every cases/lir file (the rejecting and odd inputs the emitter never produces)
#   emit/compiler.lir        `fibc emit compiler/fibc.fib` (the compiler's own lIR, about 19 MB)
#   emit/own-NAME.lir        the emitted lIR of each cases/ownership/*.fib
#   emit/mod-NAME.lir        the emitted lIR of cases/modules/NAME/main.fib
#   emit/std-NAME.lir        every STDLIB_EVERY-th (default 10th) of cases/stdlib/*.fib, in name order: a deterministic sample
#   rt/NAME.lir              each rt/*.lir part
#   fuzz/seedS-mI.lir        the mutants of `lair fuzz --keep` (FUZZ_COUNT, default 200, seed FUZZ_SEED, default 1)
#   MANIFEST.tsv             one line per file: CLASS, path relative to the directory, bytes, sha256; then `# N files`
#   skipped.txt              every program the emitter refused (a rejecting case has no lIR): path and the first error line
#
# usage: scripts/lair-corpus.sh [-o DIR] [-j N]       run from the repository root
# environment: FIBC (the fibc that emits; the v0.1.4 seed: scripts/fetch-seed.sh DIR prints it), LAIR (the Rust lair binary, for the
#   fuzz class), FIB_LIB (defaults to ./lib so the seed compiles this tree's library). A missing FIBC or LAIR skips that class and
#   says so; the manifest lists only what was gathered. Exit 0 when the directory was written.
set -u
out=${LAIR_CORPUS:-$HOME/.cache/fibber-scratch/lair-corpus}; jobs=4
while [ $# -gt 0 ]; do
  case $1 in
    -o) out=$2; shift 2 ;;
    -j) jobs=$2; shift 2 ;;
    *) echo "usage: scripts/lair-corpus.sh [-o DIR] [-j N]" >&2; exit 2 ;;
  esac
done
[ -d cases/lir ] || { echo "lair-corpus: run from the repository root" >&2; exit 2; }
[ "$jobs" -le 4 ] 2>/dev/null || { echo "lair-corpus: -j is at most 4 (heavy jobs)" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$PWD/lib}
every=${STDLIB_EVERY:-10}
rm -rf "$out"; mkdir -p "$out/lir" "$out/emit" "$out/rt" "$out/fuzz"
: > "$out/skipped.txt"

# cases/lir, rt
for f in $(find cases/lir -name '*.lir' | sort); do
  rel=${f#cases/lir/}; mkdir -p "$out/lir/$(dirname "$rel")"; cp "$f" "$out/lir/$rel"
done
for f in rt/*.lir; do cp "$f" "$out/rt/$(basename "$f")"; done

# emit one program: emit_one SRC DEST [fibc args]; a refusal is recorded, not an error
emit_one() {
  src=$1; dest=$2; shift 2
  # the `;; roots:` header of a case names its -I directories, relative to the case's own directory
  roots=(); for r in $(sed -n 's/^;; roots: *//p' "$src" | head -n 1); do roots+=(-I "$(dirname "$src")/$r"); done
  if (ulimit -v 16000000; timeout 600 "$FIBC" emit "${roots[@]}" "$@" "$src" > "$dest.tmp" 2> "$dest.err"); then
    mv "$dest.tmp" "$dest"; rm -f "$dest.err"
  else
    rm -f "$dest.tmp"; echo "$src: $(head -n 2 "$dest.err" | tr '\n' ' ')" >> "$out/skipped.txt"; rm -f "$dest.err"
  fi
}

if [ -n "${FIBC:-}" ] && [ -x "$FIBC" ]; then
  wait_slot() { while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do wait -n; done; }
  wait_slot; emit_one compiler/fibc.fib "$out/emit/compiler.lir" -I compiler &
  for f in cases/ownership/*.fib; do
    wait_slot; emit_one "$f" "$out/emit/own-$(basename "$f" .fib).lir" &
  done
  for d in cases/modules/*/; do
    n=$(basename "$d"); [ -f "$d/main.fib" ] || continue
    wait_slot; emit_one "$d/main.fib" "$out/emit/mod-$n.lir" &
  done
  i=0
  for f in $(ls cases/stdlib/*.fib | sort); do
    i=$((i+1)); [ $(( (i-1) % every )) -eq 0 ] || continue
    wait_slot; emit_one "$f" "$out/emit/std-$(basename "$f" .fib).lir" &
  done
  wait
else
  echo "lair-corpus: FIBC unset or not executable: the emit class is not gathered" >&2
fi

if [ -n "${LAIR:-}" ] && [ -x "$LAIR" ]; then
  "$LAIR" fuzz --count "${FUZZ_COUNT:-200}" --seed "${FUZZ_SEED:-1}" -o "$out/fuzz-findings" --keep "$out/fuzz" cases/lir > "$out/fuzz.log" 2>&1
  tail -n 1 "$out/fuzz.log" >&2
else
  echo "lair-corpus: LAIR unset or not executable: the fuzz class is not gathered" >&2
fi

# the manifest, sorted, with a count at the end
( cd "$out" && for f in $(find lir emit rt fuzz -name '*.lir' | sort); do
    printf '%s\t%s\t%s\t%s\n' "${f%%/*}" "$f" "$(stat -c %s "$f")" "$(sha256sum "$f" | cut -d' ' -f1)"
  done > MANIFEST.tsv
  echo "# $(wc -l < MANIFEST.tsv) files" >> MANIFEST.tsv )
for c in lir emit rt fuzz; do
  echo "lair-corpus: $c: $(grep -c "^$c	" "$out/MANIFEST.tsv") files" >&2
done
echo "lair-corpus: $(wc -l < "$out/skipped.txt") programs skipped (see skipped.txt); wrote $out/MANIFEST.tsv" >&2
