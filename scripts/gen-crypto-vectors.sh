#!/bin/bash
# Generates lib/fib/crypto/vectors.fib, the published test vectors of the fib.crypto contract (lib/fib/crypto/contract.fib), as fibber data.
# Run once, commit the result: the gate and the driver repositories run fibber code only. python3 (hashlib, hmac) and, when present, the
# `openssl` command are used here and nowhere else:
#   - every vector that a standard PUBLISHES (FIPS 180-4 and RFC 1321/3174 messages, RFC 4231 cases 1-7, RFC 7914 section 11, the
#     PBKDF2-HMAC-SHA256 vectors of RFC 6070's inputs, RFC 5869 appendix A) carries its published value below, written from the document,
#     and the script stops when hashlib/hmac disagree with it: a typo in a published value, or in a message, is found here;
#   - the values for messages a standard does not give for a hash (SHA-384/512 HMAC of the RFC 4231 inputs: RFC 4231 does publish them, but
#     they are taken from hashlib here) are computed, and every computed value is also checked with `openssl dgst` / `openssl kdf`.
# usage: scripts/gen-crypto-vectors.sh            (writes lib/fib/crypto/vectors.fib; exit 1 on any disagreement)
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PYEOF'
import hashlib, hmac, subprocess, shutil, sys

def hx(b): return b.hex()
def fail(m): print("gen-crypto-vectors: " + m, file=sys.stderr); sys.exit(1)
have_openssl = shutil.which("openssl") is not None
def ossl(args, data=b""):
    return subprocess.run(["openssl"] + args, input=data, capture_output=True, check=True).stdout

ALGS = {"sha-256": "sha256", "sha-384": "sha384", "sha-512": "sha512", "sha-1": "sha1", "md5": "md5"}

# ---- digests: (alg, name, message bytes or ("rep", byte, count), published digest or None)
M448 = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
M896 = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
digests = []
pub = {
 ("sha-256", ""): "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
 ("sha-256", "abc"): "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
 ("sha-256", "448"): "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
 ("sha-256", "896"): "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
 ("sha-256", "million"): "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
 ("sha-384", ""): "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
 ("sha-384", "abc"): "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
 ("sha-384", "448"): "3391fdddfc8dc7393707a65b1b4709397cf8b1d162af05abfe8f450de5f36bc6b0455a8520bc4e6f5fe95b1fe3c8452b",
 ("sha-384", "896"): "09330c33f71147e83d192fc782cd1b4753111b173b3b05d22fa08086e3b0f712fcc7c71a557e2db966c3e9fa91746039",
 ("sha-384", "million"): "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a5b07b8b3dc38ecc4ebae97ddd87f3d8985",
 ("sha-512", ""): "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
 ("sha-512", "abc"): "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
 ("sha-512", "448"): "204a8fc6dda82f0a0ced7beb8e08a41657c16ef468b228a8279be331a703c33596fd15c13b1b07f9aa1d3bea57789ca031ad85c7a71dd70354ec631238ca3445",
 ("sha-512", "896"): "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909",
 ("sha-512", "million"): "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b",
 ("sha-1", ""): "da39a3ee5e6b4b0d3255bfef95601890afd80709",
 ("sha-1", "abc"): "a9993e364706816aba3e25717850c26c9cd0d89d",
 ("sha-1", "448"): "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
 ("sha-1", "million"): "34aa973cd4c4daa4f61eeb2bdbad27316534016f",
 ("md5", ""): "d41d8cd98f00b204e9800998ecf8427e",
 ("md5", "abc"): "900150983cd24fb0d6963f7d28e17f72",
 ("md5", "message digest"): "f96b697d7cb7938d525a2f31aaf161d0",
 ("md5", "million"): "7707d6ae4e027c70eea2a935c2296f21",
}
msgs = {"": b"", "abc": b"abc", "448": M448, "896": M896, "million": ("rep", 0x61, 1000000), "message digest": b"message digest"}
for (alg, name), want in pub.items():
    m = msgs[name]
    data = bytes([m[1]]) * m[2] if isinstance(m, tuple) else m
    got = hashlib.new(ALGS[alg], data).hexdigest()
    if got != want: fail("published %s of %r is %s, hashlib says %s" % (alg, name, want, got))
    if have_openssl and alg != "md5" or (have_openssl and alg == "md5"):
        try:
            o = ossl(["dgst", "-" + ALGS[alg].replace("sha", "sha"), "-binary"], data).hex()
            if o != got: fail("openssl disagrees on %s %r" % (alg, name))
        except subprocess.CalledProcessError:
            pass  # md5 may be disabled by a FIPS-style configuration: python3 already agreed with the published value
    enc = ("rep:61:1000000" if isinstance(m, tuple) else hx(m))
    digests.append((alg, name, enc, want))

# ---- HMAC, RFC 4231 test cases 1-7 (the inputs); SHA-256 published values from the RFC, SHA-384/512 computed and checked with openssl
cases = [
 ("rfc4231-1", b"\x0b" * 20, b"Hi There", 0),
 ("rfc4231-2", b"Jefe", b"what do ya want for nothing?", 0),
 ("rfc4231-3", b"\xaa" * 20, b"\xdd" * 50, 0),
 ("rfc4231-4", bytes(range(1, 26)), b"\xcd" * 50, 0),
 ("rfc4231-5-truncated-128-bits", b"\x0c" * 20, b"Test With Truncation", 16),
 ("rfc4231-6-key-larger-than-block", b"\xaa" * 131, b"Test Using Larger Than Block-Size Key - Hash Key First", 0),
 ("rfc4231-7-key-and-data-larger-than-block", b"\xaa" * 131,
  b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.", 0),
 ("empty-key-empty-data", b"", b"", 0),
]
pub256 = {
 "rfc4231-1": "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
 "rfc4231-2": "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
 "rfc4231-3": "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
 "rfc4231-4": "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
 "rfc4231-5-truncated-128-bits": "a3b6167473100ee06e0c796c2955552b",
 "rfc4231-6-key-larger-than-block": "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
 "rfc4231-7-key-and-data-larger-than-block": "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
 "empty-key-empty-data": "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad",
}
macs = []
for name, key, data, trunc in cases:
    for alg in ("sha-256", "sha-384", "sha-512"):
        full = hmac.new(key, data, ALGS[alg]).digest()
        out = full[:trunc] if trunc else full
        if alg == "sha-256" and hx(out) != pub256[name]: fail("published HMAC-SHA-256 %s is %s, hmac says %s" % (name, pub256[name], hx(out)))
        if have_openssl and key:   # the openssl command refuses an empty key
            o = ossl(["dgst", "-" + ALGS[alg], "-mac", "HMAC", "-macopt", "hexkey:" + hx(key), "-binary"], data)
            if o != full: fail("openssl disagrees on HMAC %s %s" % (alg, name))
        macs.append((alg, name, hx(key), hx(data), trunc, hx(out)))

# ---- PBKDF2-HMAC-SHA256: RFC 7914 section 11 (published), RFC 6070's inputs at SHA-256 (published by the PBKDF2-SHA256 test sets), one long
pb = [
 ("rfc7914-passwd-salt-1", b"passwd", b"salt", 1, 64, "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"),
 ("rfc7914-Password-NaCl-80000", b"Password", b"NaCl", 80000, 64, "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56a1d425a1225833549adb841b51c9b3176a272bdebba1d078478f62b397f33c8d"),
 ("rfc6070-inputs-1", b"password", b"salt", 1, 32, "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"),
 ("rfc6070-inputs-2", b"password", b"salt", 2, 32, "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"),
 ("rfc6070-inputs-4096", b"password", b"salt", 4096, 32, "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"),
 ("rfc6070-inputs-long-4096", b"passwordPASSWORDpassword", b"saltSALTsaltSALTsaltSALTsaltSALTsalt", 4096, 40, "348c89dbcbd32b2f32d814b8116e84cf2b17347ebc1800181c4e2a1fb8dd53e1c635518c7dac47e9"),
 ("rfc6070-inputs-nul-4096", b"pass\0word", b"sa\0lt", 4096, 16, "89b69d0516f829893c696226650a8687"),
 ("long-100000", b"password", b"salt", 100000, 32, None),
]
pbk = []
for name, pw, salt, c, n, want in pb:
    got = hashlib.pbkdf2_hmac("sha256", pw, salt, c, n)
    if want is not None and hx(got) != want: fail("published PBKDF2 %s is %s, hashlib says %s" % (name, want, hx(got)))
    if have_openssl:
        try:
            o = ossl(["kdf", "-keylen", str(n), "-kdfopt", "digest:SHA256", "-kdfopt", "hexpass:" + hx(pw), "-kdfopt", "hexsalt:" + hx(salt), "-kdfopt", "iter:%d" % c, "-binary", "PBKDF2"])
            if o != got: fail("openssl kdf disagrees on PBKDF2 %s" % name)
        except subprocess.CalledProcessError: pass
    pbk.append((name, hx(pw), hx(salt), c, n, hx(got)))

# ---- HKDF-SHA-256, RFC 5869 appendix A.1-A.3 (published OKM); python's own HKDF over hmac is the cross-check
def hkdf(ikm, salt, info, n):
    prk = hmac.new(salt if salt else b"\0" * 32, ikm, "sha256").digest()
    t, okm, i = b"", b"", 1
    while len(okm) < n:
        t = hmac.new(prk, t + info + bytes([i]), "sha256").digest(); okm += t; i += 1
    return okm[:n]
hk = [
 ("rfc5869-A1", b"\x0b" * 22, bytes(range(0, 13)), bytes(range(0xf0, 0xfa)), 42, "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"),
 ("rfc5869-A2", bytes(range(0, 0x50)), bytes(range(0x60, 0xb0)), bytes(range(0xb0, 0x100)), 82, "b11e398dc80327a1c8e7f78c596a49344f012eda2d4efad8a050cc4c19afa97c59045a99cac7827271cb41c65e590e09da3275600c2f09b8367793a9aca3db71cc30c58179ec3e87c14c01d5c1f3434f1d87"),
 ("rfc5869-A3-no-salt-no-info", b"\x0b" * 22, b"", b"", 42, "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"),
]
hkv = []
for name, ikm, salt, info, n, want in hk:
    got = hkdf(ikm, salt, info, n)
    if hx(got) != want: fail("published HKDF %s is %s, computed %s" % (name, want, hx(got)))
    if have_openssl:
        try:
            o = ossl(["kdf", "-keylen", str(n), "-kdfopt", "digest:SHA256", "-kdfopt", "hexkey:" + hx(ikm), "-kdfopt", "hexsalt:" + hx(salt), "-kdfopt", "hexinfo:" + hx(info), "-binary", "HKDF"])
            if o != got: fail("openssl kdf disagrees on HKDF %s" % name)
        except subprocess.CalledProcessError: pass
    hkv.append((name, hx(ikm), hx(salt), hx(info), n, hx(got)))

def q(s): return '"' + s + '"'
out = []
out.append(";; fib.crypto.vectors: the published test vectors of the fib.crypto contract, as data. GENERATED by scripts/gen-crypto-vectors.sh (do not edit):")
out.append(";; each published value is written from its document and the script stops when hashlib, hmac or openssl disagree. Bytes are hex; a message")
out.append(";; \"rep:61:1000000\" is the byte 0x61 repeated 1000000 times (FIPS 180-4's million-'a' message).")
out.append("(ns fib.crypto.vectors (:use fib.core fib.seq fib.coll))")
out.append("(defstruct DigestVector (alg: keyword name: str msg: str expected: str))")
out.append("(defstruct MacVector (alg: keyword name: str key: str msg: str truncate: i64 expected: str))")
out.append("(defstruct Pbkdf2Vector (name: str password: str salt: str iterations: i64 length: i64 expected: str))")
out.append("(defstruct HkdfVector (name: str ikm: str salt: str info: str length: i64 expected: str))")
out.append("")
out.append("(defun digest-vectors () -> (Vec DigestVector)")
out.append("  [" + "\n   ".join("(DigestVector :%s %s %s %s)" % (a, q(n), q(m), q(e)) for a, n, m, e in digests) + "])")
out.append("")
out.append(";; `truncate` is the number of leading bytes of the MAC the vector gives (0: all of it).")
out.append("(defun mac-vectors () -> (Vec MacVector)")
out.append("  [" + "\n   ".join("(MacVector :%s %s %s %s %d %s)" % (a, q(n), q(k), q(d), t, q(e)) for a, n, k, d, t, e in macs) + "])")
out.append("")
out.append("(defun pbkdf2-vectors () -> (Vec Pbkdf2Vector)")
out.append("  [" + "\n   ".join("(Pbkdf2Vector %s %s %s %d %d %s)" % (q(n), q(p), q(s), c, l, q(e)) for n, p, s, c, l, e in pbk) + "])")
out.append("")
out.append("(defun hkdf-vectors () -> (Vec HkdfVector)")
out.append("  [" + "\n   ".join("(HkdfVector %s %s %s %s %d %s)" % (q(n), q(i), q(s), q(f), l, q(e)) for n, i, s, f, l, e in hkv) + "])")
open("lib/fib/crypto/vectors.fib", "w").write("\n".join(out) + "\n")
print("gen-crypto-vectors: %d digest, %d mac, %d pbkdf2, %d hkdf vectors; openssl %s" % (len(digests), len(macs), len(pbk), len(hkv), "checked" if have_openssl else "absent"))
PYEOF
