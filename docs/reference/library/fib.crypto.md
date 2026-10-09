# fib.crypto

Generated declaration inventory of [fib.crypto](../../../lib/fib/crypto.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `AeadKey` | defprotocol | [lib/fib/crypto/types.fib:63](../../../lib/fib/crypto/types.fib#L63) |
| `CryptoError` | defstruct | [lib/fib/crypto/types.fib:9](../../../lib/fib/crypto/types.fib#L9) |
| `CryptoProvider` | defprotocol | [lib/fib/crypto/types.fib:136](../../../lib/fib/crypto/types.fib#L136) |
| `Hasher` | defprotocol | [lib/fib/crypto/types.fib:43](../../../lib/fib/crypto/types.fib#L43) |
| `Kdf` | defenum | [lib/fib/crypto/types.fib:26](../../../lib/fib/crypto/types.fib#L26) |
| `PerCallKey` | defstruct | [lib/fib/crypto/types.fib:83](../../../lib/fib/crypto/types.fib#L83) |
| `PrivateKey` | defprotocol | [lib/fib/crypto/types.fib:103](../../../lib/fib/crypto/types.fib#L103) |
| `PublicKey` | defprotocol | [lib/fib/crypto/types.fib:115](../../../lib/fib/crypto/types.fib#L115) |
| `aead-algs` | defun | [lib/fib/crypto/types.fib:49](../../../lib/fib/crypto/types.fib#L49) |
| `aead-key` | defun | [lib/fib/crypto/pk.fib:21](../../../lib/fib/crypto/pk.fib#L21) |
| `aead-key-length` | defun | [lib/fib/crypto/types.fib:50](../../../lib/fib/crypto/types.fib#L50) |
| `aead-nonce-length` | defun | [lib/fib/crypto/types.fib:51](../../../lib/fib/crypto/types.fib#L51) |
| `aead-open` | defun | [lib/fib/crypto/pk.fib:15](../../../lib/fib/crypto/pk.fib#L15) |
| `aead-seal` | defun | [lib/fib/crypto/pk.fib:11](../../../lib/fib/crypto/pk.fib#L11) |
| `aead-tag-length` | defun | [lib/fib/crypto/types.fib:52](../../../lib/fib/crypto/types.fib#L52) |
| `argon2id` | defun | [lib/fib/crypto/ops.fib:59](../../../lib/fib/crypto/ops.fib#L59) |
| `authentication-failed` | defun | [lib/fib/crypto/types.fib:15](../../../lib/fib/crypto/types.fib#L15) |
| `base64` | defun | [lib/fib/crypto/encoding.fib:64](../../../lib/fib/crypto/encoding.fib#L64) |
| `base64-nopad` | defun | [lib/fib/crypto/encoding.fib:65](../../../lib/fib/crypto/encoding.fib#L65) |
| `base64url` | defun | [lib/fib/crypto/encoding.fib:66](../../../lib/fib/crypto/encoding.fib#L66) |
| `base64url-nopad` | defun | [lib/fib/crypto/encoding.fib:67](../../../lib/fib/crypto/encoding.fib#L67) |
| `bytes-cat` | defun | [lib/fib/crypto/der.fib:21](../../../lib/fib/crypto/der.fib#L21) |
| `crypto-failed` | defun | [lib/fib/crypto/types.fib:20](../../../lib/fib/crypto/types.fib#L20) |
| `ct=` | defun | [lib/fib/crypto/ops.fib:66](../../../lib/fib/crypto/ops.fib#L66) |
| `digest` | defun | [lib/fib/crypto/ops.fib:35](../../../lib/fib/crypto/ops.fib#L35) |
| `digest-start` | defun | [lib/fib/crypto/ops.fib:13](../../../lib/fib/crypto/ops.fib#L13) |
| `feed` | defun | [lib/fib/crypto/ops.fib:19](../../../lib/fib/crypto/ops.fib#L19) |
| `feed-range` | defun | [lib/fib/crypto/ops.fib:23](../../../lib/fib/crypto/ops.fib#L23) |
| `generate-key` | defun | [lib/fib/crypto/pk.fib:23](../../../lib/fib/crypto/pk.fib#L23) |
| `hex` | defun | [lib/fib/crypto/encoding.fib:18](../../../lib/fib/crypto/encoding.fib#L18) |
| `hex-digits` | def | [lib/fib/crypto/encoding.fib:14](../../../lib/fib/crypto/encoding.fib#L14) |
| `hkdf` | defun | [lib/fib/crypto/ops.fib:45](../../../lib/fib/crypto/ops.fib#L45) |
| `hkdf-expand` | defun | [lib/fib/crypto/ops.fib:50](../../../lib/fib/crypto/ops.fib#L50) |
| `hkdf-extract` | defun | [lib/fib/crypto/ops.fib:53](../../../lib/fib/crypto/ops.fib#L53) |
| `hmac` | defun | [lib/fib/crypto/ops.fib:38](../../../lib/fib/crypto/ops.fib#L38) |
| `hmac-start` | defun | [lib/fib/crypto/ops.fib:16](../../../lib/fib/crypto/ops.fib#L16) |
| `import-private-key` | defun | [lib/fib/crypto/pk.fib:24](../../../lib/fib/crypto/pk.fib#L24) |
| `import-public-key` | defun | [lib/fib/crypto/pk.fib:25](../../../lib/fib/crypto/pk.fib#L25) |
| `invalid-argument` | defun | [lib/fib/crypto/types.fib:19](../../../lib/fib/crypto/types.fib#L19) |
| `invalid-signature` | defun | [lib/fib/crypto/types.fib:16](../../../lib/fib/crypto/types.fib#L16) |
| `kdf-name` | defun | [lib/fib/crypto/types.fib:34](../../../lib/fib/crypto/types.fib#L34) |
| `key-closed` | defun | [lib/fib/crypto/types.fib:85](../../../lib/fib/crypto/types.fib#L85) |
| `pbkdf2` | defun | [lib/fib/crypto/ops.fib:42](../../../lib/fib/crypto/ops.fib#L42) |
| `random-bytes` | defun | [lib/fib/crypto/ops.fib:63](../../../lib/fib/crypto/ops.fib#L63) |
| `raw-public-length` | defun | [lib/fib/crypto/der.fib:43](../../../lib/fib/crypto/der.fib#L43) |
| `scrypt` | defun | [lib/fib/crypto/ops.fib:56](../../../lib/fib/crypto/ops.fib#L56) |
| `std-alphabet` | def | [lib/fib/crypto/encoding.fib:15](../../../lib/fib/crypto/encoding.fib#L15) |
| `supports?` | defun | [lib/fib/crypto/ops.fib:9](../../../lib/fib/crypto/ops.fib#L9) |
| `unbase64` | defun | [lib/fib/crypto/encoding.fib:113](../../../lib/fib/crypto/encoding.fib#L113) |
| `unbase64-nopad` | defun | [lib/fib/crypto/encoding.fib:114](../../../lib/fib/crypto/encoding.fib#L114) |
| `unbase64url` | defun | [lib/fib/crypto/encoding.fib:115](../../../lib/fib/crypto/encoding.fib#L115) |
| `unbase64url-nopad` | defun | [lib/fib/crypto/encoding.fib:116](../../../lib/fib/crypto/encoding.fib#L116) |
| `unhex` | defun | [lib/fib/crypto/encoding.fib:32](../../../lib/fib/crypto/encoding.fib#L32) |
| `unsupported` | defun | [lib/fib/crypto/types.fib:18](../../../lib/fib/crypto/types.fib#L18) |
| `unsupported?` | defun | [lib/fib/crypto/types.fib:21](../../../lib/fib/crypto/types.fib#L21) |
| `url-alphabet` | def | [lib/fib/crypto/encoding.fib:16](../../../lib/fib/crypto/encoding.fib#L16) |
| `verify-signature` | defun | [lib/fib/crypto/pk.fib:28](../../../lib/fib/crypto/pk.fib#L28) |
| `wrap-private` | defun | [lib/fib/crypto/der.fib:37](../../../lib/fib/crypto/der.fib#L37) |
| `wrap-public` | defun | [lib/fib/crypto/der.fib:30](../../../lib/fib/crypto/der.fib#L30) |
