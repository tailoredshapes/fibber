#!/bin/bash
# linux-only-file: needs the Linux libcrypto.so.3 through ldconfig and LD_PRELOAD; not part of the Mac gate
# Interop of the native TLS 1.3 client (fib.tls) against real servers: `openssl s_server` (OpenSSL 3.x CLI) and python's ssl module, with a throwaway CA generated into scratch,
# and, when this machine has outbound network, a few public sites with the embedded roots and with the system store. Prints one row per case: `PASS` or `FAIL` and the client's
# own RESULT line. Not part of the gate (it needs openssl, python3, the OpenSSL driver and a network port).
#   FIBC=<stage-2 fibc> DRIVER=<fib-crypto-openssl/src> [WORK=dir] scripts/tls-interop.sh
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
F=${FIBC:?set FIBC to a fibc of this tree (see CLAUDE.md: build stage 2)}
DRIVER=${DRIVER:?set DRIVER to the src directory of fib-crypto-openssl (tag v0.2.0)}
W=${WORK:-$HOME/.cache/fibber-scratch/tls-interop}
mkdir -p "$W/pki"; cd "$W/pki"
ulimit -v 16000000
export LD_PRELOAD=$(ldconfig -p | awk '/libcrypto\.so\.3 .*(x86-64|AArch64)/ { print $NF; exit }')
export FIB_LIB="$here/lib:$DRIVER"
[ -f ca.pem ] || {
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout ca.key -out ca.pem -days 3000 -subj "/CN=Interop CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
  for k in rsa ec ed ec384; do
    case $k in rsa) openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out $k.key 2>/dev/null;; ec) openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out $k.key;;
      ec384) openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out $k.key;; ed) openssl genpkey -algorithm ED25519 -out $k.key;; esac
    openssl req -new -key $k.key -subj "/CN=localhost" -out $k.csr
    printf "subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature\nextendedKeyUsage=serverAuth\n" > $k.ext
    openssl x509 -req -in $k.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out $k.pem -days 3000 -extfile $k.ext 2>/dev/null
  done
}
"$F" build "$here/tls-specs/tls-cli.fib" -I "$here/lib" -I "$DRIVER" -o "$W/tls-cli" 2>&1 | tail -3
CLI="$W/tls-cli"
pass=0; fail=0
row() { # name expected-substring output
  if printf '%s' "$3" | grep -q -- "$2"; then pass=$((pass+1)); echo "PASS | $1 | $(printf '%s' "$3" | tail -1 | cut -c1-110)"; else fail=$((fail+1)); echo "FAIL | $1 | $(printf '%s' "$3" | tail -1 | cut -c1-160)"; fi
}
port=15000
s_server() { port=$((port+1)); openssl s_server -accept $port -cert "$1.pem" -key "$1.key" -www -quiet "${@:2}" >/dev/null 2>&1 & spid=$!; sleep 0.4; }
stop() { kill $spid 2>/dev/null; wait $spid 2>/dev/null; }
cli() { timeout 60 "$CLI" "$@" 2>&1; }

echo "== openssl s_server: suite x group x certificate"
for suite in TLS_AES_128_GCM_SHA256 TLS_AES_256_GCM_SHA384 TLS_CHACHA20_POLY1305_SHA256; do
  for grp in X25519 P-256 P-384; do
    for cert in rsa ec ed; do
      s_server $cert -ciphersuites $suite -groups $grp
      row "$suite $grp $cert$([ $grp != X25519 ] && echo ' (HelloRetryRequest)')" "RESULT ok HTTP/1.0 200" "$(cli localhost $port "$W/pki/ca.pem" get)"; stop
    done
  done
done
s_server ec384 -ciphersuites TLS_AES_256_GCM_SHA384; row "P-384 ECDSA certificate" "RESULT ok HTTP/1.0 200" "$(cli localhost $port "$W/pki/ca.pem" get)"; stop
echo "== client authentication requested"
s_server ec -Verify 1 -CAfile ca.pem; row "server REQUIRES a client certificate: the client has none: certificate_required" "certificate_required" "$(cli localhost $port "$W/pki/ca.pem" get)"; stop
s_server ec -verify 1 -CAfile ca.pem; row "server ASKS for a client certificate: answered with an empty Certificate" "RESULT ok HTTP/1.0 200" "$(cli localhost $port "$W/pki/ca.pem" get)"; stop
echo "== wrong trust, wrong name"
s_server ec; row "an unknown CA is untrusted-root" "untrusted-root" "$(cli localhost $port "$W/pki/rsa.pem" get)"; stop
s_server ec; row "a name that is not in the certificate" "hostname-mismatch" "$(cli 127.0.0.2 $port "$W/pki/ca.pem" get)"; stop
s_server ec; row "an IP literal host with an IP SAN" "RESULT ok HTTP/1.0 200" "$(cli 127.0.0.1 $port "$W/pki/ca.pem" get)"; stop
echo "== through fib.http (the Transport seam)"
s_server rsa; row "(request ...) with https-options over s_server -www" "RESULT ok HTTP status 200" "$(cli localhost $port "$W/pki/ca.pem" http)"; stop
echo "== python ssl server: large payloads, truncation"
python3 "$here/scripts/tls-pyserver.py" 15200 "$W/pki/ec.pem" "$W/pki/ec.key" & spid=$!; sleep 0.8
row "GET / (ALPN http/1.1)" "RESULT ok HTTP/1.0 200" "$(cli localhost 15200 "$W/pki/ca.pem" get)"
sum=$(python3 -c "
b=bytes(97+(i*7)%26 for i in range(1048576)); a=0
for x in b: a=(a*31+x)%1000000007
print(a)")
out=$(cli localhost 15200 "$W/pki/ca.pem" down /big); row "1 MiB download (python's checksum $sum, 'bytes' includes the headers)" "checksum" "$out"
echo "     download output: $(printf '%s' "$out" | tail -1 | cut -c1-120)"
row "1 MiB upload" "RESULT ok HTTP/1.0 200" "$(cli localhost 15200 "$W/pki/ca.pem" up 1048576)"
row "a connection closed WITHOUT close_notify is a truncation, not a clean end" "without close_notify" "$(cli localhost 15200 "$W/pki/ca.pem" down /trunc)"
stop
echo "== public sites (needs outbound network)"
if curl -sI -m 8 https://example.com >/dev/null 2>&1; then
  for host in example.com github.com www.cloudflare.com letsencrypt.org www.wikipedia.org; do
    row "$host, embedded Mozilla roots" "RESULT ok" "$(cli $host 443 embedded get)"
    row "$host, the system store" "RESULT ok" "$(cli $host 443 system get)"
  done
  row "www.google.com through fib.http (length-framed: its missing close_notify does not matter)" "RESULT ok HTTP status 200" "$(cli www.google.com 443 embedded http)"
  row "www.google.com read to EOF: it closes WITHOUT close_notify, reported as a truncation, not as a clean end" "without close_notify" "$(cli www.google.com 443 embedded down /generate_204)"
  row "expired.badssl.com is a TLS 1.2-only server: refused with its handshake_failure alert, never downgraded" "handshake_failure" "$(cli expired.badssl.com 443 embedded get)"
  row "self-signed.badssl.com (TLS 1.2 only): the same" "handshake_failure" "$(cli self-signed.badssl.com 443 embedded get)"
  row "https via fib.http to example.com" "RESULT ok HTTP status 200" "$(cli example.com 443 embedded http)"
else echo "no outbound network: public sites skipped"; fi
echo "interop: $pass passed, $fail failed"
[ $fail -eq 0 ]
