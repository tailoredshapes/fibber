#!/bin/bash
# The TLS tests that need a crypto driver (the gate has none): the RFC 8448 traces byte for byte, the end-to-end specs against the in-memory server, the certificate corpus
# with real signature verification, and the mutation fuzz (FUZZ inputs of each kind, default 100000). The gate's own TLS specs (specs/tls-*-spec.fib: DER, names, chain
# logic with stubbed signatures, messages) run in `scripts/gate.sh`; interop with real servers is scripts/tls-interop.sh; planted faults are scripts/mutant-tls.sh.
#   FIBC=<stage-2 fibc of this tree> DRIVER=<src directory of fib-crypto-openssl, v0.3.0 (the AEAD key handles and RSA signing: fibber CRYPTO-3)> [FUZZ=100000] scripts/tls-test.sh
# The driver is its own repository (ssh://git@localhost:2222/tailoredshapes/fib-crypto-openssl.git): clone it at v0.3.0 (branch crypto-3 until the tag), it needs libcrypto.so.3 (OpenSSL 3).
set -euo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC to a stage-2 fibc of this tree}
DRIVER=${DRIVER:?set DRIVER to the src directory of fib-crypto-openssl}
FUZZ=${FUZZ:-100000}
lib=$(ldconfig -p | awk '/libcrypto\.so\.3 .*(x86-64|AArch64)/ { print $NF; exit }')
[ -n "$lib" ] || { echo "tls-test: libcrypto.so.3 not found (ldconfig -p): OpenSSL 3 is required" >&2; exit 2; }
ulimit -v 16000000 || true
cd "$R"
export FIB_LIB="$R/lib:$R/specs:$R/tls-specs:$DRIVER"
for s in tls-rfc8448-spec tls-chain-real-spec tls-e2e-spec; do
  echo "== $s"
  LD_PRELOAD="$lib" "$FIBC" test "tls-specs/$s.fib" --seed 1 | tail -n 3
done
scratch=$(mktemp -d "${TMPDIR:-/tmp}/tls-test.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
echo "== mutation fuzz: $FUZZ mutated certificates and $FUZZ mutated server flights"
"$FIBC" build tls-specs/tls-fuzz.fib -I lib -I "$DRIVER" -o "$scratch/fuzz" 2>&1 | tail -n 3
"$scratch/fuzz" "$FUZZ"
