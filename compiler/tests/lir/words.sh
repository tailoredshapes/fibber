#!/bin/bash
# compiler/tests/lir/words.sh: the table of lIR's words (compiler/lir/words.fib, printed by `lairf words`) equals the upper-case atoms of
# spec/lir.md's grammar (spec/lir.md 2, "Spelling"): the code blocks that hold `::=`, comments left out, an atom's `?`, `*`, `+` and `..`
# suffixes and a leading `-` dropped, less the placeholders the spec lists after "The placeholders:". A word the spec's grammar does not
# name, or an upper-case atom of the grammar that is neither a word nor a placeholder, fails it; so the spec, the parser and the printer
# cannot drift apart.
# usage: compiler/tests/lir/words.sh [FIBC]   (LAIRF names a built lairf instead; OUT the scratch, default $TMPDIR/lir-words)
# Prints the counts and every difference; exit 0 when the two sets are equal, 1 when they differ, 2 when nothing could be built.
set -u
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
F=${1:-${FIBC:-}}; L=${LAIRF:-}; SPEC=${SPEC:-$root/spec/lir.md}
out=${OUT:-${TMPDIR:-/tmp}/lir-words}; mkdir -p "$out"
lib=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
export LD_LIBRARY_PATH=$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
if [ -z "$L" ]; then
  [ -x "$F" ] || { echo "words: give a stage 2 fibc (FIBC) or a lairf (LAIRF)" >&2; exit 2; }
  (ulimit -v 16000000; timeout 900 "$F" build compiler/lairf.fib -I compiler -I lib -L "$lib" -l LLVM-21 -o "$out/lairf") > "$out/lairf.log" 2>&1 \
    || { tail -5 "$out/lairf.log"; echo "words: lairf did not build" >&2; exit 2; }
  L=$out/lairf
fi
(ulimit -v 4000000; timeout 60 "$L" words) > "$out/table.txt" || { echo "words: lairf words failed" >&2; exit 2; }
cut -d' ' -f1 "$out/table.txt" | sort -u > "$out/table"
# The grammar's upper-case atoms and the placeholders, from the spec.
awk '
  /^```/ { if (inb) { if (g) printf "%s", buf; inb = 0 } else { inb = 1; g = 0; buf = "" } next }
  inb { line = $0; sub(/;.*/, "", line); if (line ~ /::=/) g = 1; buf = buf line "\n" }
' "$SPEC" | tr -s ' \t()[]{}|' '\n' | sed -E 's/(\.\.|[?*+])+$//; s/^-//' | grep -E '^[A-Z][A-Z0-9_]*(-[A-Z0-9_]+)*$' | sort -u > "$out/grammar"
tr '\n' ' ' < "$SPEC" | grep -o 'The placeholders: `[^`]*`' | head -1 | sed -E 's/.*`([^`]*)`/\1/' | tr -s ' ' '\n' | grep -v '^$' | sort -u > "$out/placeholders"
[ -s "$out/placeholders" ] || { echo "words: the spec lists no placeholders (\"The placeholders: \`..\`\")" >&2; exit 2; }
comm -23 "$out/grammar" "$out/placeholders" > "$out/spec"
missing=$(comm -23 "$out/table" "$out/spec"); extra=$(comm -13 "$out/table" "$out/spec")
echo "words: $(wc -l < "$out/table") in the table, $(wc -l < "$out/spec") upper-case atoms in the spec's grammar ($(wc -l < "$out/placeholders") placeholders left out)"
[ -z "$missing" ] || echo "words: in the table, not in the spec's grammar: $(echo $missing)"
[ -z "$extra" ] || echo "words: in the spec's grammar, neither a word nor a placeholder: $(echo $extra)"
[ -z "$missing" ] && [ -z "$extra" ]
