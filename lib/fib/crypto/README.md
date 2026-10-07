# fib.crypto

An **interface** for cryptography in fibber, and nothing that does cryptography. fibber has no hash, MAC, key derivation function or random
generator of its own: a **driver** (its own repository) implements `CryptoProvider` over a C library, and your program passes the provider
as a value. Design: `docs/design/crypto.md`.

## Read this first

- **Not audited.** Nobody has reviewed this interface or the drivers for security. The cryptography is the C library's (OpenSSL, CommonCrypto);
  what is ours is the binding and the encodings.
- **No constant-time guarantee beyond `ct=`**, and `ct=` is the driver library's comparison (`CRYPTO_memcmp`, `timingsafe_bcmp`). Nothing else
  in fibber is written to be constant-time: do not compare MACs, tokens or passwords with `=`.
- Secrets are not zeroised from fibber arrays when they are freed. A driver cleanses the C buffers it allocates and the library cleanses its own
  contexts; the arrays you hold keys in are ordinary arrays.
- **TLS is not here.** `fib.tls` is a later package; this library is what it needs (below), and certificate parsing is its job, not ours.
- **No fallback.** An algorithm the provider does not have is an `unsupported` `CryptoError`, never a weaker substitute.
- `:sha-1` and `:md5` are *legacy, not for security*: for protocol compatibility only.

## Use

```clojure
(ns main (:use fib.core fib.seq fib.coll)
  (:require [fib.crypto :as c] [fib.crypto.openssl :as openssl]))   ; the driver: tailoredshapes/fib-crypto-openssl

(defun main () -> i64
  (match (openssl/open-provider)
    ((Err e) (do (println "no crypto:" (. e message)) 1))
    ((Ok p)
     (do (println (match (c/digest p :sha-256 (str-bytes "abc")) ((Ok d) (c/hex d)) ((Err e) (. e message))))
         (println (match (c/hmac p :sha-256 (str-bytes "key") (str-bytes "data")) ((Ok d) (c/base64 d)) ((Err e) (. e message))))
         (println (match (c/pbkdf2 p :sha-256 (str-bytes "pw") (str-bytes "salt") 4096 32) ((Ok d) (c/hex d)) ((Err e) (. e message))))
         (println (c/supports? p :argon2id))
         0))))
```

Streaming, without copying a large input:

```clojure
(match (c/digest-start p :sha-256)
  ((Ok h) (do (c/feed-range h big 0 1048576) (c/feed-range h big 1048576 1048576) (c/hasher-finish h)))   ; (Result (Array i8) CryptoError)
  ((Err e) (Err e)))
```

A hasher is finished once (`c/hasher-finish`) or closed (`c/hasher-close`); a hasher that is abandoned leaks its native context until the process
ends (fibber has no scope-exit hook yet). The one-shot functions never leak.

A library that needs cryptography takes the provider as a parameter (`:where ((c/CryptoProvider p))`, or a `(dyn c/CryptoProvider :send)` field)
and never chooses one itself.

## AEAD, key agreement, signatures (docs/design/crypto.md 9)

```clojure
(c/aead-seal p :aes-256-gcm key nonce aad plaintext)        ; ciphertext || 16-byte tag; also :aes-128-gcm :chacha20-poly1305; nonce 12 bytes, never reused under a key
(c/aead-open p :aes-256-gcm key nonce aad sealed)           ; the plaintext, or Err "authentication-failed" and nothing else
(match (c/generate-key p :x25519)                           ; keys are handles the driver owns: close them
  ((Ok k) (do (c/private-key-public k) (c/private-key-agree k peer-raw) (c/private-key-close k))))
(match (c/import-public-key p spki-der)                     ; X.509 SubjectPublicKeyInfo DER
  ((Ok k) (c/public-key-verify k :ecdsa-p256-sha256 msg sig)))   ; Ok, or Err "invalid-signature" / "invalid-argument"
(c/verify-signature p spki-der :ed25519 msg sig)            ; import, verify, release
(c/hkdf-expand p :sha-256 prk info 42)  (c/hkdf-extract p :sha-256 salt ikm)   ; the building blocks of HKDF-Expand-Label
```

Capability names: `:aes-128-gcm :aes-256-gcm :chacha20-poly1305`, `:x25519 :ecdh-p256 :ecdh-p384`, the verify schemes `:rsa-pss-sha256 :rsa-pss-sha384 :rsa-pkcs1-sha256
:rsa-pkcs1-sha384 :ecdsa-p256-sha256 :ecdsa-p384-sha384 :ed25519`, signing `:sign-ed25519 :sign-ecdsa-p256-sha256 :sign-ecdsa-p384-sha384`. A provider without them
answers `unsupported` (the protocol methods have defaults). Private keys never leave the driver as bytes; `provider-import-private` takes PKCS#8 DER for known-answer tests
and stored keys. `fib.crypto.der` wraps a raw X25519 or Ed25519 key in the PKCS#8 or SPKI DER a driver imports.

## Key handles and RSA signing (docs/design/crypto.md 9.1, 9.2)

```clojure
(match (c/aead-key p :aes-128-gcm key)                      ; cipher and key bound once: (Result (dyn AeadKey) CryptoError); close it
  ((Ok k) (do (c/aead-key-seal k nonce aad pt)               ; per record: only nonce, AAD, data
              (c/aead-key-seal-into k nonce aad data off len out out-off)   ; (Result (Array i8) ..): `out` is handed over, the array that comes back is the one to use
              (c/aead-key-close k))))
```
A driver that does not override `provider-aead-key` gets a per-call handle by default. `private-key-sign` also takes `:rsa-pss-sha256` and `:rsa-pss-sha384` (capabilities `:sign-rsa-pss-sha256`
`:sign-rsa-pss-sha384`) for a key imported from PKCS#8 as `:rsa`. Contracts: `CryptoAeadKeyContract` (fib.crypto.contract-aeadkey) and `CryptoRsaSignContract` (fib.crypto.contract-rsa).

## Encodings (ours, in fibber)

`hex` `unhex` `base64` `unbase64` `base64-nopad` `unbase64-nopad` `base64url` `unbase64url` `base64url-nopad` `unbase64url-nopad`. The decoders
are strict (alphabet, padding, length, trailing bits) and return `(Result (Array i8) CryptoError)`.

## For a driver author

Implement `Hasher` and `CryptoProvider` (`lib/fib/crypto/types.fib`; the public-key methods and the key handle protocols `PrivateKey` and `PublicKey` are optional) and run the contracts in a spec:

```clojure
(ns main (:use fib.core fib.seq fib.coll fib.test.core fib.test.run fib.crypto.contract fib.crypto.fault) (:require [fib.crypto :as c]))
(defspecs specs (implements CryptoContract "mydriver" (fn () (my-provider))))
(defun main () -> i64 (run-main (specs)))
```

and run it against `(Faulty (dyn c/CryptoProvider :send (my-provider)) :flip-bit)` (and the other faults in `fib.crypto.fault/faults`) to see it
fail. For the public-key part run `CryptoAeadContract`, `CryptoKexContract`, `CryptoSignatureContract` and `CryptoTlsKdfContract` the same way, with the faults in
`fib.crypto.fault/pk-faults` (`fault-scenarios` names the scenario each must break). The vectors are `fib.crypto.vectors`, generated by `scripts/gen-crypto-vectors.sh`, and
`vectors-aead`, `vectors-pk` and `vectors-tls`, generated by `scripts/gen-crypto-vectors-pk.sh` from RFC texts and Project Wycheproof (`scripts/fetch-crypto-vectors.sh` checksums them). Drivers: `tailoredshapes/fib-crypto-openssl`.
