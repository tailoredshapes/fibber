#!/bin/bash
# Regenerates specs/tls-trace-data.fib from RFC 8448 (the TLS 1.3 example traces). The RFC text is fetched into scratch and checked against the sha256 recorded in
# specs/crypto-vectors.sha256 (ADR 0020); scripts/gen-tls-trace.py reads every value from the text by its label and octet count and refuses a value it cannot find.
#   scripts/gen-tls-trace.sh [DEST]      (default DEST ~/.cache/fibber-scratch/tools/crypto-vectors, where scripts/fetch-crypto-vectors.sh puts the RFCs)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dest=${1:-$HOME/.cache/fibber-scratch/tools/crypto-vectors}
mkdir -p "$dest"
[ -s "$dest/rfc8448.txt" ] || curl -sSfL -m 120 -o "$dest/rfc8448.txt" "https://www.rfc-editor.org/rfc/rfc8448.txt"
(cd "$dest" && grep ' rfc8448.txt$' "$here/../specs/crypto-vectors.sha256" | sha256sum -c --quiet -) || { echo "gen-tls-trace: rfc8448.txt does not match specs/crypto-vectors.sha256" >&2; exit 1; }
python3 "$here/gen-tls-trace.py" "$dest/rfc8448.txt" "$here/../specs/tls-trace-data.fib"
