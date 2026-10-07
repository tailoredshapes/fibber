# HTTP

`fib.http` supplies common values and helpers, `fib.http.server` a Ring-style HTTP/1.1 server and `fib.http.client` a Hato-style client. Both are
native: one message layer (RFC 9112) over a `Transport` seam, plain TCP built on [fib.os](../os/README.md) (`fib.os.net`: sockets, poll, getaddrinfo).
There is no libcurl and no native library to link; a program that uses them builds with `fibc build` (and `--static`). TLS is the next package:
until a TLS `Transport` factory is registered an `https` URL is a typed error (`TlsUnsupported`), never a plain-text request. Native execution is
tested on Linux LP64; the Darwin backend of `fib.os` is checked separately and not run. The architecture, limits and what is deferred are in
[docs/design/http.md](../../../docs/design/http.md).

## Running the examples

```sh
fibc build examples/http-server.fib -I lib -o /tmp/http-server
/tmp/http-server

# In another terminal:
fibc build examples/http-client.fib -I lib -o /tmp/http-client
/tmp/http-client
```

`FIB_HTTP_PORT` selects the example server's port; `FIB_HTTP_URL` the client's destination (http only).

## Values and headers

```clojure
(:require [fib.http :as h]
          [fib.http.server :as server]
          [fib.http.client :as client])
```

| Type | Fields |
|---|---|
| `h/Response` | `status: i64`, `headers: (Map str (Vec str))`, `body: (Array i8)` |
| `h/Request` | `request-method: str`, `uri: str`, `query-string: (Option str)`, `headers`, `body`, `trailers`, `version: str`, `remote-addr: str`, `server-port: i64` |
| `h/ClientRequest` | `method: str`, `url: str`, `headers`, `body` |

Methods retain their case; use uppercase HTTP method names. `uri` and
`query-string` retain their encoded text. Header names are normalized to lowercase
by the helpers. Values retain their order within each field; `Set-Cookie` values
are never combined into one string. Received header values preserve wire bytes
as Latin-1 characters. Outgoing string values are written as their UTF-8 bytes.

`h/header` returns the first value as an Option, and `h/header-values` returns all
values. `h/header-set` replaces a field; `h/header-add` appends another value.
`h/validate-headers` rejects invalid names, control characters, and CRLF injection.
Manual header maps can use mixed case; lookup and transport setup normalize them.

Bodies are binary arrays. `h/text-response status text` and `h/response text`
encode UTF-8; the latter uses status 200. `h/with-header response name value`
returns an updated response. `h/body-text response` and `h/decode-text bytes`
return `(Result str h/Error)`, rejecting invalid UTF-8 without trapping.

Query and form parameters are `(Vec (Pair str str))`, preserving duplicate keys
and their order. `h/query-string`, `h/with-query`, and `h/form-body` encode them.
`h/form-decode` and `h/parse-query` decode percent escapes and UTF-8, returning
Result values. Plus signs mean spaces in form encoding. `parse-query` gives an
empty value to a parameter without `=`, and skips empty components.

## Server

```clojure
(defun handler (request: h/Request) -> h/Response
  (cond (= (. request uri) "/") (h/response "hello\n")
        (= (. request uri) "/echo") (h/Response 200 {} (. request body))
        :else (h/text-response 404 "not found\n")))

(server/run-server
  (server/wrap-header handler "x-app" "fibber")
  (with (server/default-options) (port 8080) (workers 4)))
```

`run-server` returns `(Result server/Server h/Error)` immediately after binding
and starting a fixed worker pool. Handlers must be sendable; the ownership checker
rejects captures of ordinary cells. Each worker owns one connection at a time,
and its buffered reader remains local to that worker.

`server/port` reports the bound port, including the assigned port when options
specify zero. `server/stop-server` stops acceptance, cancels reads and writes,
joins workers, and closes the listener. Calling it again is harmless.
`server/wait-server` joins workers without initiating shutdown. Both return
Result values. Keep the server handle and call `stop-server` on normal exit.
Shutdown waits for an executing handler to return; a trapping handler follows
Fibber's normal process-abort semantics.

| Server option | Default |
|---|---:|
| `host` | `"127.0.0.1"` (numeric IPv4 or IPv6 address) |
| `port` | 8080 |
| `workers` (most connections at once, one task each) | 64 |
| `backlog` | 128 |
| `request-timeout-ms` | 10,000 |
| `max-body-bytes` | 8,388,608 |
| `max-header-bytes` | 65,536 |
| `max-line-bytes` | 8,192 |
| `max-requests-per-connection` | 100 |
| `max-headers` | 100 |
| `header-timeout-ms` (whole head, from its first byte) | 10,000 |
| `idle-timeout-ms` (wait for the next request) | 10,000 |
| `drain-timeout-ms` (graceful stop waits for requests in flight) | 5,000 |

Options are immutable structures, updated with `with`. Request and response I/O
have deadlines; the request deadline covers the complete head and body, including
fragmented reads. Headers and trailers are limited to 100 fields, and chunked
bodies to 4,096 nonempty chunks. Idle keep-alive connections occupy workers until
their request deadline or shutdown. Workers are bounded; excess connections wait
in the socket backlog.

The server accepts HTTP/1.0 and HTTP/1.1 origin-form requests, fixed and chunked
bodies, 100-continue, keep-alive, and pipelining. It derives response framing,
supplies Date, suppresses bodies for HEAD and 204/205/304, and rejects duplicate
Host/Content-Length, simultaneous Content-Length/Transfer-Encoding, obsolete
folding, forbidden trailers, and malformed lines. Invalid response headers produce
a 500 and close the connection. Chunked transfer encoding is refused on HTTP/1.0.

## Client

```clojure
(client/get "http://example.com")
(client/get "http://example.com" (with (client/default-options) (timeout-ms 5000)))
(client/post "http://localhost:8080/echo" "hello")

(client/request
  (h/ClientRequest "PUT" "http://localhost:8080/echo"
    {"content-type" ["application/octet-stream"]} bytes)
  (client/default-options))
```

`request` and the convenience functions return `(Result h/Response h/Error)`; `get` and `post` have an optional Options argument, `head`, `put`,
`patch` and `delete` take Options explicitly. The client derives Host, Content-Length and the body framing; passing Content-Length or
Transfer-Encoding yourself is an `InvalidRequest`. Userinfo in the URL becomes a Basic Authorization header.

`client/build-http-client options` makes a reusable client (a connection pool); `client/request-with client request` sends on it;
`client/request-stream client request` returns a `client/StreamResponse` (status, headers, a `fib.http.body/Body` to pull chunks from, `abandon`);
`client/pool-stats` gives connections opened, reused and idle; `client/close-http-client` closes the pool (twice is harmless; requests after close
return `h/Closed`). A client is local to its thread; `client/request-async request options` returns a `(Task (Result h/Response h/Error))` and owns
its client. `client/form-request`, `client/basic-auth`, `client/bearer-auth` and `client/wrap-header` are as before.

| Client option | Default |
|---|---:|
| `connect-timeout-ms` (one connect) | 10,000 |
| `timeout-ms` (whole request, redirects included) | 30,000 |
| `read-timeout-ms` (the wait for any one piece of the response) | 0 (none) |
| `follow-redirects` | false |
| `max-redirects` | 5 |
| `decompress` (use the registered decoders) | true |
| `throw-exceptions` (status 400 and above is `h/StatusError`) | true |
| `ca-file` (passed to the TLS factory; unused by TCP) | nil |
| `max-response-bytes` | 8,388,608 |
| `max-header-bytes` | 65,536 |
| `max-headers` | 100 |
| `max-line-bytes` | 8,192 |
| `max-idle-per-host` (0: no reuse) | 4 |
| `idle-timeout-ms` (pooled connections) | 30,000 |
| `factories` (scheme to Transport factory) | `{"http" (tcp-factory)}` |
| `decoders` (Content-Encoding) | none |

Timeout zero means none (for `connect-timeout-ms`, the rest of `timeout-ms`). `(client/with-factory options "https" factory)` registers the TLS
driver's factory; `(client/with-decoder options (client/Decoder "gzip" inflate))` registers a decoder (DEFLATE is a driver, not here).

Errors are `h/Error` values: `InvalidRequest`, `ProtocolError`, `LimitError` (too large), `TransportError`, `StatusError response`, `Closed`, `Cancelled`,
`DnsError`, `ConnectError`, `Timeout phase` (`"connect"`, `"read"`, `"total"`), `TlsUnsupported`, `RedirectLoop`, `RedirectRefused`.

Redirects (off by default): 301/302/303/307/308; 303 becomes GET, 301 and 302 turn POST into GET, 307 and 308 keep method and body;
credentials are dropped on a change of origin; https to http is refused; loops and `max-redirects` are `RedirectLoop`.

## Transport

`fib.http.transport/Transport` is the seam: `read`, `write`, `close`, `set-deadline`, `peer`; `Factory` makes one from a `Target`. `fib.http.tcp` has
TCP, pipes and `connect`; `fib.http.transport/scripted` an in-memory stream; `fib.http.testing/loopback-factory` a client connection served by the
native server over pipes. See the design record for how a TLS driver plugs in.

## Scope and checks

Not provided: HTTP/2, TLS (next package), cookie jars, proxies, a native DNS resolver, WebSockets, multipart builders, streaming request bodies, and
automatic JSON or Transit coercion.

```sh
fibc test specs/http-message-spec.fib specs/http-uri-spec.fib specs/http-client-spec.fib specs/http-server-spec.fib specs/http-fuzz-spec.fib -I lib
HTTP_FIBC=fibc scripts/test-http.sh            # the cases and the Python differential suite
FIBC=fibc scripts/http-diff.sh                 # curl against python3 and the native server
FIBC=fibc scripts/mutant-http.sh               # 18 planted faults, each must be killed
```
