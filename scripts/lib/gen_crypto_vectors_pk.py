#!/usr/bin/env python3
# The generator behind scripts/gen-crypto-vectors-pk.sh (read that file's header). Writes lib/fib/crypto/vectors-aead.fib, vectors-pk.fib and
# vectors-tls.fib. python3 + `cryptography` are the ORACLE here and nowhere else.
import json, os, re, sys, hmac, hashlib
from collections import OrderedDict
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305
from cryptography.hazmat.primitives.asymmetric import x25519, ec, ed25519, padding, utils
from cryptography.hazmat.primitives import hashes, serialization as ser
from cryptography.hazmat.primitives.kdf.hkdf import HKDFExpand
from cryptography.exceptions import InvalidTag, InvalidSignature

SRC = os.environ["SRC"]
OUT = "lib/fib/crypto"
DER, PKCS8, SPKI, NOENC = ser.Encoding.DER, ser.PrivateFormat.PKCS8, ser.PublicFormat.SubjectPublicKeyInfo, ser.NoEncryption()

def fail(m): print("gen-crypto-vectors-pk: " + m, file=sys.stderr); sys.exit(1)
def wy(name): return json.load(open(f"{SRC}/{name}.json"))
def hx(b): return b.hex()

# ---- RFC text search: a value taken from an RFC must appear in it
_rfc = {}
def rfc_streams(n):
    if n not in _rfc:
        ls = [l.lower() for l in open(f"{SRC}/rfc{n}.txt").read().split("\n") if not re.match(r"^rfc \d+", l.lower()) and not re.search(r"\[page \d+\]", l) and "\f" not in l]
        a = "".join(re.sub(r"[^0-9a-f]", "", l) for l in ls)
        b = "".join(re.sub(r"[^0-9a-f]", "", re.sub(r"^\s*[0-9a-f]{3,4}:?\s{2,}", "", l)) for l in ls)
        dump = re.compile(r"^\s*\d{3}\s+([0-9a-f]{2}(?: [0-9a-f]{2})*)")
        c = "".join(m.group(1).replace(" ", "") for m in (dump.match(l) for l in ls) if m)
        _rfc[n] = (a, b, c)
    return _rfc[n]
def pub(n, h, what=""):
    if not any(h.lower() in x for x in rfc_streams(n)): fail(f"RFC {n} does not contain {what} {h[:40]}...")
    return h

# ---- selection: all of the first group, then up to `per` per distinct flag set, at most `cap`
def stratified(tests, per, cap):
    seen, out = {}, []
    for t in tests:
        k = tuple(sorted(t["flags"]))
        if seen.get(k, 0) < per and len(out) < cap:
            seen[k] = seen.get(k, 0) + 1; out.append(t)
    return out

def q(s): return '"' + s + '"'
def chunked(name, rows, doc, per=24):
    """A function `name` returning the vector list, built from functions of at most `per` rows each (ADR 0006: functions under 50 lines)."""
    parts = [rows[i:i + per] for i in range(0, len(rows), per)] or [[]]
    out = [doc]
    for i, part in enumerate(parts):
        out.append(f"(defun {name}-{i} :private () -> (Vec {rows_type[name]})\n  [" + "\n   ".join(part) + "])\n")
    out.append(f"(defun {name} () -> (Vec {rows_type[name]})\n  " + (f"({name}-0)" if len(parts) == 1 else _nest(name, len(parts))) + ")\n")
    return "\n".join(out)
rows_type = {}
def _nest(name, n):
    e = f"({name}-0)"
    for i in range(1, n): e = f"(into {e} ({name}-{i}))"
    return e

# ============================================================ AEAD
aead_ok, aead_bad = [], []
def aead_check(alg, key, nonce, aad, pt, ct, tag):
    c = AESGCM(key) if alg.startswith(":aes") else ChaCha20Poly1305(key)
    if c.encrypt(nonce, pt, aad) != ct + tag: fail(f"{alg} seal disagrees with python: key {hx(key)[:16]}")
def add_aead(alg, name, key, nonce, aad, pt, sealed_ok):
    aead_ok.append(f'(AeadVector {alg} {q(name)} {q(hx(key))} {q(hx(nonce))} {q(hx(aad))} {q(hx(pt))} {q(hx(sealed_ok))})')
def add_bad(alg, name, key, nonce, aad, sealed):
    c = AESGCM(key) if alg.startswith(":aes") else ChaCha20Poly1305(key)
    try: c.decrypt(nonce, sealed, aad); fail(f"python opens an invalid {alg} {name}")
    except InvalidTag: pass
    aead_bad.append(f'(AeadInvalid {alg} {q(name)} {q(hx(key))} {q(hx(nonce))} {q(hx(aad))} {q(hx(sealed))})')

# the GCM specification's test cases (McGrew and Viega): 2 (AES-128, one block), 4 (with AAD), 16 (AES-256 with AAD)
GCM = [
 (":aes-128-gcm", "gcm-spec-tc2", "00" * 16, "00" * 12, "", "00" * 16, "0388dace60b6a392f328c2b971b2fe78", "ab6e47d42cec13bdf53a67b21257bddf"),
 (":aes-128-gcm", "gcm-spec-tc4", "feffe9928665731c6d6a8f9467308308", "cafebabefacedbaddecaf888", "feedfacedeadbeeffeedfacedeadbeefabaddad2",
  "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39",
  "42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091", "5bc94fbc3221a5db94fae95ae7121a47"),
 (":aes-256-gcm", "gcm-spec-tc16", "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308", "cafebabefacedbaddecaf888", "feedfacedeadbeeffeedfacedeadbeefabaddad2",
  "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39",
  "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662", "76fc6ece0f4e1768cddf8853bb2d551b"),
]
for alg, name, k, n, a, p, c, t in GCM:
    k, n, a, p, c, t = (bytes.fromhex(x) for x in (k, n, a, p, c, t))
    aead_check(alg, k, n, a, p, c, t); add_aead(alg, name, k, n, a, p, c + t)

g = wy("aes_gcm")
for grp in g["testGroups"]:
    if grp["ivSize"] != 96 or grp["tagSize"] != 128 or grp["keySize"] not in (128, 256): continue
    alg = ":aes-128-gcm" if grp["keySize"] == 128 else ":aes-256-gcm"
    ts = grp["tests"]
    for t in [x for x in ts if x["result"] == "valid" and "Ktv" in x["flags"]] + stratified([x for x in ts if x["result"] == "valid" and "Ktv" not in x["flags"]], 2, 5):
        k, n, a, p, c, tg = (bytes.fromhex(t[f]) for f in ("key", "iv", "aad", "msg", "ct", "tag"))
        aead_check(alg, k, n, a, p, c, tg); add_aead(alg, f"wycheproof-aes-gcm-{t['tcId']}", k, n, a, p, c + tg)
    for t in stratified([x for x in ts if x["result"] == "invalid"], 3, 4):
        k, n, a, c, tg = (bytes.fromhex(t[f]) for f in ("key", "iv", "aad", "ct", "tag"))
        add_bad(alg, f"wycheproof-aes-gcm-{t['tcId']}", k, n, a, c + tg)

# ChaCha20-Poly1305: RFC 8439 2.8.2 (Wycheproof's "RFC 7539" case, its text checked here) and A.5 (read out of the RFC)
cc = wy("chacha20_poly1305")
for grp in cc["testGroups"]:
    if grp["ivSize"] != 96 or grp["tagSize"] != 128: continue
    ts = grp["tests"]
    for t in [x for x in ts if x["result"] == "valid" and "Ktv" in x["flags"]] + stratified([x for x in ts if x["result"] == "valid" and "Ktv" not in x["flags"]], 2, 5):
        k, n, a, p, c, tg = (bytes.fromhex(t[f]) for f in ("key", "iv", "aad", "msg", "ct", "tag"))
        if t["comment"] == "RFC 7539":
            for v in (t["key"], t["iv"][8:], t["iv"][:8], t["aad"], t["ct"], t["tag"]): pub(8439, v, "2.8.2")
        aead_check(":chacha20-poly1305", k, n, a, p, c, tg); add_aead(":chacha20-poly1305", f"wycheproof-chacha-{t['tcId']}", k, n, a, p, c + tg)
    for t in stratified([x for x in ts if x["result"] == "invalid"], 3, 8):
        k, n, a, c, tg = (bytes.fromhex(t[f]) for f in ("key", "iv", "aad", "ct", "tag"))
        add_bad(":chacha20-poly1305", f"wycheproof-chacha-{t['tcId']}", k, n, a, c + tg)

def rfc8439_a5():
    key, nonce, aad, tag = (pub(8439, v, "A.5") for v in ("1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0", "000000000102030405060708", "f33388860000000000004e91", "eead9d67890cbb22392336fea1851f38"))
    lines = open(f"{SRC}/rfc8439.txt").read().split("\n")
    i = next(j for j, l in enumerate(lines) if "64 a0 86 15 75 86 1a f4" in l)
    pub(8439, "1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0", "A.5 key")
    toks = []
    dump = re.compile(r"^\s*\d{3}\s+([0-9a-f]{2}(?: [0-9a-f]{2})*)")
    while len(toks) < 265:
        m = dump.match(lines[i]); i += 1
        if m: toks += m.group(1).split()
    ct = bytes.fromhex("".join(toks[:265]))
    k, n, a, tg = (bytes.fromhex(x) for x in (key, nonce, aad, tag))
    pt = ChaCha20Poly1305(k).decrypt(n, ct + tg, a)
    if not pt.startswith(b"Internet-Drafts are draft documents"): fail("A.5 plaintext is not the RFC's")
    add_aead(":chacha20-poly1305", "rfc8439-a5", k, n, a, pt, ct + tg)
rfc8439_a5()

# ============================================================ key agreement
def p8_x(raw): return x25519.X25519PrivateKey.from_private_bytes(raw).private_bytes(DER, PKCS8, NOENC)
def p8_ec(d, curve): return ec.derive_private_key(d, curve).private_bytes(DER, PKCS8, NOENC)
kex_ok, kex_bad, kex_iter = [], [], []
def x_exch(priv, peer):
    return x25519.X25519PrivateKey.from_private_bytes(priv).exchange(x25519.X25519PublicKey.from_public_bytes(peer))
def add_x(name, priv, peer, shared):
    pubk = x25519.X25519PrivateKey.from_private_bytes(priv).public_key().public_bytes(ser.Encoding.Raw, ser.PublicFormat.Raw)
    if x_exch(priv, peer) != shared: fail(f"X25519 {name}: python disagrees")
    kex_ok.append(f'(KexVector :x25519 {q(name)} {q(hx(p8_x(priv)))} {q(hx(pubk))} {q(hx(peer))} {q(hx(shared))})')
H = bytes.fromhex
for n, s, u, o in (("rfc7748-5.2-1", "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4", "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c", "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552"),
                   ("rfc7748-5.2-2", "4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d", "e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493", "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957")):
    for v in (s, u, o): pub(7748, v, n)
    add_x(n, H(s), H(u), H(o))
A = ("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a", "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
B = ("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb", "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
K = "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"
for v in A + B + (K,): pub(7748, v, "6.1")
add_x("rfc7748-6.1-alice", H(A[0]), H(B[1]), H(K)); add_x("rfc7748-6.1-bob", H(B[0]), H(A[1]), H(K))
k = u = bytes([9]) + bytes(31)
for i in range(1000):
    k, u = x_exch(k, u), k
    if i == 0: kex_iter.append(f'(KexIterated "rfc7748-5.2-1-iteration" 1 {q(pub(7748, hx(k), "iter1"))})')
kex_iter.append(f'(KexIterated "rfc7748-5.2-1000-iterations" 1000 {q(pub(7748, hx(k), "iter1000"))})')

xw = wy("x25519")
xt = [t for grp in xw["testGroups"] for t in grp["tests"]]
for t in stratified([t for t in xt if t["result"] == "valid"], 2, 14):
    add_x(f"wycheproof-x25519-{t['tcId']}", H(t["private"]), H(t["public"]), H(t["shared"]))
for t in stratified([t for t in xt if "ZeroSharedSecret" in t["flags"]], 1, 12):
    pr, pe = H(t["private"]), H(t["public"])
    try:
        r = x_exch(pr, pe)
        if r != bytes(32): fail(f"x25519 {t['tcId']}: a zero-flagged case is not zero in python")
    except ValueError: pass
    kex_bad.append(f'(KexInvalid :x25519 "wycheproof-x25519-zero-secret-{t["tcId"]}" {q(hx(p8_x(pr)))} {q(hx(pe))})')

def ecdh(alg, curve, nm, file, per_valid, per_bad, raw_points):
    w = wy(file)
    for grp in w["testGroups"]:
        for t in stratified([t for t in grp["tests"] if t["result"] == "valid" and not ("Compressed" in "".join(t["flags"]))], 1, per_valid):
            d = int(t["private"], 16)
            sk = ec.derive_private_key(d, curve())
            if raw_points: peer = H(t["public"])
            else: peer = ser.load_der_public_key(H(t["public"])).public_bytes(ser.Encoding.X962, ser.PublicFormat.UncompressedPoint)
            if len(peer) != (65 if nm == "p256" else 97): continue
            sh = sk.exchange(ec.ECDH(), ec.EllipticCurvePublicKey.from_encoded_point(curve(), peer))
            if sh != H(t["shared"]): fail(f"{file} {t['tcId']}: python disagrees")
            pk = sk.public_key().public_bytes(ser.Encoding.X962, ser.PublicFormat.UncompressedPoint)
            kex_ok.append(f'(KexVector {alg} "wycheproof-ecdh-{nm}-{t["tcId"]}" {q(hx(p8_ec(d, curve())))} {q(hx(pk))} {q(hx(peer))} {q(hx(sh))})')
        if not raw_points: continue
        for t in stratified([t for t in grp["tests"] if t["result"] == "invalid"], 1, per_bad):
            d = int(t["private"], 16)
            try: sk = ec.derive_private_key(d, curve())
            except ValueError: continue
            peer = H(t["public"])
            try: ec.EllipticCurvePublicKey.from_encoded_point(curve(), peer); fail(f"ecdh {t['tcId']}: python accepts an invalid point")
            except ValueError: pass
            kex_bad.append(f'(KexInvalid {alg} "wycheproof-ecdh-{nm}-{t["tcId"]}" {q(hx(p8_ec(d, curve())))} {q(hx(peer))})')
ecdh(":p-256", ec.SECP256R1, "p256", "ecdh_secp256r1_ecpoint", 8, 14, True)
ecdh(":p-384", ec.SECP384R1, "p384", "ecdh_secp384r1", 4, 0, False)

# ============================================================ signatures (verify)
sig_keys, sig_rows = [], []
def key_index(spki_hex):
    if spki_hex not in sig_keys: sig_keys.append(spki_hex)
    return sig_keys.index(spki_hex)
def pykey(spki_hex): return ser.load_der_public_key(H(spki_hex))
def py_verify(scheme, key, msg, sig):
    h = hashes.SHA256() if "256" in scheme else hashes.SHA384()
    if scheme.startswith("rsa-pss"): key.verify(sig, msg, padding.PSS(padding.MGF1(h), h.digest_size), h)
    elif scheme.startswith("rsa-pkcs1"): key.verify(sig, msg, padding.PKCS1v15(), h)
    elif scheme.startswith("ecdsa"): key.verify(sig, msg, ec.ECDSA(h))
    else: key.verify(sig, msg)
def add_sig(scheme, name, spki, msg, sig, valid):
    try: py_verify(scheme, pykey(spki), msg, sig); ok = True
    except (InvalidSignature, ValueError): ok = False
    if ok != valid: fail(f"{scheme} {name}: python says {ok}, the vector says {valid}")
    sig_rows.append(f'(SigVector :{scheme} {q(name)} {key_index(spki)} {q(hx(msg))} {q(hx(sig))} {"true" if valid else "false"})')
def wy_sigs(scheme, file, nvalid, per_bad, cap_bad, groups=3):
    for grp in wy(file)["testGroups"][:groups]:
        spki = grp["publicKeyDer"]
        ts = grp["tests"]
        for t in stratified([t for t in ts if t["result"] == "valid"], 2, nvalid): add_sig(scheme, f"wycheproof-{file}-{t['tcId']}", spki, H(t["msg"]), H(t["sig"]), True)
        for t in stratified([t for t in ts if t["result"] == "invalid"], per_bad, cap_bad): add_sig(scheme, f"wycheproof-{file}-{t['tcId']}", spki, H(t["msg"]), H(t["sig"]), False)
wy_sigs("ed25519", "ed25519", 5, 1, 24)
wy_sigs("ecdsa-p256-sha256", "ecdsa_secp256r1_sha256", 6, 1, 40)
wy_sigs("ecdsa-p384-sha384", "ecdsa_secp384r1_sha384", 4, 1, 24)
wy_sigs("rsa-pss-sha256", "rsa_pss_2048_sha256_mgf1_32", 4, 1, 14)
wy_sigs("rsa-pss-sha384", "rsa_pss_2048_sha384_mgf1_48", 3, 1, 10)
wy_sigs("rsa-pkcs1-sha256", "rsa_signature_2048_sha256", 4, 1, 16)
wy_sigs("rsa-pkcs1-sha384", "rsa_signature_2048_sha384", 3, 1, 10)

# RFC 8032 7.1 tests 1 and 2: public key, message and signature from the RFC; signing known-answer vectors too (Ed25519 is deterministic)
sign_rows = []
for nm, sk, pk, msg, sg in (("rfc8032-test1", "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60", "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", "",
                             "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
                            ("rfc8032-test2", "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb", "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c", "72",
                             "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")):
    for v in (sk, pk, sg): pub(8032, v, nm)
    k = ed25519.Ed25519PrivateKey.from_private_bytes(H(sk))
    if k.sign(H(msg)) != H(sg) or k.public_key().public_bytes(ser.Encoding.Raw, ser.PublicFormat.Raw) != H(pk): fail(f"{nm}: python disagrees with the RFC")
    spki = k.public_key().public_bytes(DER, SPKI).hex()
    add_sig("ed25519", nm, spki, H(msg), H(sg), True)
    sign_rows.append(f'(SignVector :ed25519 {q(nm)} {q(hx(k.private_bytes(DER, PKCS8, NOENC)))} {q(msg)} {q(sg)})')

# ============================================================ RFC 8448 key schedule
def rfc8448():
    text = open(f"{SRC}/rfc8448.txt").read().split("\n")
    ent, i = [], 0
    pat = re.compile(r"^\s+(PRK|salt|IKM|secret|info|key info|iv info|expanded|key expanded|iv expanded) \((\d+) octets\):\s+(.*)$")
    while i < len(text):
        m = pat.match(text[i])
        if m:
            h = m.group(3).split(); i += 1
            while i < len(text) and re.match(r"^\s{6,}([0-9a-f]{2}\s?)+\s*$", text[i]): h += text[i].split(); i += 1
            if len(h) != int(m.group(2)): fail(f"rfc8448 {m.group(1)}: {len(h)} bytes, the line says {m.group(2)}")
            ent.append((m.group(1), "".join(h)))
        else: i += 1
    ex, ext, seen = [], [], set()
    for j, (lab, h) in enumerate(ent):
        if lab == "PRK":
            for lab2, h2 in ent[j + 1:j + 5]:
                if lab2 in ("info", "key info", "iv info"): info = h2; kind = lab2; break
            else: continue
            exp = next(h3 for l3, h3 in ent[j + 1:j + 7] if l3.endswith("expanded"))
            key = (h, info, exp)
            if key in seen: continue
            seen.add(key)
            out = HKDFExpand(hashes.SHA256(), len(exp) // 2, H(info)).derive(H(h))
            if hx(out) != exp: fail("rfc8448: HKDF-Expand disagrees with python")
            ex.append(f'(ExpandVector "rfc8448-{len(ex) + 1}" {q(h)} {q(info)} {len(exp) // 2} {q(exp)})')
        if lab == "secret" and j >= 2 and ent[j - 1][0] == "IKM" and ent[j - 2][0] == "salt":
            salt, ikm = ent[j - 2][1], ent[j - 1][1]
            if (salt, ikm, h) in seen: continue
            seen.add((salt, ikm, h))
            if hx(hmac.new(H(salt), H(ikm), hashlib.sha256).digest()) != h: fail("rfc8448: HKDF-Extract disagrees with HMAC")
            ext.append(f'(ExtractVector "rfc8448-{len(ext) + 1}" {q(salt)} {q(ikm)} {q(h)})')
    return ex, ext
tls_ex, tls_ext = rfc8448()
# RFC 5869 appendix A.1 to A.3: HKDF-Expand alone over the PRK the RFC prints (SHA-256)
for nm, prk, info, ln, okm in (
  ("rfc5869-a1", "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5", "f0f1f2f3f4f5f6f7f8f9", 42,
   "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"),
  ("rfc5869-a3", "19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04", "", 42,
   "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8")):
    for v in (prk, okm): pub(5869, v, nm)
    if hx(HKDFExpand(hashes.SHA256(), ln, H(info)).derive(H(prk))) != okm: fail(nm + ": python disagrees")
    tls_ex.append(f'(ExpandVector "{nm}" {q(prk)} {q(info)} {ln} {q(okm)})')
# the early secret's salt is "0 (all zero octets)": the RFC prints no salt line, HKDF-Extract with a zero-length salt uses HashLen zeros
tls_ext.append(f'(ExtractVector "rfc8448-early-empty-salt" "" {q("00" * 32)} {q(hx(hmac.new(bytes(32), bytes(32), hashlib.sha256).digest()))})')
pub(8448, hx(hmac.new(bytes(32), bytes(32), hashlib.sha256).digest()), "early secret")

# ============================================================ write
def write(name, text):
    open(f"{OUT}/{name}", "w").write(text)
HDR = ";; {}: GENERATED by scripts/gen-crypto-vectors-pk.sh (do not edit): the published sources (RFC texts, Project Wycheproof at the commit named in\n" \
      ";; scripts/fetch-crypto-vectors.sh) are checksummed by specs/crypto-vectors.sha256, every RFC value is searched in its RFC, every vector is recomputed or\n" \
      ";; refused by python's `cryptography`, and the file is a SUBSET. Bytes are hex.\n"
rows_type.update({"aead-vectors": "AeadVector", "aead-invalid-vectors": "AeadInvalid", "kex-vectors": "KexVector", "kex-invalid-vectors": "KexInvalid",
                  "kex-iterated-vectors": "KexIterated", "sig-vectors": "SigVector", "sign-vectors": "SignVector", "expand-vectors": "ExpandVector", "extract-vectors": "ExtractVector"})
write("vectors-aead.fib", HDR.format("fib.crypto.vectors-aead") + "(ns fib.crypto.vectors-aead (:use fib.core fib.seq fib.coll))\n"
      ";; `sealed` is ciphertext || tag. AeadInvalid: `sealed` is a valid one with something changed (tag, ciphertext, length): open must be an error.\n"
      "(defstruct AeadVector (alg: keyword name: str key: str nonce: str aad: str pt: str sealed: str))\n(defstruct AeadInvalid (alg: keyword name: str key: str nonce: str aad: str sealed: str))\n\n"
      + chunked("aead-vectors", aead_ok, "") + "\n" + chunked("aead-invalid-vectors", aead_bad, ""))
write("vectors-pk.fib", HDR.format("fib.crypto.vectors-pk") + "(ns fib.crypto.vectors-pk (:use fib.core fib.seq fib.coll))\n"
      ";; KexVector: `private` is PKCS#8 DER, `public` the raw public key it has, `peer` the raw peer key, `shared` the secret. KexInvalid: agree must be an error.\n"
      ";; KexIterated: RFC 7748 5.2's loop (k = u = 9; k, u = X25519(k, u), k) after `n` rounds. SigVector: `key` indexes `sig-keys` (SPKI DER), `valid` is the verdict.\n"
      ";; SignVector: an Ed25519 key (PKCS#8 DER), a message and the signature RFC 8032 gives.\n"
      "(defstruct KexVector (alg: keyword name: str private: str public: str peer: str shared: str))\n(defstruct KexInvalid (alg: keyword name: str private: str peer: str))\n"
      "(defstruct KexIterated (name: str n: i64 expected: str))\n(defstruct SigVector (scheme: keyword name: str key: i64 msg: str sig: str valid: bool))\n"
      "(defstruct SignVector (scheme: keyword name: str private: str msg: str sig: str))\n\n"
      + chunked("kex-vectors", kex_ok, "") + "\n" + chunked("kex-invalid-vectors", kex_bad, "") + "\n" + chunked("kex-iterated-vectors", kex_iter, "") + "\n"
      + chunked("sign-vectors", sign_rows, "") + "\n" + chunked("sig-vectors", sig_rows, "", 20) + "\n"
      + "(defun sig-keys () -> (Vec str)\n  [" + "\n   ".join(q(k) for k in sig_keys) + "])\n")
write("vectors-tls.fib", HDR.format("fib.crypto.vectors-tls") + "(ns fib.crypto.vectors-tls (:use fib.core fib.seq fib.coll))\n"
      ";; The key schedule of RFC 8448 (TLS 1.3, SHA-256): ExpandVector is HKDF-Expand(prk, info, length) with the HkdfLabel bytes as `info`; ExtractVector is\n"
      ";; HKDF-Extract(salt, ikm) (= HMAC with the salt as key). They are what the next package (TLS) derives its traffic keys with.\n"
      "(defstruct ExpandVector (name: str prk: str info: str length: i64 expected: str))\n(defstruct ExtractVector (name: str salt: str ikm: str expected: str))\n\n"
      + chunked("expand-vectors", tls_ex, "") + "\n" + chunked("extract-vectors", tls_ext, ""))
print(f"aead {len(aead_ok)} valid {len(aead_bad)} invalid; kex {len(kex_ok)} valid {len(kex_bad)} invalid {len(kex_iter)} iterated; sigs {len(sig_rows)} "
      f"({sum('true)' in r for r in sig_rows)} valid) on {len(sig_keys)} keys; sign {len(sign_rows)}; tls expand {len(tls_ex)} extract {len(tls_ext)}")
