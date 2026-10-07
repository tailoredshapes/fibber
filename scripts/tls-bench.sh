#!/bin/bash
# Benchmark of the native TLS 1.3 client (fib.tls over the OpenSSL driver) against `openssl s_client`, `openssl s_time` and curl, all to the same `openssl s_server` on localhost:
# handshake latency (new connections, no resumption) and bulk download throughput (64 KiB reads) for AES-128-GCM and ChaCha20-Poly1305. Prints the commands it runs and the
# numbers it measured; docs/shootout/tls.md records one run. Not tuned: this is the first measurement, correctness came first.
#   FIBC=<stage-2 fibc> DRIVER=<fib-crypto-openssl/src> [MB=256] [HS=200] [WORK=dir] scripts/tls-bench.sh
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
F=${FIBC:?set FIBC}; DRIVER=${DRIVER:?set DRIVER}; MB=${MB:-256}; HS=${HS:-200}
W=${WORK:-$HOME/.cache/fibber-scratch/tls-bench}; mkdir -p "$W"; cd "$W"
ulimit -v 16000000
export LD_PRELOAD=$(ldconfig -p | awk '/libcrypto\.so\.3 .*(x86-64|AArch64)/ { print $NF; exit }')
export FIB_LIB="$here/lib:$DRIVER"
echo "# $(date -u +%FT%TZ) $(nproc) cpus, $(openssl version), $(uname -srm), cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2)"
[ -f ca.pem ] || {
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout ca.key -out ca.pem -days 3000 -subj "/CN=Bench CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ec.key
  openssl req -new -key ec.key -subj "/CN=localhost" -out ec.csr
  printf "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature\nextendedKeyUsage=serverAuth\n" > ec.ext
  openssl x509 -req -in ec.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out ec.pem -days 3000 -extfile ec.ext 2>/dev/null
}
[ -f big.bin ] || head -c $((MB * 1048576)) /dev/zero | tr '\0' 'x' > big.bin
"$F" build "$here/tls-specs/tls-cli.fib" -I "$here/lib" -I "$DRIVER" -o tls-cli 2>&1 | tail -3
port=16000
serve() { port=$((port+1)); openssl s_server -accept $port -cert ec.pem -key ec.key -WWW -quiet "$@" >/dev/null 2>&1 & spid=$!; sleep 0.5; }
stop() { kill $spid 2>/dev/null; wait $spid 2>/dev/null; }

echo "## handshake latency: $HS new connections (full handshake, X25519, ECDSA P-256 certificate, TLS_AES_128_GCM_SHA256)"
serve -ciphersuites TLS_AES_128_GCM_SHA256 -groups X25519
echo "fib.tls: $(./tls-cli localhost $port ca.pem hs $HS | tail -1 | python3 -c "
import sys,statistics,re
t=[int(x) for x in re.findall(r'-?\d+', sys.stdin.read().split('handshake-us')[1])]
t.sort(); print('median %.2f ms, p10 %.2f ms, p90 %.2f ms (n=%d)' % (statistics.median(t)/1000, t[len(t)//10]/1000, t[len(t)*9//10]/1000, len(t)))")"
echo "openssl s_time: $(openssl s_time -connect localhost:$port -new -time 5 -CAfile ca.pem -tls1_3 -ciphersuites TLS_AES_128_GCM_SHA256 2>&1 | grep -E 'connections in|ms/connection' | tr '\n' ' ')"
echo "curl (n=$HS, whole process each, includes startup): $( { time (for i in $(seq $HS); do curl -s --cacert ca.pem -o /dev/null https://localhost:$port/ec.ext; done) ; } 2>&1 | grep real | awk '{print $2}')"
stop

for suite in TLS_AES_128_GCM_SHA256 TLS_CHACHA20_POLY1305_SHA256; do
  echo "## bulk download of $MB MiB, $suite (64 KiB reads), one connection"
  serve -ciphersuites $suite -groups X25519
  flag=--aes128; [ $suite = TLS_CHACHA20_POLY1305_SHA256 ] && flag=--chacha
  for run in 1 2 3; do echo "fib.tls: $(./tls-cli localhost $port ca.pem bulk /big.bin $flag | tail -1)"; done
  for run in 1 2 3; do
    s=$(date +%s.%N); printf 'GET /big.bin HTTP/1.0\r\n\r\n' | openssl s_client -connect localhost:$port -CAfile ca.pem -quiet -ciphersuites $suite -tls1_3 2>/dev/null | wc -c > count.txt; e=$(date +%s.%N)
    echo "openssl s_client: $(cat count.txt) bytes in $(python3 -c "print('%.2f' % ($e-$s))") s = $(python3 -c "print('%.0f' % ($MB/($e-$s)))") MB/s (includes writing to a pipe and wc)"
  done
  for run in 1 2 3; do echo "curl: $(curl -s --cacert ca.pem --tlsv1.3 --tls13-ciphers $suite -o /dev/null -w '%{size_download} bytes in %{time_total} s = %{speed_download} B/s' https://localhost:$port/big.bin)"; done
  stop
done
