# A Send-able TLS session

Status: **proposal for the owner to choose from** (package FOLLOWUP-1, 2026-10-07). Nothing here is implemented. The owner picks an option in section 5; the package that implements it starts after
CRYPTO-3 has merged (section 6). Every statement about today's code was read in the tree of that date: `lib/fib/tls/conn.fib`, `record.fib`, `lib/fib/http/transport.fib`, `spec/types.md` 1.2 and 5.

## 1. The problem

`fib-db-postgres` (its own repository, v0.1.0) refuses `sslmode=require`, `verify-ca` and `verify-full` because **a `fib.db` connection must be able to move between tasks** (a pool hands a
connection to the task that asked for it) and a `fib.tls` `Session` cannot: it is not `Send`. A database over the network without TLS is a driver for trusted networks only.

## 2. Why the session is not Send (what the compiler says, and what is behind it)

`Send(T)` (spec/types.md 5.1) is "no value of type `T` can reach a `Cell` that is not inside an `Atom`", computed field by field; a closure is `Send` iff its colour is `send` (5.4), and it is `send`
iff every capture is `Send`. Three separate facts make `Session` fail it, and fixing one is not enough:

| # | fact | where | rule it breaks |
|---|---|---|---|
| 1 | `Session` is a struct of **closures**: `read`, `write`, `close`, `set-deadline` are `(fn ..)` fields. A struct field of function type has the colour `local` unless it is written `(fn :send ..)` (types 5.4, "on a struct or enum field: local") | `lib/fib/tls/conn.fib:8` | `Send(Session)` needs every field Send |
| 2 | each closure captures a `Conn`, which is **seven `Cell`s**: `inbuf rtr wtr rclosed closed failed updates`; and a `Rec` of five more (`inbuf rd wr hsbuf ccs handshaking`) | `conn.fib:10`, `record.fib:13` | `Send((Cell T)) = false` |
| 3 | the byte stream underneath is an `Io` (TLS) or a `Transport` (HTTP): again a struct of closures, whose captures are a socket and cells (read-ahead buffer, deadline). `Transport` is the culprit only in the first sense: **the closure-struct shape, not its being a seam** | `record.fib:10`, `lib/fib/http/transport.fib:12` | as 1 and 2 |

Not culprits: the **provider** (`CryptoProvider p` is a type parameter: its `Send` is the driver's business, and the keys and hashers it makes are `(dyn Hasher :send)`, `(dyn PrivateKey :send)`,
which is precedent that a handle object can be Send); the **socket** (`fib.os.net` works on an `i64` fd: scalars are Send); the **key material today** (`Prot` is `alg key iv seq`, with the key as an `(Array i8)`, and `Traffic` is three `(Array i8)`: both Send; CRYPTO-3's key handles must stay Send, section 6).
`Transport` having closures is fine for HTTP, which never moves one between tasks; the issue is only for a session that has to.

So the state is plain data and the closures are packaging. The design question is where the mutable state lives and who owns it.

## 3. Options

### A. The state is a value the caller owns; operations take it by `&` (recommended)

`(TlsSession p s)` becomes a struct of immutable fields: the record-layer state (the read and write protection `Prot` with key, iv and sequence number, the input buffer, the handshake buffer, the
flags `rclosed closed handshaking`, `failed`, `updates`), the traffic secrets, the negotiated `suite` and `alpn`, a **deadline `i64`**, and the **stream `s`**. Operations mutate a copy the caller holds
in a `Cell`: `(tls/read &sess n)`, where `&sess` is the caller's `(Cell (TlsSession p s))`: the pattern of `&pos` in `fib.json.number` and `&out` elsewhere. The struct is `Send` iff `p` and `s` are;
the *cell* is the caller's, task-local, never crossing. To move a session: take `@cell` (the value), hand it to the other task, which puts it in its own cell. Nothing is shared, so nothing needs
an atomic and the owner is unique: **in-place array operations stay in place** (`array-push!`, `array-take!` on the input buffer), so the throughput CRYPTO-3 is measuring is not given up.

The stream: `Io` and `Transport` stay closure structs for HTTP. `fib.tls` is made **generic over a `Stream` protocol** with `read (self n deadline)`, `write` and `close`, which this package writes an
instance of for `fib.os.net`'s fd (a `FdStream`: `fd: i64`, no cell) and for the old closure `Io` (an adapter: not Send). The HTTP client keeps its `Transport`: `(tls/factory p trust)` builds the
closure `Session`/`Transport` the client wants **over** a `(Cell (TlsSession ..))` it creates. The old API becomes a thin layer on the new one, so `fib.http` and its `https-options` do not change.

Exact API (new names; the old ones keep working through the adapter):

```clojure
(defstruct (TlsSession p s) ...)                                   ; Send iff p and s are
(tls/connect-stream p config stream) -> (Result (TlsSession p s) TlsError)   ; the handshake; the value is :send
(tls/read &sess n) -> (Result (Array i8) TlsError)                 ; (empty only after close_notify, as today)
(tls/write &sess bytes) -> (Result unit TlsError)
(tls/close &sess) -> unit
(tls/set-deadline &sess at) -> unit
(tls/alpn sess) -> (Option str)   (tls/suite sess) -> str
(tls/fd-stream fd) -> FdStream                                     ; the socket of fib.os.net, no closure, no cell
```

and `fib-db-postgres` stores a `(TlsSession p FdStream)` in its connection struct, which is then `Send`. Over `(Cell ..)` the driver does `(tls/write &c ..)` on a local copy for the length of
one call and stores `@c` back.

Cost: the cells of `Conn` and `Rec` become fields and every function that did `(set! (. c f) v)` takes `&c` and does `(set! f v)`: a mechanical rewrite of `record.fib` (123 lines), `conn.fib` (101) and
the parts of `hs.fib`/`hs-flight.fib` that hold a `Rec` (about 200 changed lines in all, with the cases that already test them), plus the protocol and its two instances (about 80 lines) and
a case that spawns a task with a session, which is the planted test (it must fail to compile today with `cell cannot be shared between threads`). Run-time cost: none expected; a cell
indirection per field goes away.

### B. Keep the closure API; make the state atoms and the closure fields `:send`

`Session` fields become `(fn :send ..)`; `Conn` and `Rec` hold `(Atom X)` for each state value; the stream closures capture `(Atom i64)` for the deadline. **The public API does not change at all.**
Cost: `(Atom T)` requires `Send T` at construction, so every `X` is immutable data, and an atom's value is shared, so **the input buffer can no longer be mutated in place**: each record appends by copying
(`concat-bytes` today is already an allocation; the in-place `array-push!` fast path is lost), and each record costs a compare-and-swap per state field. It also hides a race: two tasks using one
connection are memory-safe but interleave records (a bug, not an error). Smallest change, worst throughput, weakest guarantee. Reasonable only if the API must not move.

### C. A handle table (`(dyn TlsSession :send)` over an integer)

State lives in the runtime behind an `i64` handle (as an fd does); `Send` is trivially true. It makes lifetime a manual matter (a session never closed leaks its table entry, a stale handle is use after
free) and takes memory safety out of the language's hands for a protocol implementation. The crypto keys use `(dyn .. :send)` over provider objects, not a table, for this reason. **Not recommended.**

### D. No change to `fib.tls`: pin the session to one task

`fib-db-postgres` runs each TLS connection in its own task (an actor) and the `fib.db` connection it hands out is a `Send` handle that sends requests to that task and waits for the answer. Works with the
language and `fib.tls` as they are. Cost: a thread per connection, a hop per query (a queue plus a wake-up: microseconds, against a query of hundreds), and a **blocking `Send` queue that the
library does not have yet** (`fib.otel.queue`'s `BQueue` and `fib.log.async` are bounded queues over atoms with a spin lock, written for logging, not a general channel). A stopgap for the driver alone, in its own repository; it does not give any other library a Send session.

## 4. Comparison

| | A owned state + generic stream | B atoms + `:send` closures | C handle table | D actor |
|---|---|---|---|---|
| `fib.tls` public API | additive; the old API on top | unchanged | additive | unchanged |
| the session is `Send` | yes, by construction | yes | yes (unsafe) | no, the driver's handle is |
| memory safety kept by the language | yes | yes | no | yes |
| in-place buffers | yes | no (copy per record) | n/a | yes |
| two tasks on one session | cannot (a cell is local) | interleaves records | corrupts | serialised by the queue |
| work | about 380 lines + a case, one package | about 150 lines + a case | runtime change | a queue library + a driver change |
| needs CRYPTO-3's AEAD key handles to be `Send` (today `Prot` holds the key bytes, which are) | yes | yes | no | no |
| TLS in `fib-db-postgres` | after A lands | after B lands | after C lands | now |

## 5. Recommendation

**A**, because it is the one that does not buy `Send` with a copy per record or a race, and it matches how the rest of the library already passes mutable state (`&x`). If the owner wants
`sslmode=require` in `fib-db-postgres` before A is done, **D** is the driver-side stopgap and costs `fib.tls` nothing. B is the fallback if the public API has to stay as it is.

## 6. Ordering with CRYPTO-3

CRYPTO-3 is working in `lib/fib/crypto` and `lib/fib/tls` now (AEAD key handles, `blit`, throughput). The Send work edits the same files (`record.fib`, `conn.fib`, `hs*.fib`, `types.fib`), so **it
starts after CRYPTO-3 has merged**, never beside it: two rewrites of one record layer do not merge. What CRYPTO-3 can do now, at no cost, to leave the door open (a request to that package, not
done here): (1) the AEAD key handle is `(dyn Aead :send)` like `Hasher` and `PrivateKey`, so a `Prot` holding one is `Send`; (2) the throughput work keeps the input buffer and the sequence number
as plain values in `Prot`/`Rec` rather than adding new cells or closures; (3) a case that captures a provider's key in a `spawn` closure (it compiles only when (1) holds). With those, A touches the
record layer's state shape and nothing in its cryptography. The first step of the Send package is the planted test: a case that spawns a task holding a `TlsSession`, which must be rejected before
the work and accepted after.

## 7. Not decided here

TLS 1.2 (never), a server side, resumption (a Send ticket cache is a separate design), and whether `fib.http`'s own `Transport` should also become Send-able by the same method (it can: `FdStream` is the
TCP case; the seam is left alone because no HTTP code moves a transport).
