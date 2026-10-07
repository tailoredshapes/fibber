# fib.tls: a native TLS 1.3 client for fibber

**UNAUDITED.** Written from the RFCs, tested against the RFC 8448 traces, an in-memory server, OpenSSL's `s_server`, python's `ssl` and public sites. Nobody independent has reviewed it.
Do not rely on it as a security boundary until someone has. Design, threat model, what is and is not validated: `docs/design/tls.md`. Measurements: `docs/shootout/tls.md`.

TLS 1.3 **only**, **client only**: no TLS 1.2 or earlier (never), no server side, no client certificates, no resumption, no 0-RTT, no revocation checking (no OCSP, no CRL: a known gap), no
certificate transparency. Every cryptographic primitive is the crypto provider's (`fib.crypto`; a driver such as `fib-crypto-openssl` in its own repository): this library is the
protocol, the record layer, the key schedule's labels, ASN.1/X.509 parsing, path validation and host name matching.

## Use

```clojure
(ns app (:use fib.core) (:require [fib.tls :as tls] [fib.http.client :as hc] [fib.crypto.openssl :as openssl]))

(defun fetch (url: str) -> (Result Response Error)
  (match (openssl/open-provider)                                   ; the provider is the caller's choice: there is no default
    ((Err e) (Err (InvalidRequest (. e message))))
    ((Ok p) (match (tls/embedded-roots)                            ; or (tls/trust-from-file path), (tls/system-trust), your own TrustStore
              ((Err e) (Err (ConnectError (tls/tls-error-text e))))
              ((Ok trust) (hc/request (ClientRequest "GET" url {} (array 0 0i8))
                                      (tls/https-options (hc/default-options) p trust)))))))
```

Without `https-options` (or `(with-factory options "https" (tls/factory p trust))`) an `https` URL is `TlsUnsupported`: the client never connects and never falls back to plain text.
Over any byte stream: `(tls/connect p (tls/client-config "host" trust (fn () seconds-since-epoch)) io)` gives a `Session` (`read`, `write`, `close`, `set-deadline`, `alpn`, `suite`).

## What you get

- **Typed errors**: `BadCertificate reason` (expired, not-yet-valid, untrusted-root, hostname-mismatch, bad-signature, path-length, name-constraint, key-usage, weak-algorithm,
  unknown-critical-extension, parse, bad-chain), `HandshakeFailure`, `TlsProtocol`, `TlsAlert`, `UnsupportedCapability <what the provider lacks>`, `Truncated`, `TlsTimeout`, `TlsClosed`.
  **`Truncated`**: the stream ended without `close_notify` (an attacker can cut a stream; a clean end is only `close_notify`). Some servers close without it (www.google.com does); a
  length-framed HTTP response is complete anyway and `fib.http` is not affected; reading to the end of an unframed one reports it.
- **What is offered follows the provider**: suites, groups and signature schemes the provider lacks are not offered; a provider with none of a kind is `UnsupportedCapability`.
- **Trust is a value**: the Mozilla roots as data (`embedded-roots`, 121 certificates, dated 2026-09-25, regenerate with `scripts/gen-tls-roots.sh`), a PEM file, the system store
  (`system-trust`, an explicit call: never a silent default), or your own.
- **The clock is yours**: certificate validity is judged against the `now` you pass (seconds since the epoch).

## Not done, on purpose

Revocation; policies; pinning; a public suffix list (so `*.co.uk` matches); RSASSA-PSS- and SHA-512-signed certificates (refused as unsupported); zeroising secrets on free (the language
cannot yet: the provider cleanses its own buffers only). The list with reasons: `docs/design/tls.md` sections 9 and 10.

## Tests

Gate (offline, no driver): `specs/tls-*-spec.fib`. With the OpenSSL driver: `scripts/tls-test.sh`. Real servers: `scripts/tls-interop.sh`. Planted faults: `scripts/mutant-tls.sh`.
Benchmarks: `scripts/tls-bench.sh`. The certificate corpus: `scripts/tls-corpus.sh`; the RFC 8448 data: `scripts/gen-tls-trace.sh`.
