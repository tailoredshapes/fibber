# fib.tls: handshake latency and throughput

**Measured** 2026-10-07 with `scripts/tls-bench.sh` (the script prints its own header and every command's result; this file quotes one run). Nothing was tuned: correctness came first, this is
the first measurement. The client under test is `fib.tls` over the OpenSSL driver (`fib-crypto-openssl` v0.2.0, OpenSSL 3.5.5) built with `fibc build` (`-O 2`); the references are
OpenSSL 3.5.5's `s_client` and `s_time` and curl, all to the **same** `openssl s_server -WWW` on localhost, an ECDSA P-256 certificate, X25519. Machine: Intel i7-14700KF, 28 logical CPUs,
Linux 7.0, nothing else heavy running during the bulk runs (the first handshake run overlapped a test run: its p90 is noisier).

```
FIBC=<stage 2> DRIVER=<fib-crypto-openssl/src> MB=256 HS=200 scripts/tls-bench.sh
```

## Handshake latency (new connection, full handshake, no resumption, TLS_AES_128_GCM_SHA256)

| client | result |
|---|---|
| `fib.tls` (`tls-cli ... hs 200`, TCP connect + handshake + close, wall clock per connection) | median 0.45 ms, p10 0.43 ms, p90 1.23 ms (n=200); a second run: median 0.45 ms, p90 0.52 ms |
| `openssl s_time -new -time 5` | 11389 connections in 6 real seconds (about 0.53 ms each, 3301 per user second) |
| curl, 200 whole processes | 1.12 s total, 5.6 ms each (process start dominates) |

The handshake costs the client one X25519 key generation and agreement, one ECDSA verification of the leaf (the chain here is leaf + root: two verifications), the key schedule (about
20 HMAC/HKDF calls through the provider) and 6 record seals/opens. At 0.45 ms it is within noise of `s_time` (whose number includes the server's work in the same process pair).

## Bulk download of 256 MiB, one connection, 64 KiB reads

| suite | `fib.tls` | `openssl s_client` (to a pipe and `wc -c`) | curl (to /dev/null) |
|---|---|---|---|
| TLS_AES_128_GCM_SHA256 | 633, 607, 536 MB/s | 1268, 2028, 1738 MB/s | 2244, 2123, 2240 MB/s |
| TLS_CHACHA20_POLY1305_SHA256 | 537, 533, 535 MB/s | 1394, 1440, 1242 MB/s | 1478, 1440, 1250 MB/s |

(Three runs each, in the order run.) `fib.tls` reads at about **2-4x less than OpenSSL's own client or curl for AES-GCM** and 2.3-2.8x less for ChaCha20-Poly1305. The gap is expected and is not
the AEAD: the AEAD itself is the same libcrypto code. Where it goes, in the order the code makes it likely (not profiled: not yet measured, so do not trust this order):

1. **A provider call per 16 KiB record** that fetches the cipher, creates and frees a context and copies the record into and out of driver buffers: the known per-call cipher-fetch overhead
   of the OpenSSL driver, 25-40 % at 16 KiB records, as the driver's own benchmark (`scripts/bench-pk.sh` in fib-crypto-openssl; docs/design/crypto.md section 10) found (an incremental or context-reusing AEAD in the provider protocol would remove it).
2. **Byte-at-a-time copies in fibber**: there is no array blit (section 11 of docs/design/tls.md); `record.fib` copies the record out of the input buffer, the plaintext out of the
   decrypted array, and the input buffer's remainder after every record, each as an element loop. A 64 KiB read is four records and about a dozen such copies.
3. The nonce XOR and the inner-type scan are per record and small.

So the numbers are an honest first measurement of **correctness-first code**, not a statement about the best fibber can do. Two cheap levers are visible and untried: a provider AEAD that keeps
its context for a key (the largest), and `array-copy`-style builtins for the record copies.

## Not measured

Memory use; CPU use of the client alone (the server shares the machine); larger records or other sizes of read; the P-256 and P-384 groups and the RSA and Ed25519 certificates; resumption (not built);
other machines (the Mac and the Ryzen box of the memory notes were not used: the driver ran only here).

## After CRYPTO-3 (2026-10-07): key handles, into-buffer calls, a read buffer, `array-blit!`

**Measured** with `FIBC=<stage 2> DRIVER=<fib-crypto-openssl v0.3.0 src> MB=256 HS=100 scripts/tls-bench.sh` (same machine and the same `s_server`; the machine was shared with other agents' jobs, so the
spread is wide: the three runs of each row are quoted). What changed in `fib.tls`: a key handle per direction (the cipher fetched and the key set once, section 9.1 of docs/design/crypto.md),
`aead-key-open-into` straight out of a byte queue into one array per record, one array per `send` for all the records of a write (`aead-key-seal-into`, one reused inner-plaintext buffer), reads of at
least 128 KiB, and `array-blit!` for every copy (`fib.tls.buf`).

| suite | `fib.tls` before | `fib.tls` after | `openssl s_client` (same run) | curl (same run) |
|---|---|---|---|---|
| TLS_AES_128_GCM_SHA256 | 633, 607, 536 MB/s | 946, 1709, 1209 MB/s | 1596, 1725, 1198 MB/s | 1744, 1590, 1712 MB/s |
| TLS_CHACHA20_POLY1305_SHA256 | 537, 533, 535 MB/s | 1033, 998, 962 MB/s | 1056, 1099, 1080 MB/s | 1192, 1120, 1130 MB/s |

AES-128-GCM went from 2 to 4 times slower than `s_client`/curl to about 1.0 to 1.7 times slower (the median run 1209 against 1596: 1.32x); ChaCha20-Poly1305 is within 1.1x of `s_client`. The
target of 1.5x is met on the median of both suites and missed on the slowest AES run (946 against 1596). What is left is not profiled: the read of the transport (a fresh array per `read`,
copied once into the queue), one allocation of the plaintext per record, and the copy out to the 64 KiB array the caller asked for. Handshake latency in the same run: median 1.01 ms, p10 0.60 ms,
p90 1.10 ms (n=100), against 0.45 ms before; the run shared the machine and a handshake now makes three key handles (six contexts), so the figure is not a conclusion: it should be measured again
on a quiet machine before anything is said of it.

The driver alone (`scripts/bench-pk.sh` of fib-crypto-openssl, 28 cores, MB/s of 10^6 bytes, wall time of the whole process):

```
aes-128-gcm           16384 bytes   per-call 4280   handle 6647   handle+into 7466   openssl speed 7629
aes-128-gcm         1048576 bytes   per-call 6510   handle 6528   handle+into 7718   openssl speed 8014
aes-256-gcm           16384 bytes   per-call 4046   handle 6301   handle+into 6598   openssl speed 6661
aes-256-gcm         1048576 bytes   per-call 5395   handle 5748   handle+into 6472   openssl speed 6753
chacha20-poly1305     16384 bytes   per-call 1956   handle 2343   handle+into 2437   openssl speed 2585
chacha20-poly1305   1048576 bytes   per-call 2417   handle 2348   handle+into 2504   openssl speed 2532
```
A handle that seals into one buffer is within 1.02 to 1.07 times `openssl speed` at 16 KiB and 1 MiB for the three ciphers (target 1.15).
