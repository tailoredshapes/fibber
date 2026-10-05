#!/bin/bash
# Builds heap-gold.fib (20 object graphs written out by hand from cases/ownership, driven through fibref.heap) and compares the free trace it prints for each
# with the recorded one in compiler/tests/golden/fibref/itrace-ownership.txt, byte for byte (ordinals renumbered by the trace itself). usage: heap-gold.sh
# FIBC names the fibc (default `fibc`); FIB_LIB the library. Exit: 0 every case equal; 1 a difference or a build failure.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
export FIB_LIB=${FIB_LIB:-$root/lib}
llvmdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/heap-gold-XXXXXX"); trap 'rm -rf "$tmp"' EXIT
cd "$root" || exit 2
"$fibc" build compiler/tests/fibref/heap-gold.fib -I compiler -I lib -L "$llvmdir" -l LLVM-21 -o "$tmp/gold" 2> "$tmp/err" || { head -5 "$tmp/err"; echo "heap-gold: FAILED to build"; exit 1; }
"$tmp/gold" > "$tmp/ours"
golden=compiler/tests/golden/fibref/itrace-ownership.txt
bad=0; n=0
for name in $(grep '^=== ' "$tmp/ours" | sed 's/^=== //'); do
  awk -v n="$name" '$0=="=== "n{on=1;print;next} on&&/^(result:|audit:|--- )/{exit} on{print}' "$golden" > "$tmp/want"
  awk -v n="$name" '$0=="=== "n{on=1;print;next} on&&/^=== /{exit} on{print}' "$tmp/ours" > "$tmp/got"
  n=$((n+1))
  if cmp -s "$tmp/want" "$tmp/got"; then echo "equal    $name"; else echo "DIFFERS  $name"; diff "$tmp/want" "$tmp/got" | head -10; bad=1; fi
done
echo "heap-gold: $n cases compared, $([ $bad = 0 ] && echo all equal || echo some differ)"
exit $bad
