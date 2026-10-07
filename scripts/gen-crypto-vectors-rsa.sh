#!/bin/bash
# Generates lib/fib/crypto/vectors-rsa.fib (the RSA keys and RSASSA-PSS signatures of the RSA signing contract). Run once, commit the result: the keys are fresh each run
# (python's `cryptography` is the oracle and the key generator; nothing here is a primitive of ours), so a rerun CHANGES the file and the counts the shape spec fixes
# stay the same. This script and scripts/gen-crypto-vectors-pk.sh are the only places python is used.
# usage: scripts/gen-crypto-vectors-rsa.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=lib/fib/crypto python3 scripts/lib/gen_crypto_vectors_rsa.py
