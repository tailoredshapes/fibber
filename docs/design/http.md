# HTTP client and server (HTTP-1)

`fib.http` is a native HTTP/1.1 implementation: one message layer, a `Transport` seam under it, a client and a server on top. There is no libcurl and no
`extern` in `lib/fib/http` (ADR 0011): sockets, polling and name resolution are `fib.os.net`, the clock is `fib.os.time`. The libcurl client and its
`:lib "curl"` driver were removed by this package; the ADR's allow-list lost `http/server/date` (the date is computed in fibber) and the driver row.

The public API is the [library README](../../lib/fib/http/README.md). This file is the architecture, the limits, the security notes and what is deferred.

## Layers

```
fib.http.client            fib.http.server           the two facades (Hato- and Ring-shaped, as before)
  client/run   pool          server/connection reply   the engines: redirects, pool, deadlines | the request loop, replies
fib.http.body   fib.http.message   fib.http.wire        framing: streaming bodies, chunked, response head | request head, header lines
fib.http.reader                                          bytes -> lines and counted reads, with byte bounds checked before storing
fib.http.transport   fib.http.tcp                        the seam | descriptors: TCP, pipes; connect (with getaddrinfo) in fib.os.net
fib.os.net  fib.os.io  fib.os.time                       the platform layer (extern lives only here)
```

`fib.http.uri` (RFC 3986), `fib.http.url` (query and form encoding, Base64), `fib.http.headers`, `fib.http.types` and `fib.http.bytes` are pure.
`fib.http.testing` has the helpers the specs use and any program may: a message parsed from arbitrary fragmentation, and a loopback `Factory` served by
the native server over pipes.

## The Transport seam (what TLS-1 plugs into)

```clojure
(defstruct Transport (read: (fn (i64) (Result (Array i8) Error))   ; at most n bytes; the empty array is end of stream
                      write: (fn ((Array i8)) (Result unit Error)) ; all of the bytes or an error
                      close: (fn () unit)                          ; idempotent
                      set-deadline: (fn (i64) unit)                ; absolute fib.os.time/clock-now ns, 0 = none; read and write observe it
                      peer: str))
(defstruct Target (scheme host port connect-timeout-ms ca-file))   ; what a factory opens
(defstruct Factory (name: str open: (fn :send (Target) (Result Transport Error))))
```

A Transport is a value of closures over private state (the pattern of `fib.os.memory-stream`), used by one thread; `spawn` rejects capturing one
(case 8038). A deadline that passes makes a read or write fail with `(Timeout "io" ..)`; the client renames it `read` or `total`, the server answers 408.
`Error` is the engine's, so a transport reports its own failures in HTTP's terms (`ConnectError`, `Timeout "connect"`, `DnsError`, `TransportError`).

Implementations in this package: `fib.http.tcp` (a connected socket, `connect` through `fib.os.net/resolve-host` and `connect-tcp`), a pair of pipes
(`pipe-pair`, or the descriptors `pipe-fds` so a task builds its own end), and `fib.http.transport/scripted` (reads from a list of chunks, writes into
a sink; memory only).

The client takes a **Factory per scheme** in `Options.factories` (a `(Map str Factory)`, default `{"http" (tcp-factory)}`; `with-factory` adds one).
`https` has none by default: the request fails with `(TlsUnsupported "unsupported scheme https: a TLS Transport factory (driver fib.tls) must be
registered ...")` before any connection is attempted (case 8035 asserts zero connections opened). It is never sent in the clear and never downgraded.

**TLS-1 (built: `fib.tls`, docs/design/tls.md) provides, as this section asked:** a a `Factory` whose `open` makes the TCP connection (`fib.http.tcp/connect host port connect-timeout-ms`, or the Transport it
returns wrapped), runs the handshake over it with `fib.crypto`, and returns a `Transport` whose `read` and `write` carry application data and whose
`set-deadline` also bounds the handshake and record reads; `Target.host` is the name to verify, `Target.ca-file` the explicit trust anchor option the old
client had. Then `(tls/https-options options provider trust)` (which is `(with-factory options "https" (tls/factory provider trust))`) is the whole integration: `fib.http` does not require `fib.tls` or any crypto driver, the caller passes the provider and the trust as values; a server side needs `fib.http.server.connection/serve-connection`
over a Transport from an accepted socket, which is already how the server works. Nothing in the engine knows the scheme beyond the pool key.

## Message layer (RFC 9112)

- **Reading** (`fib.http.reader`): a line ends at CRLF and nowhere else; a bare LF, a bare CR and a NUL are errors. Each line has a byte bound
  (`max-line-bytes`), each head a total bound and a line count, all checked before the byte is stored.
- **Head** (`fib.http.wire`, `fib.http.message`): header names are case-insensitive tokens and are lower-cased; values keep their order and repeats
  (`Set-Cookie` is never merged); whitespace before the colon, obsolete line folding, a leading space on the first line, a missing colon, an empty
  name, an invalid method, version or target are `ProtocolError`s. Targets: origin-form, absolute-form (the URL is kept in `uri`), `*`.
  HTTP/1.0 and 1.1 only. Received header bytes above 127 are kept as Latin-1 characters (as before); every slice of header text is at an ASCII byte
  (the fuzzer found a slice inside a character that trapped; case 8041).
- **Framing**: `request-framing` and `response-framing` give `NoBody`, `Fixed n`, `Chunked` or `UntilClose`. Content-Length together with
  Transfer-Encoding, a repeated Content-Length (including `3, 3`), a non-digit length (`-1`, `+3`, `0x3`, empty, `3 4`), a length beyond i64 or beyond
  the limit, a Transfer-Encoding other than exactly `chunked` (`xchunked`, `chunked, identity`, a second field) are errors, never resolved in favour
  of one reading. A response with both is refused too. HEAD, 1xx, 204 and 304 have no body whatever the headers say.
- **Chunked**: size in hex with overflow checked per digit, extensions after `;` ignored but must be header-value bytes, the CRLF after every chunk
  checked, trailers parsed into `Body.trailers` (a trailer that could change framing or routing is refused: Content-Length, Transfer-Encoding, Host,
  Connection, Trailer, Expect), at most 100 trailers.
- **Bodies are streams** (`fib.http.body`): `Body` yields chunks of the decoded payload, the empty array ends it; `read-all` buffers under a limit.
  A truncated fixed body or chunk is an error, not a short message. Writing: `write-fixed`, `write-chunked` (a producer pulled until it answers nil).
- **Content-Encoding** is an optional `Decoder` seam (`Options.decoders`, `with-decoder`): `(Decoder "gzip" (fn :send ((Array i8)) (Result (Array i8) Error)))`
  inflates a whole body, and `Accept-Encoding` lists the registered names. DEFLATE is not implemented here (zlib is a driver, to be written in its own
  repo like the crypto ones). With no decoder the body arrives as sent and `content-encoding` is kept (the old client decoded gzip through libcurl).
- **Limits** are options with safe defaults: header bytes 65536, line 8192, header count 100, body 8 MiB (server) / response 8 MiB (client).

## Client

`request`, `get`, `post`, ... and `build-http-client` with `request-with`, `request-stream`, `pool-stats`, `close-http-client`: the shapes of the libcurl
client, plus the stream. A client is local to its thread; `request-async` gives each request its own.

- **Errors** (`fib.http.types/Error`, extended): `InvalidRequest`, `ProtocolError`, `LimitError` (too large), `TransportError`, `StatusError response`,
  `Closed`, `Cancelled` (existing), and new `DnsError`, `ConnectError`, `Timeout phase` (`connect`, `read`, `total`), `TlsUnsupported`, `RedirectLoop`,
  `RedirectRefused`. Code that matched the old set with a catch-all keeps working (the one thing that does not: `TransportError 28` was libcurl's
  timeout, now `Timeout`; the fib-hocon change is listed in the package report).
- **Deadlines**: `connect-timeout-ms` (one connect, clamped to what is left of the total), `read-timeout-ms` (the wait for any one piece of the
  response; default none), `timeout-ms` (the whole request including redirects; default 30 s). Armed before every wait; the nearer wins.
- **Redirects** (`client/redirect.fib`, pure): off by default; 301/302/303/307/308 with a Location; 303 becomes GET (HEAD stays), 301 and 302 turn POST into
  GET, 307 and 308 keep method and body; a method change drops the body and the headers that described it; `authorization`, `cookie` and
  `proxy-authorization` do not follow a change of origin (scheme, host or port); https to http is refused (`RedirectRefused`); a repeated
  (method, URL) is `RedirectLoop`, as is exceeding `max-redirects` (default 5). Location is resolved by RFC 3986 section 5.2.
- **Pool**: per origin (`scheme://host:port`), most recently idle first, `max-idle-per-host` (default 4; 0 disables reuse and sends `Connection: close`),
  `idle-timeout-ms` (default 30 s, swept on each use), the oldest evicted over the limit. A connection returns to the pool only when its body ran to the
  end under keep-alive; an unread body closes it (`abandon`). There is no liveness probe: a pooled connection the peer closed fails on first use and,
  for an idempotent method, is retried once on a fresh connection. `pool-stats` reports connections opened, reuses and idle.
- **DNS** is `getaddrinfo` (`fib.os.net/resolve-host`, the platform layer; blocking, no timeout; stream sockets; addresses tried in the resolver's
  order). A native resolver is later work (docs/design/static-linking.md: musl's resolver needs `/etc/hosts` and `/etc/resolv.conf` in the image).
- `Expect: 100-continue` on a request with a body holds the body until the interim 100 (or one second, or a final status, in which case the body is
  not sent and the connection is closed). Userinfo in the URL becomes a Basic `Authorization` header.

## Server

`run-server` (Ring-shaped, a handler `Request -> Response`) and `run-server-reply` (a handler returning `Reply`: `(Whole response)` or
`(Streamed status headers next)`, the body pulled from `next` and sent chunked on HTTP/1.1, close-delimited on 1.0). One acceptor task; one task per
connection, at most `workers` at once (default 64; beyond it the listener stops accepting and the kernel's backlog queues). A request loop per
connection: pipelined requests are answered in order, `max-requests-per-connection`, `Connection: close`, HTTP/1.0 semantics, HEAD (the head of the GET,
no body), 100-continue (sent only after the framing was accepted), `Date` (IMF-fixdate computed in fibber), 408/413/417/431/501/400 answers that close.

- **Slowloris**: `idle-timeout-ms` (first byte of the next request, closed silently), `header-timeout-ms` (the whole head from its first byte; never
  longer than `request-timeout-ms`; a drip of one byte at a time cannot outlive it: 408), `request-timeout-ms` (body, and each write).
- **Graceful stop**: `stop-server` stops accepting, lets requests in flight finish for up to `drain-timeout-ms` (idle connections close within
  100 ms), then cancels what is left and closes the listener. Connections queued in the kernel and not yet accepted are reset.
- Routing is out of scope; the server has `wrap-header` only. Request bodies are buffered (up to `max-body-bytes`) with their trailers; streaming
  request bodies are not offered (the message layer has them; the server API does not yet).

## Security notes

Request smuggling defences are the framing rules above, tested as a table (specs/http-message-spec.fib: 67 refused shapes whole and one byte at a time,
13 valid borderline shapes accepted) and by fuzzing (100,000 mutants: no trap, the same result for every fragmentation, every truncation and every
CRLF-to-LF of a valid request refused). The HTTP/1.0 request with Transfer-Encoding is refused. Responses with ambiguous framing are refused, not guessed.
Credentials do not cross origins on redirect. `https` never degrades. A hostile peer is bounded in memory by the line, head, body and trailer limits
and in time by the deadlines; the client's body limit applies to chunked and close-delimited bodies as they stream.

## Not done (deferred)

HTTP/2; cookies (not in this package); compression (the Decoder seam only); proxies (`http_proxy`, `no_proxy`, CONNECT: the old client honoured
libcurl's environment; the design is a Factory that connects to the proxy and, for `https`, issues CONNECT before the TLS factory runs, so it belongs
with TLS-1); a native DNS resolver and happy eyeballs; a liveness probe for pooled connections; streaming request bodies on the client and the server;
HTTP pipelining by the client (off by design); a lingering close on the server (a reset instead of a FIN when a peer's bytes are unread at stop).
Windows is not built or tested; macOS uses the platform layer's Darwin ABI table (`getaddrinfo`'s `ai_addr` offset differs and is chosen by it) and
was not run.

## Tests

`specs/http-message-spec.fib`, `http-uri-spec.fib`, `http-client-spec.fib`, `http-server-spec.fib`, `http-fuzz-spec.fib` (`fibc test specs/http-*-spec.fib`);
cases 7300-7308 and 8030-8041; `scripts/mutant-http.sh` (18 planted faults, each killed by the spec named in the script);
`scripts/test-http.sh` (the Python differential suite: 27 tests, the native client against python3 servers and python clients against the native server);
`scripts/http-diff.sh` (the curl CLI against python's http.server and the native server over 22 requests: identical outcomes); `scripts/bench/http-bench.py`
and [docs/shootout/http.md](../shootout/http.md) for the measurements.

## Provenance

[Ring](https://github.com/ring-clojure/ring/blob/master/SPEC) and [Hato](https://github.com/gnarroway/hato) supplied the API shapes. The code is original.
Protocol rules follow [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) and
[RFC 3986](https://www.rfc-editor.org/rfc/rfc3986.html).
