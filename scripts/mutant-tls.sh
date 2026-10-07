#!/bin/bash
# scripts/mutant-tls.sh: the mutation review of fib.tls. Copies lib/fib/tls to a scratch directory, applies ONE planted fault to the copy, and runs the specs that must notice it
# (the scratch copy is first on the module path): at least one scenario must FAIL (or the spec must not compile or must trap). A fault under which every scenario passes means the specs
# have a hole. Nothing in the tree changes. The faults that need real cryptography (signatures, AEAD, key schedule) are judged by the driver specs (tls-specs/, with the OpenSSL
# driver); the others by the gate's specs.
#   FIBC=<stage-2 fibc> DRIVER=<fib-crypto-openssl/src> scripts/mutant-tls.sh [MUTANT..]        (default: all)
#   skip-hostname        the host name is not checked against the subjectAltName            skip-chain-sigs      the signatures of the chain are not verified
#   accept-expired       an expired certificate is accepted                                  ignore-pathlen       pathLenConstraint is ignored
#   wrong-cv-context     the CertificateVerify context string is the client's                skip-finished        the server's Finished is not verified
#   nonce-no-seq         the record nonce omits the sequence number                          accept-sentinel      the TLS 1.2 downgrade sentinel is accepted
#   accept-tls12         a ServerHello without TLS 1.3 is accepted                            wrong-label          the HKDF label prefix is "tls12 "
#   truncation-as-eof    a transport that ends without close_notify reads as a clean end     wildcard-multi       a wildcard matches several labels
#   accept-ca-false      an intermediate that is not a CA is accepted                        accept-critical      an unknown critical extension is accepted
#   hrr-transcript       the transcript after a HelloRetryRequest omits the HRR message      no-key-update        KeyUpdate does not change the read keys
#   accept-oversize      a record over 2^14+256 bytes is accepted                            accept-sha1          a SHA-1 signature is accepted
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC to a stage-2 fibc of this tree}
DRIVER=${DRIVER:?set DRIVER to the src directory of fib-crypto-openssl}
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-tls}
MUTANTS=("$@")
ALL=(skip-hostname skip-chain-sigs accept-expired ignore-pathlen wrong-cv-context skip-finished nonce-no-seq accept-sentinel accept-tls12 wrong-label truncation-as-eof wildcard-multi accept-ca-false accept-critical hrr-transcript no-key-update accept-oversize accept-sha1)
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=("${ALL[@]}")
ulimit -v 16000000 || true
LIBCRYPTO=$(ldconfig -p | awk '/libcrypto\.so\.3 .*(x86-64|AArch64)/ { print $NF; exit }')

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error
  cp "$1" "$1.orig"; perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-tls: pattern not found: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
apply() { # apply DIR MUTANT -> echoes the specs that must kill it
  local t=$1/fib/tls
  case $2 in
    skip-hostname)    sub $t/chain.fib 's/\(cond \(host-matches\? host \(\. ex san-dns\) \(\. ex san-ip\)\) \(Ok \(\)\)/(cond (or true (host-matches? host (. ex san-dns) (. ex san-ip))) (Ok ())/'; echo specs/tls-chain-spec.fib ;;
    skip-chain-sigs)  sub $t/chain.fib 's/\(c\/verify-signature p \(\. issuer spki\) s \(\. cert tbs\) \(\. cert signature\)\)/(Ok ())/'; echo tls-specs/tls-chain-real-spec.fib ;;
    accept-expired)   sub $t/chain.fib 's/\(> now \(\. x not-after\)\) \(Err/(> now (+ (. x not-after) 99999999999)) (Err/'; echo specs/tls-chain-spec.fib ;;
    ignore-pathlen)   sub $t/chain.fib 's/\(and \(>= \(\. ex path-len\) 0\) \(> below \(\. ex path-len\)\)\)/(and false (> below (. ex path-len)))/'; echo specs/tls-chain-spec.fib ;;
    wrong-cv-context) sub $t/keys.fib 's/\(if server "TLS 1.3, server CertificateVerify"/(if (not server) "TLS 1.3, server CertificateVerify"/'; echo tls-specs/tls-e2e-spec.fib ;;
    skip-finished)    sub $t/hs-flight.fib 's/\(if \(c\/ct= p expected \(\. m body\)\)/(if true/'; echo tls-specs/tls-e2e-spec.fib ;;
    nonce-no-seq)     sub $t/keys.fib 's/\(if \(< i 4\) 0 \(bit-and \(shr seq \(\* 8 \(- 11 i\)\)\) 255\)\)/0/'; echo "tls-specs/tls-rfc8448-spec.fib tls-specs/tls-e2e-spec.fib" ;;
    accept-sentinel)  sub $t/hs.fib 's/\(or \(bytes-eq\? tail downgrade-12\) \(bytes-eq\? tail downgrade-11\)\)/false/'; echo specs/tls-msg-spec.fib ;;
    accept-tls12)     sub $t/hs.fib 's/\(not \(= \(\. sh version\) 772\)\) \(Err \(hs-fail 70 "the server did not select/false (Err (hs-fail 70 "the server did not select/'; echo specs/tls-msg-spec.fib ;;
    wrong-label)      sub $t/keys.fib 's/\(str "tls13 " label\)/(str "tls12 " label)/'; echo tls-specs/tls-rfc8448-spec.fib ;;
    truncation-as-eof) sub $t/conn.fib 's/\(match \(read-app p c n\) \(\(Err e\) \(Err \(fail p c e\)\)\) \(\(Ok b\) \(Ok b\)\)\)/(match (read-app p c n) ((Err (Truncated)) (Ok (empty-bytes))) ((Err e) (Err (fail p c e))) ((Ok b) (Ok b)))/'; echo tls-specs/tls-e2e-spec.fib ;;
    wildcard-multi)   sub $t/names.fib 's/\(= \(count hl\) \(count pl\)\) \(= \(vec \(drop 1 hl\)\) \(vec \(drop 1 pl\)\)\)/(>= (count hl) (count pl)) (= (vec (drop (- (count hl) (- (count pl) 1)) hl)) (vec (drop 1 pl)))/'; echo specs/tls-names-spec.fib ;;
    accept-ca-false)  sub $t/chain.fib 's/\(and \(not anchor\?\) \(not \(and \(\. ex has-bc\) \(\. ex is-ca\)\)\)\)/false/'; echo specs/tls-chain-spec.fib ;;
    accept-critical)  sub $t/x509-ext.fib 's/critical \(Err \(bad-cert CertUnknownCritical \(str "unknown critical extension " oid\)\)\)/critical (Ok e)/'; echo specs/tls-chain-spec.fib ;;
    hrr-transcript)   sub $t/hs.fib 's/h1 raw \(\. st2 hello\)\]/h1 (. st2 hello)]/'; echo tls-specs/tls-rfc8448-spec.fib ;;
    no-key-update)    sub $t/conn.fib 's/\(do \(set! \(\. c rtr\) n\) \(set-read-keys \(\. c rec\) \(\. \(\. c suite\) aead\) n\) \(Ok \(\)\)\)/(do (set! (. c rtr) n) (Ok ()))/'; echo tls-specs/tls-e2e-spec.fib ;;
    accept-oversize)  sub $t/record.fib 's/\(> len \(if \(some\? enc\) max-cipher max-plain\)\)/(> len 99999999)/'; echo "specs/tls-record-spec.fib tls-specs/tls-e2e-spec.fib" ;;
    accept-sha1)      sub $t/chain.fib 's/\(weak-scheme\? s\) \(Err/false (Err/'; echo specs/tls-chain-spec.fib ;;
    *) echo "mutant-tls: unknown mutant $2" >&2; exit 2 ;;
  esac
}
survived=0
for m in "${MUTANTS[@]}"; do
  d=$OUT/$m; rm -rf "$d"; mkdir -p "$d/fib"; cp -r "$R/lib/fib/tls" "$d/fib/tls"; cp "$R/lib/fib/tls.fib" "$d/fib/tls.fib"
  specs=$(apply "$d" "$m") || exit 2
  killed=no; detail=""
  for s in $specs; do
    out=$(cd "$R" && FIB_LIB="$d:$R/lib:$R/specs:$R/tls-specs:$DRIVER" LD_PRELOAD="$LIBCRYPTO" "$FIBC" test "$s" --seed 1 2>&1)
    if [ $? -ne 0 ] || printf '%s' "$out" | grep -qE '^\s+(FAIL|TRAP)|does not compile'; then killed=yes; detail="$s: $(printf '%s' "$out" | grep -E '^\s+(FAIL|TRAP)|does not compile' | head -1 | sed 's/^ *//' | cut -c1-90)"; break; fi
  done
  if [ $killed = yes ]; then echo "KILLED   $m  ($detail)"; else echo "SURVIVED $m  (specs: $specs)"; survived=1; fi
done
exit $survived
