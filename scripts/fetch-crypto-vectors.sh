#!/bin/bash
# Fetches the PUBLISHED sources of the fib.crypto vectors into scratch and verifies each against specs/crypto-vectors.sha256 (committed; the
# files are not): the RFC texts (rfc-editor.org) and Project Wycheproof's test vector files (C2SP/wycheproof, pinned commit). The gate never
# calls this; scripts/gen-crypto-vectors.sh reads the files from DEST and writes the subset that lib/fib/crypto/vectors*.fib carries.
# usage: scripts/fetch-crypto-vectors.sh [DEST] [--record]   (default DEST ~/.cache/fibber-scratch/tools/crypto-vectors; --record rewrites the list)
set -eu
COMMIT=${WYCHEPROOF_COMMIT:-12fd3aaf33eb5fa1f52e026912ee00c054f9d984}
here=$(cd "$(dirname "$0")" && pwd)
dest=${1:-$HOME/.cache/fibber-scratch/tools/crypto-vectors}
record=no; [ "${2:-}" = "--record" ] && record=yes
mkdir -p "$dest"
RFCS="5869 7748 8032 8439 8448 8446"
WYCHE="aes_gcm chacha20_poly1305 x25519 ed25519 ecdsa_secp256r1_sha256 ecdsa_secp384r1_sha384 rsa_pss_2048_sha256_mgf1_32 rsa_pss_2048_sha384_mgf1_48 rsa_pss_2048_sha256_mgf1sha1_20 rsa_pss_2048_sha256_mgf1_0 rsa_signature_2048_sha256 rsa_signature_2048_sha384 ecdh_secp256r1 ecdh_secp384r1 ecdh_secp256r1_ecpoint"
for r in $RFCS; do
  [ -s "$dest/rfc$r.txt" ] || curl -sSfL -m 120 -o "$dest/rfc$r.txt" "https://www.rfc-editor.org/rfc/rfc$r.txt"
done
for w in $WYCHE; do
  [ -s "$dest/$w.json" ] || curl -sSfL -m 120 -o "$dest/$w.json" "https://raw.githubusercontent.com/C2SP/wycheproof/$COMMIT/testvectors_v1/${w}_test.json"
done
if [ "$record" = yes ]; then
  (cd "$dest" && sha256sum rfc*.txt *.json) > "$here/../specs/crypto-vectors.sha256"
fi
(cd "$dest" && sha256sum -c --quiet "$here/../specs/crypto-vectors.sha256") && echo "crypto vector sources verified in $dest"
