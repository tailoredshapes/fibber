#!/bin/bash
# scripts/mutant-http.sh: the mutation review of fib.http (HTTP-1). Copies lib/fib/http to a scratch directory, applies ONE planted fault to the
# copy and runs the spec that is meant to catch it (the scratch copy is first on the module path): the spec must FAIL, TRAP or not compile.
# A mutant under which the spec still passes is a hole in the specs. The unmutated copy must pass every spec first. Nothing in the tree changes.
#
# usage: FIBC=/path/to/fibc scripts/mutant-http.sh [MUTANT..]        (default: all)
#   fixed-overread     message layer  a fixed-length body reads one byte more than its length (framing off by one)
#   chunk-terminator   message layer  the CRLF after a chunk is not checked
#   cl-te              message layer  Content-Length together with Transfer-Encoding is accepted (request)
#   cl-te-response     message layer  the same for a response
#   bare-lf            message layer  a bare LF ends a line
#   chunk-overflow     message layer  the chunk-size overflow check is gone
#   header-count       message layer  the header count limit is off by one (reader and parser)
#   head-after-limit   message layer  the parser's header limit is off by one
#   head-sends-body    message layer  a HEAD reply carries the body
#   keepalive-ignores-close  server   Connection: close of the request is ignored
#   slowloris          server         the header deadline is a hundred times longer
#   no-drain           server         stop does not wait for requests in flight
#   redirect-301       client         301 keeps POST (method rewriting dropped)
#   redirect-307       client         307 rewrites POST to GET
#   downgrade          client         https -> http redirects are followed
#   pool-leak          client         the pool never evicts over max-idle-per-host
#   no-deadline        client         no deadline is armed before an exchange
#   no-host            client         the Host header is not sent
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC to a fibc (the seed is fine: this is library code)}
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-http}
MUTANTS=("$@")
ALL=(fixed-overread chunk-terminator cl-te cl-te-response bare-lf chunk-overflow header-count head-after-limit head-sends-body keepalive-ignores-close slowloris no-drain redirect-301 redirect-307 downgrade pool-leak no-deadline no-host)
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=("${ALL[@]}")
ulimit -v 16000000 || true
rm -rf "$OUT"; mkdir -p "$OUT"

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error
  cp "$1" "$1.orig"; perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-http: pattern not found in $1: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
spec_for() { # which spec must kill the mutant
  case $1 in
    fixed-overread|chunk-terminator|cl-te|cl-te-response|bare-lf|chunk-overflow|header-count|head-after-limit) echo http-message-spec ;;
    head-sends-body|keepalive-ignores-close|slowloris|no-drain) echo http-server-spec ;;
    *) echo http-client-spec ;;
  esac
}
apply() {
  local h=$1/fib/http
  case $2 in
    fixed-overread)    sub "$h/body.fib" 's/\(read-some r \(min piece \@left\)\)/(read-some r (min piece (+ \@left 1)))/' ;;
    chunk-terminator)  sub "$h/body.fib" 's/\(if \(and \(= \(array-get ending 0\) 13i8\) \(= \(array-get ending 1\) 10i8\)\) \(Ok bytes\)/(if true (Ok bytes)/' ;;
    cl-te)             sub "$h/wire.fib" 's/\(cond \(and \(> \(count lengths\) 0\) \(> \(count encodings\) 0\)\) \(Err \(ProtocolError "ambiguous HTTP body framing"\)\)/(cond false (Err (ProtocolError "ambiguous HTTP body framing"))/' ;;
    cl-te-response)    sub "$h/message.fib" 's/\(and \(> \(count lengths\) 0\) \(> \(count encodings\) 0\)\) \(Err \(ProtocolError "ambiguous HTTP body framing"\)\)/false (Err (ProtocolError "ambiguous HTTP body framing"))/' ;;
    bare-lf)           sub "$h/reader.fib" 's/\(= c 10\) \(if \(= prev 13\) \(Ok \(some \@out\)\) \(Err \(ProtocolError "bare LF in an HTTP line"\)\)\)/(= c 10) (Ok (some \@out))/' ;;
    chunk-overflow)    sub "$h/wire.fib" 's/\(or \(> d limit\) \(> n \(quot \(- limit d\) base\)\)\)/(> d limit)/' ;;
    header-count)      sub "$h/reader.fib" 's/\(>= \(count parts\) max-lines\)/(> (count parts) max-lines)/' ;;
    head-after-limit)  sub "$h/wire.fib" 's/\(if \(> \(- end start\) max\) \(Err \(LimitError "too many HTTP headers"\)\)/(if (> (- end start) (+ max 1)) (Err (LimitError "too many HTTP headers"))/' ;;
    head-sends-body)   sub "$h/wire.fib" 's/\(if \(or \(= method "HEAD"\) \(= status 204\)/(if (or (= status 204)/' ;;
    keepalive-ignores-close) sub "$h/wire.fib" 's/\(and \(not \(header-token\? \(\. head headers\) "connection" "close"\)\)/(and true/' ;;
    slowloris)         sub "$h/server/connection.fib" 's/\(ms \(\. o header-timeout-ms\)\)/(ms (* 100 (. o header-timeout-ms)))/' ;;
    no-drain)          sub "$h/server/api.fib" 's/\(loop \[\] \(if \(and \(> \@\(\. st active\) 0\) \(< \(time\/clock-now\) limit\)\)/(loop [] (if false/' ;;
    redirect-301)      sub "$h/client/redirect.fib" 's/\(and \(includes\? status \[301 302\]\) \(= method "POST"\)\)/(and (includes? status [302]) (= method "POST"))/' ;;
    redirect-307)      sub "$h/client/redirect.fib" 's/\(cond \(= status 303\)/(cond (includes? status [303 307])/' ;;
    downgrade)         sub "$h/client/redirect.fib" 's/\(cond \(and \(= \(\. from scheme\) "https"\) \(= \(\. to scheme\) "http"\)\)/(cond false/' ;;
    pool-leak)         sub "$h/client/pool.fib" 's/\(do \(evict-oldest p \(\. c key\)\) \(recur\)\)/()/' ;;
    no-deadline)       sub "$h/client/exchange.fib" 's/\(t-deadline \(\. c t\) \(soonest total/(t-deadline (. c t) (soonest 0/; s/\(if \(> \(\. o read-timeout-ms\) 0\) \(\+ \(time\/clock-now\)/(if false (+ (time\/clock-now)/' ;;
    no-host)           sub "$h/client/run.fib" 's/h1 \(if \(some\? \(get h "host"\)\) h \(assoc h "host" \[\(authority url\)\]\)\)/h1 h/' ;;
    *) echo "unknown mutant $2" >&2; exit 2 ;;
  esac
}

mkdir -p "$OUT/base/fib"; cp -r "$R/lib/fib/http" "$R/lib/fib/http.fib" "$OUT/base/fib/"
for spec in http-message-spec http-server-spec http-client-spec; do
  "$FIBC" test "$R/specs/$spec.fib" -I "$OUT/base" -I "$R/lib" > "$OUT/base-$spec.log" 2>&1 || { echo "mutant-http: the unmutated copy fails $spec: see $OUT/base-$spec.log" >&2; exit 2; }
  echo "unmutated $spec: $(grep '^total:' "$OUT/base-$spec.log")"
done
survived=0
for m in "${MUTANTS[@]}"; do
  d=$OUT/$m; mkdir -p "$d/fib"; cp -r "$R/lib/fib/http" "$R/lib/fib/http.fib" "$d/fib/"
  apply "$d" "$m"
  spec=$(spec_for "$m")
  if timeout 300 "$FIBC" test "$R/specs/$spec.fib" -I "$d" -I "$R/lib" > "$d.log" 2>&1; then
    echo "SURVIVED  $m  ($spec)"; survived=1
  else
    echo "killed    $m  ($spec: $(grep -c '^  FAIL\|^  TRAP' "$d.log") failing; $(grep '^total:' "$d.log" | sed 's/^total: //; s/ in .*//'))"
  fi
done
exit $survived
