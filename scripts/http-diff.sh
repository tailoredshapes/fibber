#!/bin/bash
# scripts/http-diff.sh: the differential harness of fib.http (HTTP-1), outside the gate. The curl CLI is the client; two servers answer the same
# routes: python3's http.server (the reference) and the native fib.http server. For each curl invocation the observable outcome (status, size,
# content type, a checksum of the body, the number of connections curl opened) must be identical. The other direction (the native client against
# python servers: chunked, streaming, slow, redirects) is scripts/tests/http/integration.py, run by scripts/test-http.sh.
# usage: FIBC=/path/to/fibc scripts/http-diff.sh            exit 0 when every case agrees, 1 on a difference, 2 on a setup error
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC}
command -v curl >/dev/null || { echo "http-diff: curl not found" >&2; exit 2; }
W=${HTTP_DIFF_OUT:-$HOME/.cache/fibber-scratch/http-diff}; mkdir -p "$W"; ulimit -v 16000000 || true
"$FIBC" build "$R/scripts/tests/http/diff-server.fib" -I "$R/lib" -o "$W/diff-server" >"$W/build.log" 2>&1 || { cat "$W/build.log" >&2; exit 2; }
head -c 70000 /dev/urandom > "$W/upload.bin"

start() { # start NAME CMD..: the server's stdin is a fifo we hold open (closing it stops the server); its first output line is its port, kept in PORT_NAME
  local name=$1; shift; rm -f "$W/$name.in" "$W/$name.out"; mkfifo "$W/$name.in"
  "$@" < "$W/$name.in" > "$W/$name.out" 2> "$W/$name.err" &
  echo $! >> "$W/pids"
  local fd; exec {fd}<>"$W/$name.in"
  for _ in $(seq 100); do [ -s "$W/$name.out" ] && break; sleep 0.1; done
  printf -v "PORT_$name" '%s' "$(head -1 "$W/$name.out")"
}
rm -f "$W/pids"
start py python3 "$R/scripts/tests/http/diff-server.py"
start fb "$W/diff-server"
PY_PORT=$PORT_py; FB_PORT=$PORT_fb
[ -n "$PY_PORT" ] && [ -n "$FB_PORT" ] || { echo "http-diff: a server did not start (see $W/*.err)" >&2; exit 2; }
trap 'kill $(cat "$W/pids") 2>/dev/null' EXIT

observe() { # observe PORT CURL-ARGS..  (a path starting with / is the URL tail)
  local port=$1; shift; local args=() a
  for a in "$@"; do case $a in /*) args+=("http://127.0.0.1:$port$a") ;; *) args+=("$a") ;; esac; done
  curl -s -m 5 -o "$W/body" -w "%{http_code} %{size_download} %{content_type} conns=%{num_connects} redirects=%{num_redirects}" "${args[@]}" 2>&1
  case " $* " in *" -I "*) echo " md5=(headers, not compared)" ;; *) echo " md5=$(md5sum < "$W/body" | cut -c1-12)" ;; esac
}
fail=0; n=0
check() { # check LABEL CURL-ARGS..
  local label=$1; shift; n=$((n+1))
  local a b; a=$(observe "$PY_PORT" "$@"); b=$(observe "$FB_PORT" "$@")
  # the reference server adds "; charset=utf-8" to text/plain: compare media types only
  a=${a//; charset=utf-8/}; b=${b//; charset=utf-8/}
  case "$a$b" in 000*|*" 000 "*) echo "http-diff: a server did not answer ($label): $a / $b" >&2; exit 2 ;; esac
  if [ "$a" = "$b" ]; then echo "same     $label  [$b]"; else echo "DIFFER   $label"; echo "   python: $a"; echo "   native: $b"; fail=1; fi
}
check "GET small" /hello
check "GET 404" /nope
check "HEAD" -I /hello
check "POST body" -d "hello world" /echo
check "POST 70000 bytes binary" --data-binary @"$W/upload.bin" /echo
check "PUT body" -X PUT -d "x=1" /echo
check "chunked upload" -H "Transfer-Encoding: chunked" --data-binary @"$W/upload.bin" /echo
check "Expect: 100-continue" -H "Expect: 100-continue" --data-binary @"$W/upload.bin" /echo
check "chunked response" /chunked
check "1 MiB response" /big
check "302 not followed" /redirect
check "302 followed" -L /redirect
check "301 followed" -L /redirect301
check "301 POST followed (becomes GET)" -L -d "a=b" /redirect301
check "status 204" /status/204
check "status 500" /status/500
check "request header reaches the handler" -H "X-Probe: probed" /probe
check "two URLs reuse one connection" /hello /hello
check "three URLs, three kinds of reply, one connection" /hello /chunked /status/204
check "slow handler" /slow
check "HTTP/1.0 client" --http1.0 /hello
check "Connection: close" -H "Connection: close" /hello /hello
echo "http-diff: $n cases, $([ $fail = 0 ] && echo 'all agree' || echo 'DIFFERENCES')"
exit $fail
