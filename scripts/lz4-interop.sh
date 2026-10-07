#!/bin/bash
# scripts/lz4-interop.sh: examples/lz4.fib against the reference `lz4` CLI (v1.10.0, scripts/fetch-lz4-tools.sh), both ways, for a set of inputs and flag combinations:
#   ours compresses -> `lz4 -d` decompresses, and the reference compresses (`lz4` with the same flags) -> ours decompresses; every result is compared with the input (cmp).
# usage: scripts/lz4-interop.sh [LZ4F] [FILE..]    (default: examples/lz4.fib built with FIBC, and a few generated files and Silesia's dickens, xml and sao)
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
T=${LZ4_TOOLS:-$HOME/.cache/fibber-scratch/tools/lz4}
REF=$T/lz4-1.10.0/programs/lz4
W=${LZ4_INTEROP_OUT:-$HOME/.cache/fibber-scratch/lz4-interop}
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$REF" ] || "$R/scripts/fetch-lz4-tools.sh" "$T" >/dev/null
mkdir -p "$W"
LZ4F=${1:-$W/lz4f}
[ -x "$LZ4F" ] || "$FIBC" build "$R/examples/lz4.fib" -o "$LZ4F" || exit 2
shift 2>/dev/null
FILES=("$@")
if [ ${#FILES[@]} -eq 0 ]; then
  python3 "$R/scripts/lz4-interop-inputs.py" "$W"
  FILES=("$W/empty" "$W/one" "$W/zeros" "$W/random" "$W/text" "$T/silesia/dickens" "$T/silesia/xml" "$T/silesia/sao")
fi
FLAGSETS=("" "-1" "-3" "-9" "-12" "-B4" "-B5 -BI" "-B6 -BD" "-B7 -BD" "-BX" "--no-frame-crc" "-9 -BI -BX --no-frame-crc")
n=0; bad=0
for f in "${FILES[@]}"; do
  for fl in "${FLAGSETS[@]}"; do
    n=$((n+1))
    # ours -> reference
    "$LZ4F" $fl "$f" "$W/o.lz4" && "$REF" -d -q -f "$W/o.lz4" "$W/o.out" 2>/dev/null && cmp -s "$f" "$W/o.out" || { echo "FAIL ours->ref   $(basename "$f") [$fl]"; bad=$((bad+1)); }
    # reference -> ours (the flag names are the same: -BD -BI -BX --no-frame-crc -B4..-B7 -1..-12)
    "$REF" -q -f $fl "$f" "$W/r.lz4" 2>/dev/null && "$LZ4F" -d "$W/r.lz4" "$W/r.out" && cmp -s "$f" "$W/r.out" || { echo "FAIL ref->ours   $(basename "$f") [$fl]"; bad=$((bad+1)); }
  done
done
echo "lz4-interop: $n combinations x 2 directions, $bad failures"
[ $bad = 0 ]
