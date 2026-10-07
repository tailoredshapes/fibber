#!/usr/bin/env python3
"""Reads RFC 8448 (TLS 1.3 example traces) and writes specs/tls-trace-data.fib: the values the TLS client specs replay. Run by scripts/gen-tls-trace.sh, which has
checked the RFC text against specs/crypto-vectors.sha256. Every value is taken from the text by its label and its stated octet count; a value that is not found, or
whose bytes do not add up to the stated count, stops the run. Sections: 3 (simple 1-RTT, x25519, TLS_AES_128_GCM_SHA256) and 5 (HelloRetryRequest, x25519 then P-256)."""
import re, sys

text = open(sys.argv[1]).read().split("\n")
out = sys.argv[2]
HEX = re.compile(r"^[0-9a-f]{2}$")


def section(start, end):
    return text[start:end]


def find_line(lines, pattern, frm=0):
    rx = re.compile(pattern)
    for i in range(frm, len(lines)):
        if rx.search(lines[i]):
            return i
    raise SystemExit("not found: " + pattern)


def octets(lines, label, frm=0):
    """The bytes of the first `label (N octets):` at or after line `frm`; returns (hex, index after the block)."""
    rx = re.compile(r"^\s*" + re.escape(label) + r" \((\d+) octets\):(.*)$")
    for i in range(frm, len(lines)):
        m = rx.match(lines[i])
        if m:
            n = int(m.group(1))
            toks = m.group(2).split()
            j = i + 1
            while len(toks) < n:
                if j >= len(lines):
                    raise SystemExit("short block " + label)
                cand = lines[j].split()
                if cand and all(HEX.match(t) for t in cand) and lines[j].startswith("         "):
                    toks += cand
                j += 1
            if len(toks) != n or not all(HEX.match(t) for t in toks):
                raise SystemExit("bad block %s: %d of %d" % (label, len(toks), n))
            return "".join(toks), j
    raise SystemExit("octets not found: " + label)


def after(lines, anchor, label):
    i = find_line(lines, anchor)
    return octets(lines, label, i)[0]


def records(lines, label="complete record"):
    res, frm = [], 0
    while True:
        try:
            h, frm = octets(lines, label, frm)
        except SystemExit:
            return res
        res.append(h)


def trace(name, lines, keys):
    d = {}
    d["client-private-%s" % keys[0]] = after(lines, r"\{client\}  create an ephemeral .* key pair", "private key")
    d["server-private"] = None
    recs = records(lines)
    return d, recs


simple = section(151, 856)
hrr = section(1588, 2358)
fields = []


def emit(k, v):
    fields.append((k, v))


# ---- section 3
emit("s3-client-private", after(simple, r"\{client\}  create an ephemeral x25519 key pair", "private key"))
emit("s3-client-hello", after(simple, r"\{client\}  construct a ClientHello handshake message", "ClientHello"))
r = records(simple)
emit("s3-server-hello-record", r[1])
emit("s3-server-flight-record", r[2])
emit("s3-client-finished-record", r[3])
emit("s3-server-hello", after(simple, r"\{server\}  construct a ServerHello handshake message", "ServerHello"))
emit("s3-shared", after(simple, r"\{server\}  extract secret \"handshake\"", "IKM"))
emit("s3-handshake-secret", after(simple, r"\{server\}  extract secret \"handshake\"", "secret"))
emit("s3-c-hs", after(simple, r"\{server\}  derive secret \"tls13 c hs traffic\"", "expanded"))
emit("s3-s-hs", after(simple, r"\{server\}  derive secret \"tls13 s hs traffic\"", "expanded"))
emit("s3-master", after(simple, r"\{server\}  extract secret \"master\"", "secret"))
emit("s3-c-ap", after(simple, r"\{server\}  derive secret \"tls13 c ap traffic\"", "expanded"))
emit("s3-s-ap", after(simple, r"\{server\}  derive secret \"tls13 s ap traffic\"", "expanded"))
emit("s3-client-finished", after(simple, r"\{client\}  calculate finished \"tls13 finished\":", "finished"))
emit("s3-server-flight-payload", after(simple, r"\{server\}  send handshake record:", "payload") if False else "")
# the plaintext flight (EncryptedExtensions .. Finished), for the byte-exact comparison of what the client decrypts
i = find_line(simple, r"\{server\}  send handshake record:", find_line(simple, r"\{server\}  construct a Finished handshake message"))
fields[-1] = ("s3-server-flight-payload", octets(simple, "payload", i)[0])

# ---- section 5
emit("s5-client-private-x25519", after(hrr, r"\{client\}  create an ephemeral x25519 key pair", "private key"))
emit("s5-client-hello-1", after(hrr, r"\{client\}  construct a ClientHello handshake message", "ClientHello"))
r = records(hrr)
emit("s5-hrr-record", r[1])
emit("s5-client-hello-2-record", r[2])
emit("s5-server-hello-record", r[3])
emit("s5-server-flight-record", r[4])
emit("s5-client-finished-record", r[5])
emit("s5-client-private-p256", after(hrr, r"\{client\}  create an ephemeral P-256 key pair", "private key"))
j = find_line(hrr, r"\{client\}  construct a ClientHello handshake message", find_line(hrr, r"\{client\}  create an ephemeral P-256"))
emit("s5-client-hello-2", octets(hrr, "ClientHello", j)[0])
emit("s5-hrr", after(hrr, r"\{server\}  construct a ServerHello handshake message", "ServerHello"))
emit("s5-server-hello", after(hrr, r"\{server\}  construct a ServerHello handshake message", "ServerHello") if False else "")
k = find_line(hrr, r"\{server\}  construct a ServerHello handshake message", find_line(hrr, r"\{server\}  create an ephemeral P-256"))
fields[-1] = ("s5-server-hello", octets(hrr, "ServerHello", k)[0])
emit("s5-shared", after(hrr, r"\{server\}  extract secret \"handshake\"", "IKM"))
emit("s5-handshake-secret", after(hrr, r"\{server\}  extract secret \"handshake\"", "secret"))
emit("s5-c-hs", after(hrr, r"\{server\}  derive secret \"tls13 c hs traffic\"", "expanded"))
emit("s5-s-hs", after(hrr, r"\{server\}  derive secret \"tls13 s hs traffic\"", "expanded"))
emit("s5-master", after(hrr, r"\{server\}  extract secret \"master\"", "secret"))
emit("s5-c-ap", after(hrr, r"\{server\}  derive secret \"tls13 c ap traffic\"", "expanded"))
emit("s5-s-ap", after(hrr, r"\{server\}  derive secret \"tls13 s ap traffic\"", "expanded"))
emit("s5-client-finished", after(hrr, r"\{client\}  calculate finished \"tls13 finished\":", "finished"))

with open(out, "w") as f:
    f.write(";; GENERATED by scripts/gen-tls-trace.sh from RFC 8448 (checksummed by specs/crypto-vectors.sha256): do not edit. The traces of its section 3 (simple 1-RTT\n")
    f.write(";; handshake, X25519, TLS_AES_128_GCM_SHA256) and section 5 (HelloRetryRequest: X25519 offered, P-256 chosen). Bytes are hex.\n")
    f.write("(ns tls-trace-data (:use fib.core fib.seq fib.coll))\n")
    f.write("(defstruct Item (name: str hex: str))\n")
    for n, (k, v) in enumerate(fields):
        if v == "":
            raise SystemExit("empty " + k)
    f.write("(defun trace-items () -> (Vec Item)\n  [")
    f.write("\n   ".join('(Item "%s" "%s")' % (k, v) for k, v in fields))
    f.write("])\n")
    f.write("(defun trace-hex (name: str) -> str\n  (match (first (filter (fn (i: Item) (= (. i name) name)) (trace-items))) ((some i) (. i hex)) (nil (trap (str \"no trace item \" name)))))\n")
print("wrote %d items to %s" % (len(fields), out))
