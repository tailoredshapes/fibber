#!/bin/bash
# Generates lib/fib/crypto/vectors-aead.fib, vectors-pk.fib and vectors-tls.fib: the published test vectors of the AEAD, key agreement and signature
# contracts, and of the TLS 1.3 key schedule (RFC 8448), as fibber data. Run once, commit the result; the gate and the driver repositories run fibber
# code only. This script is the ONLY place python3 and the `cryptography` package are used, and they are the oracle, not the source:
#   - the sources are PUBLISHED files, fetched and checksummed by scripts/fetch-crypto-vectors.sh (RFC texts, Project Wycheproof);
#   - every vector taken from an RFC must appear in that RFC's text (the script searches the text for the hex), and the script stops when it does not;
#   - every VALID vector is recomputed with python's `cryptography` (OpenSSL underneath: a second implementation, not a second library) and must
#     match; every INVALID one must fail there too (a Wycheproof "invalid" that python accepts stops the script);
#   - the selection is deterministic: all the known-answer vectors, then a fixed number per distinct Wycheproof flag set. The files carry a SUBSET.
# usage: scripts/gen-crypto-vectors-pk.sh [SRC]       (SRC: the directory fetch-crypto-vectors.sh filled; it runs the fetch when absent)
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=${1:-$HOME/.cache/fibber-scratch/tools/crypto-vectors}
scripts/fetch-crypto-vectors.sh "$SRC"
SRC="$SRC" python3 scripts/lib/gen_crypto_vectors_pk.py
