# HTTP client and server

The library has a Ring-style HTTP/1.1 server and a Hato-style client. Hato is a
client around Java's HTTP stack; Fibber's client uses libcurl as its native
transport. Protocol handling, data structures, options, and server scheduling
live in Fibber. The server uses POSIX sockets through the shared `fib.os` transport boundary.

Requests and responses hold immutable headers and binary byte arrays. Header
names are case-insensitive; repeated values, especially Set-Cookie, remain
separate. UTF-8 conversion is explicit and returns an error for invalid bytes.
Handlers are functions from a request to a response; middleware composes those
functions. Model options are typed structures with defaults and `with` updates.

Server framing accepts HTTP/1.0 and HTTP/1.1, fixed and chunked request bodies,
100-continue, HEAD, keep-alive, and pipelined requests. It rejects ambiguous
framing, duplicate Host and Content-Length fields, obsolete line folding, and
header injection. Headers, lines, bodies, chunk counts, request times, worker
counts, and requests per connection have bounds. Shutdown stops acceptance and
joins workers; raw descriptors are closed on every ordinary error path.

The client supports synchronous and native-task requests, reusable connections,
binary request and response bodies, repeated headers, encoded query and form
parameters, redirects, timeouts, decompression, and verified HTTPS. A reusable
client is local to its thread; asynchronous requests own separate clients.
Failures return Result values; HTTP status errors retain their response.

Protocol cases are written before implementation. Integration checks use a
Python HTTP implementation and raw sockets, including fragmented requests,
chunked bodies, connection reuse, rejection paths, concurrency, and local TLS.

## API and ownership

The [library README](../../lib/fib/http/README.md) is the public API reference.
`fib.http` is transport-independent; requiring it does not introduce libcurl.
`fib.http.server` and `fib.http.client` are separate facades. Both use Response,
binary byte arrays, and maps of header names to vectors of values. The server
Request separates the encoded URI and query, body, and trailers. ClientRequest
contains an absolute URL. Options are immutable records updated with `with`.

A Server provides local stop and wait closures over a private state containing
the listening descriptor, stop atom, and fixed vector of native tasks. Neither
the descriptor nor its cell is exposed in the public value. Workers
receive only the descriptor, immutable options, a sendable handler, and the stop
atom. Each worker owns its accepted descriptor until connection processing
returns, then closes it. Readers use private cells for the current buffer,
position, and deadline; a reader never crosses threads.

A reusable Client provides local request and close closures over a private
libcurl handle cell and immutable options. Requiring Send rejects sharing it,
as case 7305 demonstrates. Cases 7307 and 7308 reject direct access to native
handle fields; they originally compiled before this encapsulation. One-shot and
asynchronous requests
allocate and close their own clients. libcurl global initialization and cleanup
are balanced with each client's lifetime. The supported libcurl versions have
thread-safe global initialization; no Fibber global or hidden mutable pool is
introduced. Resetting an easy handle between calls clears request options while
retaining its connection cache. The interoperability check observes one peer
port across three successive requests from the same client.

Explicit close operations are idempotent. Ordinary Result errors take the same
cleanup paths as success: streams, header lists, temporary C strings, and native
handles are released. Native allocation failure and a handler's `trap` follow
the language's existing process-abort behavior. Stop cancels socket I/O and
joins executing handlers; it cannot interrupt an arbitrary user handler.

## Protocol engine

The server keeps unread bytes in one connection buffer, including bytes from
subsequent pipelined requests. Line reading requires CRLF. Framing is chosen
before Expect handling or body allocation: no body, an exact Content-Length,
or chunked. Duplicate lengths and a length combined with Transfer-Encoding
fail and close the connection. The chunk decoder enforces its decoded size
and chunk-count limits, checks each chunk's CRLF, and reads bounded trailers.
Fields that could alter framing or routing are refused in trailers.

The head and body share an absolute request deadline. Reads poll in intervals
of at most 100 ms so shutdown can wake idle workers. Writes have a separate
deadline, handle short writes, retry EINTR/EAGAIN, and use MSG_NOSIGNAL. A
disconnected peer therefore cannot terminate the process through SIGPIPE.
Accepted sockets are nonblocking, so a writable poll result cannot turn into an
unbounded blocking send. Two regression tests originally reproduced an 8 MiB
response exceeding its deadline and shutdown hanging when the peer stopped
reading. Both pass with nonblocking accepted sockets. The worker pool bounds
simultaneous handlers and connections; idle keep-alive
connections retain their worker until the next request deadline. This provides
a simple synchronous reference architecture rather than an event-loop server.

Response framing is derived from actual byte counts. A supplied Content-Length
or Transfer-Encoding does not override it. HEAD advertises the corresponding
body size but sends no body. Statuses 204 and 304 omit Content-Length and send
no body; 205 sends Content-Length zero. Handlers choose whether to close a
connection through Connection, subject to server limits and shutdown. Invalid
handler headers become a 500 with a closed connection.

HTTP method and header-name tokens are checked as ASCII bytes before slicing.
This matters for malformed input: case 7300 originally reproduced a runtime
trap on a non-ASCII name, and now verifies a Result error. UTF-8 body decoding
checks continuation bytes, shortest encoding, surrogate exclusion, and the
Unicode maximum. Raw body bytes never undergo this conversion implicitly.

## Native client boundary

The implementation uses libcurl's easy API and `fib.os.memory-stream`. A bounded
FILE stream captures headers and another captures the decoded body. Both
disable stdio buffering so excess writes become errors during transfer. Each
buffer has one extra byte for stdio's terminating NUL; `ftell` supplies the
actual length, including embedded NUL bytes in the payload. Successful captures
copy only their written prefix into immutable Fibber arrays. The extra-byte
boundary is tested with bodies exactly at, one byte above, and above a zero
size limit. No C write callback or temporary file is needed.

The bindings assume LP64 and are tested natively on Linux. The OS layer owns
libc differences; its Darwin source backend is checked separately and still
requires native macOS/toolchain validation. Opaque C handles use the same integer-address
convention as the compiler's existing native bindings. Stage 2 currently types
only the explicitly declared prefix of a `:varargs` extern, so the bindings
declare libcurl's third argument word explicitly. Pointer values use the LP64
integer/pointer register representation. Existing runtime declarations such as
`fclose(ptr)` retain their pointer signature. A general variadic-call inference
extension remains compiler work, independent of this HTTP implementation.

Only HTTP and HTTPS URLs and redirects are enabled. TLS chain and hostname
verification use libcurl's defaults and are never disabled by these options.
An explicit CA bundle is supported. Redirect following defaults off; enabling
it has Hato's `:always` behavior and a finite redirect limit. The `:normal`
policy, cookie jars, streaming, multipart construction, WebSockets, and data
coercion are outside this initial API, as documented in the public README.

## Validation

The nine native cases 7300–7308 pass, covering headers and injection, request
framing, size overflow, query/form encoding, invalid UTF-8, Base64, response
framing, server startup/shutdown, client options, and the rejected cross-thread
client capture and access to native handle fields. Cases are run with the native
memory audit.

The offline Python suite passes all 27 tests. It exercises each implementation
against an independent counterpart and also tests Fibber-to-Fibber exchange.
It verifies four concurrent server handlers and four async client requests,
descriptor reclamation after repeated requests, repeated response headers,
gzip, redirects including POST-to-GET on 302, closed handles, size limits,
fragmentation, pipelining, 100-continue, trailers, timeouts, stalled readers, and disconnected
peers. A generated local certificate proves that trusted HTTPS succeeds while
an untrusted certificate and a hostname mismatch both fail. CI runs this suite
after the stage-2 cases with libcurl and openssl installed.

Two integration tests enable Fibber's runtime ownership trace while transferring
actual messages. They require allocations to be recorded and reject duplicate
frees, unknown frees, and live owned objects after client or server shutdown.
Client success, status errors, size errors, connection reuse, and async requests
are covered. Server binary echo, invalid handler output, and shutdown are covered.
This audits Fibber-owned values; it does not instrument libcurl or raw C buffers.

The full native gate also passes after the OS migration: stage 2 and stage 3
emit identical lIR; ownership and module cases pass; the standard-library suite
has its existing 21 open cases and the existing expected 1707 atom-lifetime
audit failure. There are no new unexpected failures. The OS suite additionally
checks native ABI conformance against C headers and Darwin backend emission.
See [the OS design record](os.md) for platform status and compiler integration.
Both README examples have been built and run together, including binary echo
and encoded query routes.

Reproduce with `HTTP_FIBC=./F scripts/test-http.sh`. Set HTTP_TEST_OUT to keep
the built fixtures in a chosen scratch directory; otherwise the script removes
its temporary directory. It holds `/tmp/fibsuite.lock` to avoid overlap with
the performance harness.

## Provenance

[Hato](https://github.com/gnarroway/hato) supplies the API inspiration for a
small client, reusable native transport, options, and middleware.
[Ring](https://github.com/ring-clojure/ring/blob/master/SPEC) supplies the request
and response model. The Fibber implementation is original code; it does not
copy either project's implementation.

Protocol framing follows [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html).
The native boundary uses the documented
[libcurl option API](https://curl.se/libcurl/c/curl_easy_setopt.html),
[default FILE write transport](https://curl.se/libcurl/c/CURLOPT_WRITEDATA.html),
and [bounded memory streams](https://man7.org/linux/man-pages/man3/fmemopen.3.html).
