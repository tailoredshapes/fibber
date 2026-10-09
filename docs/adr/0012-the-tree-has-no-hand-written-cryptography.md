# 0012. The tree has no hand-written cryptography

Status: accepted
Date: 2026-10-06 (extended 2026-10-07: AEAD, key agreement, signatures)
Source: the owner's standing rule (memory: "No hand-written crypto"); docs/design/crypto.md sections 1, 3 and 6; lib/fib/crypto/README.md.

## Context

fibber has no hash, MAC, key derivation function, cipher or random generator of its own. `fib.crypto` is a **protocol** (`CryptoProvider`)
and a **contract** every implementation must pass; the implementations are *drivers* in their own repositories that wrap OpenSSL or
CommonCrypto. A hand-written SHA-256 in a library "just for this one thing" is how a project ends up owning an unaudited cipher. The rule
was prose (a memory note and a design paragraph); nothing stopped a module from carrying its own round function.

## Decision

1. No module of `compiler/` or `lib/` (outside the tests) implements a hash, MAC, KDF or cipher. A heuristic stands for the rule: the
   well-known initial values and round constants (SHA-1, SHA-2, MD5) and the start of the AES S-box do not appear in live code.
2. There is no default provider: no `default-provider` in `lib/`. A program passes a provider as a value (docs/design/crypto.md 3).
3. No module of `lib/` requires a driver namespace (`fib.crypto.openssl`, `fib.crypto.commoncrypto`): drivers live in their own repositories.
4. The contract that every provider must pass, and the proofs that the contract can fail, stay in the tree: `fib.crypto.contract`, the
   fault decorator `fib.crypto.fault`, and the two gate specs of `specs/`.
5. (2026-10-07) The same holds for what the protocol grew: AEAD (AES-GCM, ChaCha20-Poly1305), key agreement (X25519, ECDH), signatures (RSA, ECDSA, Ed25519)
   and HKDF-Expand are a protocol, four contracts (`contract-aead`, `-kex`, `-sig`, `-tls`) over published vectors (`vectors-aead`, `-pk`, `-tls`, generated from
   RFC texts and Project Wycheproof by `scripts/gen-crypto-vectors-pk.sh`), and the faults of `fib.crypto.fault`; the primitives are a driver's. The only
   byte-level code of ours is `fib.crypto.der` (the fixed AlgorithmIdentifier headers of RFC 5480 and RFC 8410 that wrap a raw key) and `fib.crypto.encoding`:
   encodings of public data. The heuristic list below gains the ChaCha20 constants, Curve25519's `a24` and the P-256 prime. No fake provider is in `lib/`: the
   shape spec's `Refuses` provider is in `specs/`, lists capabilities, does no cryptography and answers an error to everything.

## Consequences

- A person adding a hash to `lib/` is stopped by `make adr` if the code carries the standard constants. Code that computes a hash without
  them (a table-free SHA, a rewritten constant) is not stopped: the rule is a heuristic, and review stays the rule.
- The non-cryptographic hashes of the library (map hashing, `fib.rng`) do not use these constants and are not touched.
- A driver repository runs the contract against its own provider; that run is not part of this repository's gate (there is no C library
  to bind in it).

## Governance

```fibber fitness
(defun hash-constants () -> (Vec str)
  ["0x428a2f98" "0x428A2F98" "0x6a09e667" "0x6A09E667" "0xbb67ae85" "0x67452301" "0xefcdab89" "0xEFCDAB89" "0x5be0cd19" "0xd76aa478"
   "0xc1059ed8" "0xcbbb9d5d" "0x6a09e667f3bcc908" "0xcbbb9d5dc1059ed8" "0x428a2f98d728ae22"
   "1116352408" "1779033703" "1732584193" "3614090360" "0x63 0x7c 0x77 0x7b" "99 124 119 123"
   "0x61707865" "0x3320646e" "0x79622d32" "0x6b206574" "1634760805" "121665" "0x0ffffffc0ffffffc" "0xffffffff00000001" "0xFFFFFFFF00000001"])

(defun constants-in (repo: Repo) -> (Vec Finding)
  (reduce (fn (acc: (Vec Finding) c: str) (into acc (grep-live repo ["lib/**.fib" "compiler/**.fib" "!compiler/tests/**"] c))) [] (hash-constants)))

(rule "no live line of lib/ or compiler/ carries a SHA, MD5 or AES constant"
  (constants-in repo)
  (plant "lib/fib/crypto/ops.fib" "\n(def k0: i64 0x428a2f98)\n")
  (plant "lib/fib/otel/ids.fib" "\n(def sbox: (Array i64) [0x63 0x7c 0x77 0x7b 0xf2])\n")
  (plant "lib/fib/crypto/pk.fib" "\n(def sigma0: i64 0x61707865)\n")
  (plant "lib/fib/crypto/der.fib" "\n(def a24: i64 121665)\n"))

(rule "no definition named default-provider (a provider is a value the program passes)"
  (grep-live repo ["lib/**.fib" "compiler/**.fib" "!compiler/tests/**"] "default-provider")
  (plant "lib/fib/crypto/ops.fib" "\n(defun default-provider () -> i64 0)\n"))

(rule "no module of lib/ requires a crypto driver"
  (into (requires-into repo ["lib/**.fib"] "fib.crypto.openssl" [])
        (into (requires-into repo ["lib/**.fib"] "fib.crypto.commoncrypto" [])
              (requires-into repo ["lib/**.fib"] "fib.crypto.libcrypto" [])))
  (plant-file "lib/fib/zz-plant.fib" "(ns fib.zz-plant (:require [fib.crypto.openssl :as o]))\n"))

(rule "the contract, the fault decorator and the gate's two specs are in the tree"
  (missing repo ["lib/fib/crypto/contract.fib" "lib/fib/crypto/fault.fib" "specs/crypto-contract-shape-spec.fib" "specs/crypto-encoding-spec.fib"
                "lib/fib/crypto/contract-aead.fib" "lib/fib/crypto/contract-kex.fib" "lib/fib/crypto/contract-sig.fib" "lib/fib/crypto/contract-tls.fib"])
  (plant-remove "lib/fib/crypto/contract.fib")
  (plant-remove "specs/crypto-contract-shape-spec.fib")
  (plant-remove "lib/fib/crypto/contract-kex.fib"))

(rule "the AEAD and key-agreement contracts keep their negative cases (a tag that does not verify, a small-order point)"
  (into (must-contain repo "lib/fib/crypto/contract-aead.fib" "authentication-failed")
        (must-contain repo "lib/fib/crypto/contract-kex.fib" "small-order-points"))
  (plant-file "lib/fib/crypto/contract-aead.fib" "(ns fib.crypto.contract-aead)\n"))

(rule "the contract requires the capabilities a protocol consumer needs, and the decorator can fake a fallback"
  (into (must-contain repo "lib/fib/crypto/contract.fib" "(defun required-capabilities () -> (Vec keyword) [:sha-256 :sha-384 :sha-512 :hmac :pbkdf2 :hkdf :random :ct-equal])")
        (must-contain repo "lib/fib/crypto/fault.fib" ":fallback"))
  (plant-file "lib/fib/crypto/contract.fib" "(ns fib.crypto.contract)\n"))
```

### What this does not check

That a provider passes the contract (it runs in the driver's repository, against a real library, and against `Faulty` to show it can
fail: docs/design/crypto.md 6); that code which computes a primitive without the well-known constants is absent (the rule is a
heuristic over a list of constants); `compiler/tests/`, `cases/` and `scripts/` (a test may carry a vector); that a driver does not
depend on something unsafe (it is not in this repository).
