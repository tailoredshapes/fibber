#!/bin/bash
# scripts/mutant-dns.sh: the mutation review of fib.dns (HTTP-2). Copies lib/fib/dns to a scratch directory, applies ONE planted fault to the copy and runs the
# specs that are meant to catch it (the scratch copy is first on the module path): a spec must FAIL, TRAP, time out or not compile. A mutant under which the
# specs still pass is a hole in the specs. The unmutated copy must pass first. Nothing in the tree changes.
#
# usage: FIBC=/path/to/fibc scripts/mutant-dns.sh [MUTANT..]        (default: all)
#   accept-wrong-id     the response ID is not compared
#   no-question-check   the question of a response is not compared
#   no-response-bit     a query (QR=0) is accepted as an answer
#   tc-ignored          a truncated UDP answer is used as it is, TCP is never tried
#   no-pointer-bound    a compression pointer may point forward (the jump cap still stops a loop)
#   pointer-loop        no pointer bound and no jump cap: a pointer loop never ends (the timeout kills it)
#   trailing-ok         bytes after the last record are accepted
#   no-count-check      the section counts are not checked against the bytes before reading
#   ttl-top-bit         a TTL with the top bit set is taken as a huge number
#   cache-ignores-ttl   a cache entry never expires
#   no-negative-cache   NXDOMAIN and NODATA are not cached
#   cache-unbounded     the cache never evicts
#   no-chain-limit      a CNAME chain may be any length
#   no-loop-check       a CNAME seen twice is not noticed
#   off-chain-accepted  records for names off the CNAME chain are used
#   constant-id         the query ID is always the same number
#   search-order        ndots decides the wrong way round
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC to a fibc (the seed is fine: this is library code)}
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-dns}
MUTANTS=("$@")
ALL=(accept-wrong-id no-question-check no-response-bit tc-ignored no-pointer-bound pointer-loop trailing-ok no-count-check ttl-top-bit cache-ignores-ttl no-negative-cache cache-unbounded no-chain-limit no-loop-check off-chain-accepted constant-id search-order)
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=("${ALL[@]}")
ulimit -v 16000000 || true
rm -rf "$OUT"; mkdir -p "$OUT"

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error
  cp "$1" "$1.orig"; perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-dns: pattern not found in $1: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
apply() {
  local d=$1/fib/dns
  case $2 in
    accept-wrong-id)    sub "$d/exchange.fib" 's/\(= \(\. m id\) id\)/true/' ;;
    no-question-check)  sub "$d/exchange.fib" 's/\(same-question\? q \(nth \(\. m questions\) 0\)\)/true/' ;;
    no-response-bit)    sub "$d/exchange.fib" 's/\(and \(response\? m\) \(= \(\. m id\) id\)/(and true (= (. m id) id)/' ;;
    tc-ignored)         sub "$d/exchange.fib" 's/\(if \(truncated\? m\) \(tcp-once server q id query deadline\) \(Ok m\)\)/(Ok m)/' ;;
    no-pointer-bound)   sub "$d/decode.fib" 's/\(>= target \(\. w bound\)\)/false/' ;;
    pointer-loop)       sub "$d/decode.fib" 's/\(>= target \(\. w bound\)\)/false/; s/\(>= \(\. w jumps\) max-jumps\)/false/' ;;
    trailing-ok)        sub "$d/decode.fib" 's/\(!= \(\. x snd\) \(array-len b\)\)/false/' ;;
    no-count-check)     sub "$d/decode.fib" 's/\(> \(\+ \(\* qd 5\) \(\* \(\+ an ns ar\) 11\)\) \(- size 12\)\)/false/' ;;
    ttl-top-bit)        sub "$d/decode.fib" 's/\(if \(>= raw 2147483648\) 0 raw\)/raw/' ;;
    cache-ignores-ttl)  sub "$d/cache.fib" 's/\(<= \(\. e expires\) now\)/false/' ;;
    no-negative-cache)  sub "$d/cache.fib" 's/\(put-entry c key \[\] kind ttl-s max-negative-ttl-s now\)/false/' ;;
    cache-unbounded)    sub "$d/cache.fib" 's/\(if \(< \(count m\) max\) m/(if true m/' ;;
    no-chain-limit)     sub "$d/answer.fib" 's/\(>= hops max-chain\)/false/' ;;
    no-loop-check)      sub "$d/answer.fib" 's/\(includes\? next seen\)/false/' ;;
    off-chain-accepted) sub "$d/answer.fib" 's/\(vec \(filter \(fn \(r: Rr\) \(= \(lower \(\. r name\)\) name\)\) rrs\)\)/rrs/' ;;
    constant-id)        sub "$d/exchange.fib" 's/\(Ok \(\+ \(\* 256 \(zext i64 \(array-get b 0\)\)\) \(zext i64 \(array-get b 1\)\)\)\)/(Ok 4660)/' ;;
    search-order)       sub "$d/resolve.fib" 's/\(if \(>= \(dots name\) \(\. c ndots\)\)/(if (< (dots name) (. c ndots))/' ;;
    *) echo "unknown mutant $2" >&2; exit 2 ;;
  esac
}

mkdir -p "$OUT/base/fib"; cp -r "$R/lib/fib/dns" "$R/lib/fib/dns.fib" "$OUT/base/fib/"
SPECS=(dns-wire-spec dns-resolver-spec)
for spec in "${SPECS[@]}"; do
  "$FIBC" test "$R/specs/$spec.fib" -I "$OUT/base" -I "$R/lib" > "$OUT/base-$spec.log" 2>&1 || { echo "mutant-dns: the unmutated copy fails $spec: see $OUT/base-$spec.log" >&2; exit 2; }
  echo "unmutated $spec: $(grep '^total:' "$OUT/base-$spec.log")"
done
survived=0
for m in "${MUTANTS[@]}"; do
  d=$OUT/$m; mkdir -p "$d/fib"; cp -r "$R/lib/fib/dns" "$R/lib/fib/dns.fib" "$d/fib/"
  apply "$d" "$m"
  killed_by=""
  for spec in "${SPECS[@]}"; do
    if ! timeout 240 "$FIBC" test "$R/specs/$spec.fib" -I "$d" -I "$R/lib" --timeout 60 > "$d-$spec.log" 2>&1; then
      killed_by="$killed_by $spec($(grep -c '^  FAIL\|^  TRAP\|^  TIMEOUT\|ERROR' "$d-$spec.log"))"
    fi
  done
  if [ -z "$killed_by" ]; then echo "SURVIVED  $m"; survived=1; else echo "killed    $m  by$killed_by"; fi
done
exit $survived
