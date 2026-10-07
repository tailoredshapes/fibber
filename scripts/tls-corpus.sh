#!/bin/bash
# Generates the certificate corpus of fib.tls with the openssl CLI (3.4 or newer: `x509 -not_before/-not_after`) into scratch, and writes specs/tls-fixtures.fib from it: every
# case is a chain (end-entity first), the trust anchors, the host, the clock and the typed reason the validator must give ("ok" for the accepted ones). The fixtures are COMMITTED
# (the gate is offline and needs no openssl); the sha256 of the generated file is recorded in specs/tls-fixtures.sha256, and the commands that made them are THIS script. Keys
# are generated fresh (the fixtures are random bytes): re-running it makes a different, equivalent corpus; the validity dates are fixed, and the specs' clock is NOW below.
#   scripts/tls-corpus.sh [WORKDIR]       (default ~/.cache/fibber-scratch/tools/tls-corpus)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
w=${1:-$HOME/.cache/fibber-scratch/tools/tls-corpus}
rm -rf "$w"; mkdir -p "$w"; cd "$w"
NB=20240101000000Z; NA=20440101000000Z; NOW=1790000000     # 2026-09-21T14:13:20Z
ecparam() { openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$1.key" 2>/dev/null; }
mkkey() { case $2 in ec) ecparam "$1";; rsa) openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$1.key" 2>/dev/null;;
  rsa1024) openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:1024 -out "$1.key" 2>/dev/null;; ed) openssl genpkey -algorithm ED25519 -out "$1.key";; esac; }
# cert NAME KEYKIND SUBJECT-CN ISSUER(name or SELF) EXTENSIONS [NB NA DIGEST]
cert() {
  local n=$1 kind=$2 cn=$3 iss=$4 ext=$5 nb=${6:-$NB} na=${7:-$NA} dg=${8:-sha256}
  [ -f "$n.key" ] || mkkey "$n" "$kind"
  printf "%b\n" "$ext" > "$n.ext"
  openssl req -new -key "$n.key" -subj "/CN=$cn" -out "$n.csr" 2>/dev/null
  if [ "$iss" = SELF ]; then
    openssl x509 -req -in "$n.csr" -signkey "$n.key" -out "$n.pem" -not_before $nb -not_after $na -extfile "$n.ext" -$dg 2>/dev/null
  else
    openssl x509 -req -in "$n.csr" -CA "$iss.pem" -CAkey "$iss.key" -set_serial $RANDOM$RANDOM -out "$n.pem" -not_before $nb -not_after $na -extfile "$n.ext" -$dg 2>/dev/null
  fi
  openssl x509 -in "$n.pem" -outform DER -out "$n.der"
}
CA="basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign"
LEAF="basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth"
leaf() { echo "$LEAF\nsubjectAltName=$1"; }
cert root ec "Fixture Root" SELF "$CA"
cert inter ec "Fixture Intermediate" root "$CA"
cert other-root ec "Other Root" SELF "$CA"
cert other-inter ec "Other Intermediate" other-root "$CA"
cert root-rsa rsa "Fixture RSA Root" SELF "$CA"
# --- leaves
cert leaf ec good.example inter "$(leaf DNS:good.example)"
cert leaf-rsa rsa good.example root-rsa "$(leaf DNS:good.example)"
cert leaf-ed ed good.example inter "$(leaf DNS:good.example)"
cert leaf-expired ec good.example inter "$(leaf DNS:good.example)" $NB 20250101000000Z
cert leaf-future ec good.example inter "$(leaf DNS:good.example)" 20350101000000Z $NA
cert leaf-self ec good.example SELF "$(leaf DNS:good.example)"
cert leaf-other ec good.example other-inter "$(leaf DNS:good.example)"
cert leaf-wild ec "*.example.com" inter "$(leaf 'DNS:*.example.com')"
cert leaf-wild2 ec "x" inter "$(leaf 'DNS:*.*.example.com')"
cert leaf-wildp ec "x" inter "$(leaf 'DNS:a*.example.com')"
cert leaf-ip ec 127.0.0.1 inter "$(leaf 'IP:127.0.0.1')"
cert leaf-ipdns ec 127.0.0.1 inter "$(leaf 'DNS:127.0.0.1')"
cert leaf-nosan ec good.example inter "$LEAF"
cert leaf-clientauth ec good.example inter "basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=clientAuth\nsubjectAltName=DNS:good.example"
cert leaf-kuenc ec good.example inter "basicConstraints=CA:FALSE\nkeyUsage=critical,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:good.example"
cert leaf-sha1 rsa good.example root-rsa "$(leaf DNS:good.example)" $NB $NA sha1
cert leaf-rsa1024 rsa1024 good.example root "$(leaf DNS:good.example)"
cert leaf-critical ec good.example inter "$(leaf DNS:good.example)\n1.2.3.4=critical,ASN1:NULL:"
cert leaf-sub ec a.bad.example inter "$(leaf DNS:a.bad.example)"
cert leaf-ip2 ec 10.1.2.3 inter "$(leaf 'IP:10.1.2.3')"
# --- intermediates with a defect
cert inter-nc-excl ec "NC Excluding" root "$CA\nnameConstraints=critical,excluded;DNS:bad.example"
cert inter-nc-perm ec "NC Permitting" root "$CA\nnameConstraints=critical,permitted;DNS:ok.example"
cert inter-nc-ip ec "NC IP" root "$CA\nnameConstraints=critical,excluded;IP:10.0.0.0/255.0.0.0"
cert inter-noks ec "No keyCertSign" root "basicConstraints=critical,CA:TRUE\nkeyUsage=critical,digitalSignature"
cert inter-notca ec "Not a CA" root "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyCertSign"
cert root-pl0 ec "PathLen 0 Root" SELF "basicConstraints=critical,CA:TRUE,pathlen:0\nkeyUsage=critical,keyCertSign"
cert inter-a ec "Chain A" root-pl0 "$CA"
cert inter-b ec "Chain B" inter-a "$CA"
cert root-pl1 ec "PathLen 1 Root" SELF "basicConstraints=critical,CA:TRUE,pathlen:1\nkeyUsage=critical,keyCertSign"
cert inter-c ec "Chain C" root-pl1 "$CA"
cert root-expired ec "Expired Root" SELF "$CA" $NB 20250101000000Z
cert inter-e ec "Under Expired Root" root-expired "$CA"
for k in leaf-nc-excl leaf-nc-perm leaf-nc-ip leaf-noks leaf-notca leaf-b leaf-c leaf-e; do :; done
cert leaf-nc-excl ec a.bad.example inter-nc-excl "$(leaf DNS:a.bad.example)"
cert leaf-nc-perm ec good.example inter-nc-perm "$(leaf DNS:good.example)"
cert leaf-nc-ip ec 10.1.2.3 inter-nc-ip "$(leaf 'IP:10.1.2.3')"
cert leaf-noks ec good.example inter-noks "$(leaf DNS:good.example)"
cert leaf-notca ec good.example inter-notca "$(leaf DNS:good.example)"
cert leaf-b ec good.example inter-b "$(leaf DNS:good.example)"
cert leaf-c ec good.example inter-c "$(leaf DNS:good.example)"
cert leaf-e ec good.example inter-e "$(leaf DNS:good.example)"
cert leaf-nc-ok ec good.example inter-nc-excl "$(leaf DNS:good.example)"
python3 - "$w" "$NOW" "$here/../specs/tls-fixtures.fib" <<'EOF'
import sys, subprocess
w, now, out = sys.argv[1], sys.argv[2], sys.argv[3]
def der(n): return open("%s/%s.der" % (w, n), "rb").read().hex()
# name, host, chain (leaf first), anchors, expected reason, needs a real verifier (a signature is meant to be wrong)
C = [
 ("ok-ec-chain", "good.example", ["leaf", "inter"], ["root"], "ok", False),
 ("ok-rsa-leaf", "good.example", ["leaf-rsa"], ["root-rsa"], "ok", False),
 ("ok-ed25519-leaf", "good.example", ["leaf-ed", "inter"], ["root"], "ok", False),
 ("ok-root-sent-too", "good.example", ["leaf", "inter", "root"], ["root"], "ok", False),
 ("ok-case-insensitive-host", "GOOD.Example", ["leaf", "inter"], ["root"], "ok", False),
 ("ok-trailing-dot-host", "good.example.", ["leaf", "inter"], ["root"], "ok", False),
 ("ok-wildcard-one-label", "a.example.com", ["leaf-wild", "inter"], ["root"], "ok", False),
 ("ok-ip-san", "127.0.0.1", ["leaf-ip", "inter"], ["root"], "ok", False),
 ("ok-self-signed-leaf-is-the-anchor", "good.example", ["leaf-self"], ["leaf-self"], "ok", False),
 ("ok-pathlen-1-with-one-intermediate", "good.example", ["leaf-c", "inter-c"], ["root-pl1"], "ok", False),
 ("ok-name-constraint-not-hit", "good.example", ["leaf-nc-ok", "inter-nc-excl"], ["root"], "ok", False),
 ("expired-leaf", "good.example", ["leaf-expired", "inter"], ["root"], "expired", False),
 ("not-yet-valid-leaf", "good.example", ["leaf-future", "inter"], ["root"], "not-yet-valid", False),
 ("expired-anchor", "good.example", ["leaf-e", "inter-e"], ["root-expired"], "expired", False),
 ("wrong-host", "other.example", ["leaf", "inter"], ["root"], "hostname-mismatch", False),
 ("self-signed-not-trusted", "good.example", ["leaf-self"], ["root"], "untrusted-root", False),
 ("untrusted-root", "good.example", ["leaf-other", "other-inter"], ["root"], "untrusted-root", False),
 ("intermediate-missing", "good.example", ["leaf"], ["root"], "untrusted-root", False),
 ("path-length-violated", "good.example", ["leaf-b", "inter-b", "inter-a"], ["root-pl0"], "path-length", False),
 ("name-constraint-excluded", "a.bad.example", ["leaf-nc-excl", "inter-nc-excl"], ["root"], "name-constraint", False),
 ("name-constraint-not-permitted", "good.example", ["leaf-nc-perm", "inter-nc-perm"], ["root"], "name-constraint", False),
 ("name-constraint-ip-excluded", "10.1.2.3", ["leaf-nc-ip", "inter-nc-ip"], ["root"], "name-constraint", False),
 ("intermediate-lacks-keycertsign", "good.example", ["leaf-noks", "inter-noks"], ["root"], "key-usage", False),
 ("intermediate-is-not-a-ca", "good.example", ["leaf-notca", "inter-notca"], ["root"], "bad-chain", False),
 ("leaf-eku-clientauth-only", "good.example", ["leaf-clientauth", "inter"], ["root"], "key-usage", False),
 ("leaf-keyusage-without-digitalsignature", "good.example", ["leaf-kuenc", "inter"], ["root"], "key-usage", False),
 ("sha1-signature", "good.example", ["leaf-sha1"], ["root-rsa"], "weak-algorithm", False),
 ("rsa-1024-key", "good.example", ["leaf-rsa1024", "root"], ["root"], "weak-algorithm", False),
 ("unknown-critical-extension", "good.example", ["leaf-critical", "inter"], ["root"], "unknown-critical-extension", False),
 ("wildcard-two-labels-deep", "a.b.example.com", ["leaf-wild", "inter"], ["root"], "hostname-mismatch", False),
 ("wildcard-star-star", "a.b.example.com", ["leaf-wild2", "inter"], ["root"], "hostname-mismatch", False),
 ("wildcard-partial-label", "ab.example.com", ["leaf-wildp", "inter"], ["root"], "hostname-mismatch", False),
 ("wildcard-does-not-match-the-bare-domain", "example.com", ["leaf-wild", "inter"], ["root"], "hostname-mismatch", False),
 ("ip-host-does-not-match-a-dns-san", "127.0.0.1", ["leaf-ipdns", "inter"], ["root"], "hostname-mismatch", False),
 ("no-san-the-common-name-is-not-used", "good.example", ["leaf-nosan", "inter"], ["root"], "hostname-mismatch", False),
]
def flip(h, from_end):  # change one byte of the signature value (the last bytes of the DER) or of the tbs
    b = bytearray(bytes.fromhex(h)); i = len(b) - from_end; b[i] ^= 0x01; return b.hex()
def tbs_flip(h):
    b = bytearray(bytes.fromhex(h)); i = 120; b[i] ^= 0x01; return b.hex()
rows = []
for name, host, chain, anchors, exp, real in C:
    rows.append((name, host, [der(c) for c in chain], [der(a) for a in anchors], exp, real))
rows.append(("tampered-leaf-signature", "good.example", [flip(der("leaf"), 5), der("inter")], [der("root")], "bad-signature", True))
rows.append(("tampered-intermediate-signature", "good.example", [der("leaf"), flip(der("inter"), 5)], [der("root")], "bad-signature", True))
rows.append(("tampered-leaf-contents", "good.example", [tbs_flip(der("leaf")), der("inter")], [der("root")], "bad-signature", True))
with open(out, "w") as f:
    f.write(";; GENERATED by scripts/tls-corpus.sh (do not edit): certificates made with the openssl CLI, every one with fixed validity dates; the clock of the specs is `fixture-now`.\n")
    f.write(";; A fixture is a chain (the DER of each certificate in hex, end-entity first), the trust anchors, the host and the reason the validator must give. `real` marks the\n")
    f.write(";; ones whose signature is wrong on purpose: only a provider that verifies signatures can tell (the gate's stub accepts every signature; tls-specs/ runs these).\n")
    f.write("(ns tls-fixtures (:use fib.core fib.seq fib.coll))\n")
    f.write("(defstruct Fx (name: str host: str chain: (Vec str) anchors: (Vec str) expect: str real: bool))\n")
    f.write("(defun fixture-now () -> i64 %s)\n" % now)
    parts = [rows[i:i+12] for i in range(0, len(rows), 12)]
    for n, p in enumerate(parts):
        f.write("(defun fixtures-%d :private () -> (Vec Fx)\n  [" % n)
        f.write("\n   ".join('(Fx "%s" "%s" [%s] [%s] "%s" %s)' % (nm, h, " ".join('"%s"' % x for x in ch), " ".join('"%s"' % x for x in an), e, "true" if r else "false") for nm, h, ch, an, e, r in p))
        f.write("])\n")
    expr = "(fixtures-0)"
    for n in range(1, len(parts)): expr = "(concat %s (fixtures-%d))" % (expr, n)
    f.write("(defun fixtures () -> (Vec Fx) (vec %s))\n" % expr)
print("wrote %d fixtures" % len(rows))
EOF
(cd "$here/.." && sha256sum specs/tls-fixtures.fib > specs/tls-fixtures.sha256)
