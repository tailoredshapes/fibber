# HTTP

`fib.http` supplies common values and helpers, `fib.http.server` supplies a
Ring-style HTTP/1.1 server, and `fib.http.client` supplies a Hato-style client.
Native execution is tested on Linux LP64. Platform sockets, errors, clocks, and
memory streams use [fib.os](../os/README.md), which includes a Darwin backend
whose macOS execution and toolchain validation remain pending. The server
uses POSIX sockets; the client uses libcurl 7.85 or newer for HTTP, HTTPS, TLS
verification, compression, and connection reuse.

## Running the examples

```sh
./F build examples/http-server.fib -I lib -o /tmp/http-server
/tmp/http-server

# In another terminal:
./F build examples/http-client.fib -I lib -l curl -o /tmp/http-client
/tmp/http-client
FIB_HTTP_URL=https://example.com /tmp/http-client
```

Install libcurl's development package for client linking, for example
`sudo apt-get install libcurl4-openssl-dev`. A server application needs only
Fibber and the system C library. `FIB_HTTP_PORT` selects the example server's
port; `FIB_HTTP_URL` selects the client example's destination.

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
| `workers` | 4 |
| `backlog` | 128 |
| `request-timeout-ms` | 10,000 |
| `max-body-bytes` | 8,388,608 |
| `max-header-bytes` | 65,536 |
| `max-line-bytes` | 8,192 |
| `max-requests-per-connection` | 100 |

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
(client/get "https://example.com")
(client/get "https://example.com"
  (with (client/default-options) (timeout-ms 5000)))
(client/post "http://localhost:8080/echo" "hello")

(client/request
  (h/ClientRequest "PUT" "http://localhost:8080/echo"
    {"content-type" ["application/octet-stream"]} bytes)
  (client/default-options))
```

`request` and the convenience functions return `(Result h/Response h/Error)`.
`get` and `post` have an optional Options argument; `head`, `put`, `patch`, and
`delete` take Options explicitly. `ClientRequest` supports additional methods
and binary bodies. The client derives Content-Length and transport framing;
passing Content-Length or Transfer-Encoding yourself is an InvalidRequest.

For connection reuse, call `client/build-http-client options`, use
`client/request-with client request`, then `client/close-http-client client`.
Every ordinary Result path in the one-shot `request` closes its native handle.
Reusable clients use local closures over their private state, so they cannot
cross threads. Native handles and state cells are never exposed by the public
client or server values. Closing twice is harmless; requests after close return
`h/Closed`.

`client/request-async request options` returns `(Task (Result h/Response h/Error))`.
Use `join` or `@` to obtain the result. Each async request owns a separate client
and executes on a native thread. Applications should bound the tasks they start.

`client/form-request` builds a form-encoded request. `client/basic-auth` and
`client/bearer-auth` add authorization to a request; Basic credentials are encoded
from UTF-8. `client/wrap-header` composes a sendable request function with an
additional header.

| Client option | Default |
|---|---:|
| `connect-timeout-ms` | 10,000 |
| `timeout-ms` | 30,000 |
| `follow-redirects` | false |
| `max-redirects` | 5 |
| `decompress` | true |
| `throw-exceptions` | true |
| `ca-file` | nil (system trust store) |
| `max-response-bytes` | 8,388,608 |
| `max-header-bytes` | 65,536 |

Timeout zero means no timeout. Redirect following is explicit and restricted to
HTTP and HTTPS schemes; enabling it permits HTTPS-to-HTTP redirects, like Hato's
`:always` policy. Hato's separate `:normal` redirect policy is not provided.
TLS certificate chains and hostnames remain verified; `ca-file` adds an explicit
CA bundle through libcurl. Proxy environment variables follow libcurl's policy.

HTTP statuses 400 and above return `h/StatusError response` by default, retaining
the body and headers. Set `throw-exceptions` false to receive them as Ok values.
Transport errors, malformed messages, size limits, closed handles, and invalid
requests also return Error variants. These are Result values, rather than language
exceptions. Decompressed bodies retain the original response headers.

Responses are buffered in bounded memory streams. `max-response-bytes` bounds the
decoded body; `max-header-bytes` bounds aggregate headers, including redirects and
interim responses. No response body or header is written to a temporary file.

## Scope and checks

This first API supplies buffered HTTP messages. Server TLS, HTTP/2 serving,
WebSockets, streaming/SSE, multipart builders, cookie jars, and automatic JSON or
Transit coercion are not provided. Client protocol negotiation follows the linked
libcurl build. Middleware and serialization can be added around these typed
messages without changing the transport contract.

Run the protocol cases and independent offline HTTP/TLS tests with:

```sh
HTTP_FIBC=./F scripts/test-http.sh
```

The checks include binary and chunked exchange, repeated headers, fragmentation,
HEAD, pipelining, malformed framing, limits, timeouts, cleanup, concurrency,
connection reuse, redirect behavior, gzip, verified local TLS, untrusted
certificates, and hostname mismatches. See [the design record](../../../docs/design/http.md)
for implementation details and provenance.
