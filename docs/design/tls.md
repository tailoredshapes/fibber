# fib.tls: a native TLS 1.3 client

Status: built (TLS-1), 2026-10-07. **UNAUDITED.** Written from RFC 8446, 8448, 5280, 9525 and tested against the RFC 8448 traces, an in-memory server, `openssl s_server`, python's
`ssl` and public sites; never reviewed by an independent cryptographer or protocol engineer. Do not treat it as a security boundary for anything that matters until someone has.
**Measured** means a command was run and its output is quoted in `docs/shootout/tls.md` or in the package report; everything else is design or a statement of scope.

Contents: 1 scope and the rules; 2 layout; 3 the protocol as built; 4 certificates and trust; 5 what is and is not validated; 6 threat model; 7 wiring; 8 tests (what runs in the
gate, what runs with a driver); 9 deviations and known gaps; 10 deferred; 11 what the language lacked.

## 1. Scope and the rules

A TLS **1.3 client** over `fib.crypto`, plugged into the HTTP-1 `Transport` seam. The owner's rules, which shaped it:

- **No primitive is ours** (ADR 0012). AEAD, HMAC, HKDF-Expand, the digest, key agreement, signature verification, the random generator and the constant-time comparison are the
  crypto provider's (`CryptoProvider`, a driver in its own repository). What is ours is the *protocol*: the record layer, the handshake, the key-schedule labels, ASN.1 and X.509
  parsing, path validation, host name matching.
- **The provider is a value** the caller passes (ADR 0013): there is no default provider and no global trust store. `fib.tls` requires no driver namespace; `fib.http` does not require
  `fib.tls`.
- **Fail closed, no downgrade.** Only TLS 1.3 is offered (`supported_versions = [0x0304]`); a ServerHello without it, with the TLS 1.2 or 1.1 downgrade sentinel, or with a suite that
  was not offered, ends the handshake. There is no weaker-protocol, weaker-suite or weaker-check fallback, and no switch that turns certificate verification off.
- A provider that lacks something names it: `UnsupportedCapability "<what>"`.

## 2. Layout (`lib/fib/tls/`, facade `lib/fib/tls.fib`)

| module | what |
|---|---|
| `bytes` | big-endian integers, vectors with length prefixes, concatenation (no cryptography) |
| `types` | `TlsError`, `CertReason`, alert numbers and names |
| `asn1` | the strict DER reader (`Tlv`, `der-parse`, `der-kids`, `der-walk`, typed contents) |
| `x509-base`, `x509-ext`, `x509` | times, names, algorithms; extensions; the `Cert` value |
| `names` | IP literals, host name matching, name-constraint subtrees |
| `pem`, `roots` | PEM, `TrustStore`; the Mozilla roots as data (generated) |
| `chain` | path building and validation (`validate-chain`) |
| `msg`, `msg-parse` | the numbers of RFC 8446; encoders (ClientHello); parsers of what the client receives |
| `keys` | the key schedule: HkdfLabel, Derive-Secret, traffic keys, Finished, the nonce |
| `record` | the record layer over an `Io` |
| `hello`, `hs`, `hs-flight` | the configuration and ClientHello; the handshake (first half, second half) |
| `conn` | a `Session`: application data, KeyUpdate, close_notify, truncation |
| `client`, `http` | `connect` over an `Io`; the `fib.http` `Transport` and `"https"` `Factory` |

Every file is under 500 lines and every function under 50 (ADR 0006); `roots.fib` is generated data of 139 long lines.

## 3. The protocol as built

**ClientHello**: legacy_version 0x0303, a 32-byte random and a 32-byte `legacy_session_id` from the provider (middlebox compatibility mode: a `change_cipher_spec` record is sent
once), cipher suites `TLS_AES_128_GCM_SHA256`, `TLS_CHACHA20_POLY1305_SHA256`, `TLS_AES_256_GCM_SHA384` in that order **filtered by the provider's capabilities**, extensions
`server_name` (not for IP literals), `supported_groups` (x25519, secp256r1, secp384r1, filtered), `signature_algorithms` (ecdsa_secp256r1_sha256, ecdsa_secp384r1_sha384,
rsa_pss_rsae_sha256/384, ed25519, filtered), `signature_algorithms_cert` (those plus rsa_pkcs1_sha256/384, because servers choose their chain by it), `supported_versions`, one
`key_share` for the preferred group, ALPN `http/1.1` (configurable). No `psk_key_exchange_modes` (no resumption), no 0-RTT, no padding.

**ServerHello / HelloRetryRequest**: version, session-id echo, suite offered, downgrade sentinels (`DOWNGRD\x01`, `\x00`), unsolicited extensions, duplicate extensions, share length,
and for a HelloRetryRequest: the group was offered and differs (or a cookie is present), one retry only, the transcript restarts as `message_hash(Hash(ClientHello1)) || HRR ||
ClientHello2` (RFC 8446 4.4.1), the second ServerHello keeps the suite. **Key schedule**: early, handshake and master secrets, client and server handshake and application traffic
secrets, keys and IVs, exactly as RFC 8446 7 (the HkdfLabel encoding and every label are in `keys.fib`; HKDF-Extract is the provider's HMAC, HKDF-Expand its `KdfHkdfExpand`).
**Flight**: EncryptedExtensions (ALPN must be one that was offered; unsolicited extensions are refused), CertificateRequest (answered with an empty Certificate: no client
certificates here), Certificate (parsed and validated, section 4), CertificateVerify (the 64 spaces, the context string, a zero byte and the transcript hash, verified with the
leaf's key under a scheme that was offered and fits the key), Finished (HMAC over the transcript, compared with the provider's constant-time comparison). The client's Finished and
the application keys follow.

**Records**: TLSPlaintext and TLSCiphertext framing; 2^14 plaintext, 2^14+256 ciphertext limits; inner content type and padding (none is written; zeros are skipped on read; an
all-zero inner plaintext is refused); the nonce is the IV XOR the 64-bit sequence number; a plaintext record once the read keys are on is refused (except the compatibility
`change_cipher_spec`, at most two, body 0x01, only during the handshake); handshake messages may span records and share a record, but an empty handshake record, an alert or
application data inside a half-received message, and a message over 2^18 bytes are refused; a message may not span a key change. **Sequence numbers**: the writer sends a
KeyUpdate before 2^24 records under one key (the AES-GCM limit of RFC 8446 5.5); both directions refuse at 2^40. **KeyUpdate** in both directions (a request is answered once,
at most 1024 per connection); **NewSessionTicket** is checked for shape and ignored; any other post-handshake handshake message is `unexpected_message`.

**Closing**: the peer's `close_notify` is a clean end (`read` returns the empty array and keeps doing so); the transport ending without it is `Truncated`, an error, because an
attacker can cut a stream (RFC 8446 6.1). A fatal error sends the alert that names it (best effort), closes the transport and is returned by every later call. `Session.close`
sends `close_notify`. **Deadlines**: `set-deadline` is the stream's; the handshake runs under `handshake-timeout-ms` (default 10 s) and then the deadline is cleared.

**Errors** (`TlsError`): `HandshakeFailure alert message`, `BadCertificate reason message` (reasons: expired, not-yet-valid, untrusted-root, hostname-mismatch, bad-signature,
path-length, name-constraint, key-usage, weak-algorithm, unknown-critical-extension, parse, bad-chain), `UnsupportedCapability what`, `TlsProtocol alert message`, `TlsAlert desc name`,
`Truncated`, `TlsTimeout`, `TlsClosed`, `TransportFailed`. Through the `fib.http` seam they become `Timeout "io"`, `Closed`, `ConnectError "tls: ..."` (handshake and certificate),
`ProtocolError` (record, alert, truncation) and `TransportError`.

## 4. Certificates and trust

**DER** is strict (X.690 section 10): definite lengths only, the minimal length encoding, no high tag numbers, every length checked against the bytes present, BOOLEAN 0x00/0xFF, minimal
INTEGERs, zero unused BIT STRING bits, OID sub-identifiers without a leading 0x80, nesting depth at most 24, no trailing bytes. No input makes the reader trap (fuzzed, section 8).
**Certificates**: v1 and v3, `tbsCertificate` fields, the two signature algorithm fields must be equal, extensions only in v3, no duplicate extensions, no unique IDs,
`UTCTime`/`GeneralizedTime` in the DER form (`Z`, seconds, no fraction; see deviations). **Extensions understood**: basicConstraints, keyUsage, extKeyUsage, subjectAltName
(dNSName, iPAddress), nameConstraints (dNSName, iPAddress), subject/authorityKeyIdentifier. **Recognised and not processed**: certificatePolicies, cRLDistributionPoints,
authorityInfoAccess, subjectInfoAccess, issuerAltName, freshestCRL, SCTs. **Unknown critical extensions are refused**, as are policyMappings, policyConstraints and inhibitAnyPolicy
when critical (their processing is not built).

**Path validation** (`validate-chain`): the path is built by name (the issuer name equals the subject name; the key identifiers must agree when both are present); every signature is
verified through `fib.crypto` (`import-public-key` + `verify-signature`); the first path whose signatures verify and whose checks pass is the answer, trust anchors preferred over
presented intermediates; then: validity of every certificate against the **caller's clock** (inclusive), SHA-1/MD5/MD2/SHA-224 signatures refused (weak-algorithm), RSA keys under 2048
bits refused (weak-algorithm), every issuer a CA (basicConstraints cA; keyCertSign when keyUsage is present), pathLenConstraint over the non-self-issued intermediates, the leaf's
keyUsage (digitalSignature when present) and extKeyUsage (serverAuth or anyExtendedKeyUsage; an issuer's EKU restricts the same way), nameConstraints (dNSName and iPAddress
evaluated; a constrained name type the library cannot judge fails closed *when the certificates use it*), and the **host**: against the subjectAltName only. **Name normalisation** for
issuer/subject matching: the DER bytes are equal, or the canonical forms are: per attribute the type OID and the value with ASCII letters lower-cased, leading and trailing spaces
removed and inner runs of spaces made one (UTF8String, PrintableString, IA5String, TeletexString), other string types as bytes, the values of a multi-valued RDN sorted. No Unicode
case folding or normalisation: a name that differs only by it does not match (a chain is refused, never accepted wrongly). **Host names** (RFC 6125 6.4 / RFC 9525 6.3): case-insensitive;
a wildcard is the whole left-most label, covers exactly one label, needs two labels after it (`*.com` matches nothing; `*.co.uk` still does: no public suffix list); `a*.example.com`
and `*.*.example.com` match nothing; an IP literal host matches only an iPAddress SAN of the same bytes and never a dNSName; **there is no common-name fallback**.

**Trust is a value** (`TrustStore`): `trust-from-pem` (a PEM bundle; a certificate that does not parse refuses the whole store), `trust-from-file`, `system-trust` (the first of
`/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt`, `/etc/ssl/cert.pem`, `/etc/ssl/ca-bundle.pem`: a platform default for the client factory only; an error when
none exists), and **`embedded-roots`**: `lib/fib/tls/roots.fib`, the Mozilla roots as data (121 certificates from the curl project's extract `cacert-2026-09-25.pem`, sha256
`a41b5d35...0505`), made by `scripts/gen-tls-roots.sh` from the dated file, which it checks against the recorded sha256 (ADR 0020) and refuses on a mismatch. **Decision**: the
generated file (175 KB) is *committed*, not made at build time: there is no build step in fibber, an offline gate and a scratch image with no `/etc/ssl` need it as a file, and the
generator plus the checksum make the diff of an update reviewable. It ages: the caller decides whether to trust Mozilla's list of that date, a file, or a store of its own. All 121 roots
parse (about 2 ms). A target's `ca-file` (the `fib.http` client option) replaces the factory's trust for that connection.

**Not done**: revocation (no OCSP, no CRL, no stapling check: a revoked certificate is accepted); certificate transparency; policy processing; RSASSA-PSS-signed certificates and SHA-512
certificate signatures (refused as an unsupported signature algorithm: the provider's scheme list has neither); DSA.

## 5. What is and is not validated

Validated, by tests that fail when the code is wrong (section 8): the RFC 8448 simple-1-RTT and HelloRetryRequest traces **byte for byte** (ClientHello layouts, every secret, the
Finished data, every byte the client wrote); interop with OpenSSL's `s_server` in all three suites x three groups x RSA/ECDSA/Ed25519 certificates (the non-X25519 groups
force a HelloRetryRequest) and python's `ssl`; a certificate corpus of 38 chains with the typed reason of each; 1 MiB each way; truncation and corruption of the server's flight at
every byte; 200 000 fuzz inputs; 18 planted faults, each killed.

**Not validated**: constant-time behaviour (the comparisons and the primitives are the provider's; this code branches on nothing secret that it computes, but it was not measured);
behaviour against servers other than OpenSSL and the sites tried; the provider's own bugs; memory: arrays are not zeroised when freed (see 9); anything on a platform other than
x86-64 Linux (the driver and fibber's aarch64 target exist; nothing here was run on them).

## 6. Threat model

In scope: an active network attacker who can read, change, drop, reorder, replay and truncate the byte stream, and who may control a server or a certificate authority the trust store
does not contain. The client authenticates the server (chain, name, CertificateVerify, Finished) and the transcript, protects records, and tells an end of data from a cut. Out of scope:
a compromised trusted CA (no revocation, no transparency, no pinning), a compromised or malicious crypto provider, side channels beyond what the provider gives, a hostile
local process (secrets are in ordinary heap arrays), denial of service by a slow peer beyond the deadlines (a message is capped at 256 KiB, a certificate list at 16 entries,
KeyUpdates at 1024).

## 7. Wiring

```
(require [fib.http.client :as hc] [fib.tls :as tls] [fib.crypto.openssl :as openssl])    ; the driver is the caller's choice
(let [p (openssl/open-provider) trust (tls/embedded-roots)]
  (hc/request (ClientRequest "GET" "https://example.com/" {} (array 0 0i8))
              (tls/https-options (hc/default-options) p trust)))
```
`fib.http`'s defaults still map `https` to nothing: without a registered factory the client answers `TlsUnsupported` and never connects or downgrades (ADR 0013: no global default).
Because `fib.http` does not require `fib.tls`, a program that never touches TLS links no crypto driver; one that does passes the provider and the trust. `tls/connect provider cfg io`
works over any byte stream (an `Io` of closures: a socket, a pipe, an in-memory pair); `tls/tls-transport provider cfg transport` wraps a `fib.http` `Transport`.

## 8. Tests

**In the gate** (offline, driver-free; `specs/`): `tls-der-spec` (tables, times, 20 000 mutated certificates), `tls-names-spec` (host matching, IP literals, constraints), `tls-chain-spec`
(the 38-chain corpus with the signatures answered by a stub), `tls-msg-spec` (the ClientHello, every received message cut at every byte, the capability filtering),
`tls-record-spec` (split at every byte, coalescing, limits, change_cipher_spec, what is refused before decrypting). The stub (`tls-stub`) does no cryptography: it answers `unsupported`
to every primitive and accepts or rejects a signature as told, so the gate judges everything of the path logic that is not a signature.
**With the driver** (`scripts/tls-test.sh`; `tls-specs/`; the driver is the OpenSSL one at tag v0.2.0): `tls-rfc8448-spec` (the traces, through a deterministic decorator, `tls-det`, that
answers the RFC's keys and randoms), `tls-chain-real-spec` (the corpus with real signatures, including three wrong ones), `tls-e2e-spec` (an in-memory server, `tls-server`, written with
the library's own schedule and a real provider, and able to break its flight in 8 ways and to send 15 post-handshake events; 1 MiB each way; cut and flip at every byte), `tls-fuzz`.
**With real servers** (`scripts/tls-interop.sh`, `scripts/tls-pyserver.py`): the matrix, client authentication, wrong trust and wrong name, truncation, public sites.
**Planted faults** (`scripts/mutant-tls.sh`): 18, each must fail a spec. The corpus is made by `scripts/tls-corpus.sh` (openssl CLI; committed with `specs/tls-fixtures.sha256`); the
traces by `scripts/gen-tls-trace.sh` from RFC 8448 (checksummed by `specs/crypto-vectors.sha256`). Test ideas written as tables rather than copied: Go `crypto/x509` and BoringSSL
host-name and constraint cases (`tls-names-spec`), Wycheproof's signature and key vectors are `fib.crypto`'s (the provider's), RFC 9525's wildcard examples.

## 9. Deviations and known gaps

- `GeneralizedTime` is accepted for any year from 1950 (RFC 5280 reserves it for 2050 and later; real roots break that, as OpenSSL and Go accept them).
- Certificates signed with RSASSA-PSS or SHA-512, DSA, or with parameters this library does not know are refused (`bad-signature`: "not supported"), not skipped.
- A wildcard in a name constraint check is judged as its parent with a placeholder label; an excluded subtree under a wildcard SAN can be missed. dirName, email and URI constraints fail
  closed only when the certificates use that type (the subject DN counts for dirName).
- No revocation; no certificate policies; no CT; no pinning.
- **Secrets are not zeroised**: traffic secrets, keys and IVs live in ordinary `(Array i8)` values that the allocator frees without clearing. Only the provider's own buffers are cleansed
  (the driver does). Nothing in the language today can wipe an array in place and be sure the optimiser keeps the write; a `fib.crypto` primitive for it (a provider operation) is the way.
- Every record seal/open crosses the provider with the key and nonce (the OpenSSL driver fetches the cipher per call: the known 25-40% at 16 KiB records of docs/shootout/crypto.md).
- Hostnames: a trailing dot on the reference is dropped; internationalised names must be given as A-labels; no Unicode processing.
- The post-handshake deadline is the stream's `set-deadline`; there is no separate idle timer.

## 10. Deferred

Server side (the record layer, schedule and messages are symmetric; the certificate checks are not needed): its own package. Client certificates (the `Certificate` and `CertificateVerify`
messages of the client; `private-key-sign` exists for Ed25519 and ECDSA). Resumption: *design only*: accept `NewSessionTicket` (today checked and dropped), keep `resumption_master_secret`
(`Derive-Secret(master, "res master", transcript-through-client-Finished)`), offer `pre_shared_key` with `psk_key_exchange_modes = [psk_dhe_ke]` and a binder (`Derive-Secret(early,
"res binder", "")` -> finished key -> HMAC of the truncated ClientHello transcript); never offer without ECDHE; the ticket store would be a value the caller owns. **0-RTT is never the
default**; if ever built it is opt-in per request, for idempotent methods only, and replay-aware. TLS 1.2 and earlier: never. Revocation (OCSP stapling check, CRL sets), pinning, a public
suffix list for wildcards, RSA-PSS certificate signatures (needs a provider scheme), hybrid post-quantum key shares (needs provider support).

## 11. What the language lacked

- **A byte-array blit and builder**: every concatenation is an element loop (`cat`); a `array-copy-into` or an append-with-capacity would make the record layer's copies memcpys.
- **Heterogeneous literals**: a table of `[name bytes expected]` has to be a struct; the specs have a dozen of them.
- **No `try-let` over mixed error types**: every provider call is wrapped by `lift`; a Result-mapping form would remove noise.
- **A way to wipe an array** (section 9) and **a way to say "this value must not be copied"** for keys.
- Closure fields cannot be called as `(f x)` on a field access without the double parenthesis `((. s read) n)`; easy to get wrong.
- The `cond`/`match` indentation depth: a handshake is straight-line code that wants `do`-notation.
- A `fib.test` runner that finds a spec's `main` by itself (every spec file ends in `(defun main () (run-main (specs)))`), and `-I` flags for `fibc test` (an environment variable was needed).
