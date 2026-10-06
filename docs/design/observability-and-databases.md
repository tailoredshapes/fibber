# Logging, telemetry, databases and SQL as data: `fib.log`, `fib.otel`, `fib.db`, `fib.sql` (OBS-DB)

The owner: "Next I want to add logging, monitoring, and database protocols akin to java. So that its easy to create with something like
log4j, opentelemetry and various databases", and later "I'd also like a honeysql like layer on top of fib.db". Java's ecosystem gives the
shapes (SLF4J/log4j, OpenTelemetry, JDBC); Clojure's gives the ergonomics (clojure.tools.logging, clj-otel, next.jdbc, HoneySQL 2). The
tie-breaker is the project's: Clojure's ergonomics unless they break memory safety, then Rust's.

Status: design, with a prototype of the thinnest slice of three of the four libraries (section 8). Everything called "measured" or
"passes" below was run on 2026-10-06 on this machine (28 threads) with stage 2 built from `main` at `5bd4a7a` by the v0.1.7 seed.

## 0. The short answer

- **One shared value type, `Datum`** (`fib.datum`): null, bool, i64, f64, str, bytes. Log fields, span attributes and database columns are
  `Datum`s; a protocol `ToDatum` lets a call site pass plain fibber values (`:port 5432`). It is not the spec's `Val` (stdlib §4.13), which
  is the whole dynamic value of the language and is not designed yet; `Val` would embed `Datum`.
- **No process-wide mutable state.** Java and OpenTelemetry keep a global LoggerContext and a global TracerProvider. Fibber has no
  global mutable state to put them in (`def` is a compile-time constant, spec/types.md §8; the runtime has globals, the language does not),
  and the project forbids it in Rust and avoids it in fibber. So a `LogSystem`, a `TracerProvider` and a `DataSource` are **values**
  built at start-up and passed down; a `Logger` or `Tracer` is a value made from one. Tasks get them the way they get any value: by
  capture. A program that wants zero wiring calls `(system-from-env)` in `main` and passes the result.
- **Context (log4j's MDC, OTel's Context) is a field of the value, not a thread-local.** `(with-context lg :request-id id)` returns a
  logger whose records carry the field; a task that captured it logs with it. Measured alternative: a task-local slot read with
  `pthread_getspecific` costs 1.1 ns per read and needs a runtime change to copy the slot at `spawn`; the value costs a field read
  (section 2.4).
- **A disabled log call costs one compare.** The level macros put `(if (enabled? lg L) ..)` around the call, so the message and the
  field values are not evaluated. Measured: 0.33 ns per disabled `(debug lg "never" :i i :sq (* i i))` against 0.27 to 0.31 ns for the
  loop alone (section 2.5).
- **Errors are values, resources are closed once, and there is no catch.** `fib.db` returns `(Result T DbError)` with a SQLSTATE;
  `with-transaction` commits on `Ok` and rolls back on `Err`; a trap in a transaction body is handled by running the body in a task
  (`try-transact`, package D3), not by unwinding, which does not exist (docs/design/exceptions.md, stage 1).
- **SQL as data is a sum type plus a macro.** HoneySQL's maps mix keywords, numbers, strings, vectors and maps; fibber's collections are
  homogeneous, so a query is one enum `SqlV`, and the `sql` macro turns the HoneySQL text a user writes into it at compile time.
  `format` binds every number and string as a parameter. A corpus of 26 queries formatted by HoneySQL 2.7.1479 and by the prototype in
  three dialects gives 78 identical lines (section 5.6).
- **First package that gives users something: L1 (`fib.log`) and D1 (`fib.db` with SQLite)**, both prototyped here; then S1 (`fib.sql`
  format, prototyped), then O1 tracing. PostgreSQL (D2) needs C1 (`fib.crypto`) for SCRAM-SHA-256.

## 1. What the tree gives and what it lacks (found while prototyping)

| Fact | Where | Consequence here |
|---|---|---|
| No catch, no unwinding; a trap in a task is isolated and `try-join` returns it | exceptions.md stage 1, `fib.async/try-join` | errors as `Result`; a body that may trap runs in a task |
| No global mutable state; `def` is evaluated at compile time | types §8 (`def` initialisation) | providers are values (section 6.1) |
| `keyword` (string to keyword at run time) is not built (stdlib row 1505); `(str :k)` gives `":k"` | prelude | rows are `(Map str Datum)`; `col` takes a keyword and looks up its text |
| A macro's template resolves names in the USING module (syntax §5); `fib.x/f` resolves only if the user `:use`s `fib.x` (or for the implicit facades); `(var fib.x/f)` does not reach a module the user only `:require`d | measured: `unbound name fib.log/enabled?` with `(:require [fib.log :as log])` alone | the macro libraries are `:use`d (section 5.3 and open question 1) |
| A library function that no program reaches is not type-checked | `levels-from-env` had `and-then`'s arguments swapped and compiled until a scenario called it (commit `bd57bad`) | every public function needs a scenario |
| `fibc run` and `fibc test` resolve externs against the process (the JIT); `fibc build -l:libsqlite3.so.0` links the runtime library (no `-dev` package here) | `compiler/native/jit.fib` `names-check` | C-library drivers are tested by a script, not by the gate's `fibc test specs` (section 6.3) |
| No scope-exit hook (`Drop`); `with-open` waits for L12 | stdlib §7 L12, row 1471 | connections are closed by `conn-close`/`with-connection`; idempotent close |
| `(dyn P :send e)` boxes any `Send` implementation; atoms are `Send` | types §2.15 | sinks, exporters and connections are `(dyn .. :send)` where the choice is made at start-up |
| docker 29.8.2 is usable | `docker info` | PostgreSQL and an OTel collector can run locally for D2/O3 tests (none was started for this design) |

## 2. `fib.log`

### 2.1 Use

```clojure
(ns app.main (:use fib.log))
(defun main () -> i64
  (let [sys (system-from-env)                      ; FIB_LOG_LEVEL, FIB_LOG_LEVELS, FIB_LOG_FORMAT=json
        lg  (get-logger sys "app.db")]
    (do (info lg "connected" :host "db1" :port 5432)
        (debug lg "row" :id (expensive-id))         ; off: `expensive-id` is not called
        (let [lg (with-context lg :request-id "r-17")]
          (with-tasks [s] (fork s (fn () (warn lg "slow" :ms 812)))))   ; the task's record carries request-id
        0)))
```

Text: `2026-10-06T15:04:09.808Z INFO  app.db.pool - connected host=db1 port=5432 ok=true ratio=0.5`.
JSON: `{"ts":"2026-10-06T15:04:09.808Z","level":"INFO","logger":"app.db.pool","msg":"connected","host":"db1","port":5432,"ok":true,"ratio":0.5}`
(both printed by the prototype, `probe/p3.fib`).

### 2.2 Levels and the hierarchy

`TRACE DEBUG INFO WARN ERROR FATAL OFF` (0 to 6). A `LevelTable` is a root level and rules by name prefix; the longest prefix wins and a
prefix covers a name only at a dot (`app.db` covers `app.db.pool`, not `app.dbx`): log4j's logger hierarchy without objects. A
logger's level is resolved once, in `get-logger`, so `enabled?` is one compare of two integers. Changing levels at run time (log4j's
reconfiguration) is a new `LogSystem`; a program that wants it holds the level in an `Atom` (package L2 adds `(set-level! sys name l)`
behind an atom read: one more load per check, to be measured before it is the default).

### 2.3 Structured fields, not format strings

A call is a message and keyword/value pairs (`:host h :port 5432`). The macro wraps each value in `->datum` (protocol `ToDatum`: i64, f64,
bool, str, keyword, Datum; a library adds an impl for its own types), so the record holds data, the text sink writes `k=v` and the
JSON sink writes typed JSON. `(Vec (Pair keyword Datum))` keeps order and allows a repeated key. A format string API (`logf`) is
`(info lg (format "..." ..))`: the message is evaluated only when the level is on, so nothing is lost by not having one.

### 2.4 Context (MDC) across tasks: the options, measured

| Option | How it reaches a task | Cost per log call | Change needed |
|---|---|---|---|
| **A. Context in the logger value** (chosen) | the task's closure captures the logger | none beyond the record's own copy | none |
| B. Task-local slot (`pthread_getspecific`), copied at `spawn`/`fork`/`pmap` chunk start | the runtime copies the parent's slot | 1.13 to 1.23 ns per read (measured: 200M reads in a loop, `probe/logbench.fib`) plus a retain per spawn | rt/task.lir, every spawn path, a release at task end |
| C. Clojure's `binding` (dynamic vars) | conveyed by `bound-fn` | as B | the language form `binding` (stdlib lists it, not built) |

A is Rust's `slog` and Go's explicit `context.Context`; B is log4j's ThreadContext and OTel Java's Context. A is chosen because it costs
nothing, needs no runtime change, cannot leak a context into an unrelated task (a pooled thread keeps no state), and makes the data
flow visible, which the language already does for every other value. The cost is that a function that logs takes a logger argument.
The usual pattern, a request handler that makes `(with-context lg :request-id ..)` once and passes it down, is what clj-otel and
next.jdbc users do with their context and connectable anyway. B stays the fallback if a measured program shows argument threading is
the real burden; it would be a runtime package of its own (~60 lines of lIR and a spec section), not part of OBS-DB.

Checked: scenario "a task forked in with-tasks logs with the context of the logger it captured" (3 tasks, every record starts with
`req=r-3`); the planted fault "context dropped in emit" fails 5 scenarios.

### 2.5 Laziness: what a disabled call costs

`(debug lg msg :k v ..)` expands to

```clojure
(let ((lg0 lg)) (if (fib.log/enabled? lg0 1) (fib.log/emit lg0 1 msg [(Pair :k (fib.log/->datum v)) ..]) ()))
```

Measured (`probe/logbench.fib`, built with `fibc build`, 200M iterations, three runs):

| Loop body | ns per iteration |
|---|---|
| the loop alone (`acc + (i xor 7)`) | 0.277, 0.269, 0.314 |
| plus a disabled `(debug lg "never" :i i :sq (* i i))` | 0.337, 0.334, 0.335 |
| an enabled `(info lg "on" :i i)` to a sink that does nothing (2M iterations) | 106.9, 82.7, 82.4 |
| `pthread_getspecific` and a load (option B's read) | 1.125, 1.126, 1.225 |

The disabled call is under 0.1 ns here because LLVM can hoist the level test out of the loop; where it cannot (the logger arrives
through a call), the cost is one load, one compare and a predicted branch, ~1 cycle. The enabled path's ~83 ns is the record: the
wall clock read, the context concatenation, a heap `Datum` per field and the dyn call; L2 should measure it again with a real sink
(a JSON line to a file is a `write` system call, ~1 us, which dominates). Planted fault "macro not lazy" fails 2 scenarios.

### 2.6 Sinks (log4j's appenders)

`(defprotocol Sink (sink-write (self rec: LogRecord) -> (Result unit str)))`, chosen at start-up as `(dyn Sink :send ..)`.

- **Thread safety:** a sink is called from many tasks. A line is rendered whole and written with one `write-all`, so lines do not
  interleave on a pipe (a write of at most `PIPE_BUF`, 4096 bytes, is atomic) or an `O_APPEND` file. A longer line or a partial write
  can interleave; the async sink below removes that by having one writer. The memory sink uses an atom.
- **Failure:** a sink returns `Err`; the logger counts it in the system's failure atom and prints the first one to standard error
  (log4j's StatusLogger). A sink must not trap: a trap in a sink would end the logging task, and in `main` the process. Sinks written in
  the library are trap-free by construction (no `nth` without a bound check, no `unwrap`); a user sink is wrapped by L2's
  `guarded-sink`, which runs it in a task with `try-join` on its first failure only if the owner wants that cost (open question 4).
- **Prototype:** `TextSink`, `JsonSink` (any descriptor), `MemorySink`. **L2:** `FileSink` with size and daily rotation (rename, reopen,
  keep N; the rotation is done by the one writer task), `AsyncSink` (a bounded queue, the same structure as O1's span processor: a
  `fib.chan` when P-chan lands, an atom-guarded ring until then; policy drop-newest with a dropped counter, never block the caller;
  `flush!` and `shutdown!` drain it), `FanoutSink` (several sinks, each with its own level), and an OTel bridge (2.8).

### 2.7 Configuration

Environment: `FIB_LOG_LEVEL` (root), `FIB_LOG_LEVELS` (`app.db=debug,app.http=warn`; an unreadable entry is skipped, a log
configuration must not stop a program), `FIB_LOG_FORMAT` (`text` or `json`). Data: `(levels-from-map "info" {"app.db" "debug"})`
returns `Err` naming the first bad level. A data file (log4j2's XML or properties) is a fibber map read with `fib.edn` when that exists;
no XML. Both checked by the scenario "the environment sets the root and the rules; the data form names a bad level".

### 2.8 Bridge to OpenTelemetry

`(otel-sink tracer-provider inner)`: a sink that adds `trace_id` and `span_id` of the current span to the record's fields and passes it
on, and an OTLP log exporter that ships records as OTel LogRecords (severity number = level mapped to OTel's 1..24 scale). Because the
current span lives in a context value (3.2), the bridge reads it from the logger's context fields, which `with-span` adds.

## 3. `fib.otel`

### 3.1 Data model (OpenTelemetry's, kept)

- `TracerProvider` (resource, sampler, span processors) is a value; `(tracer provider "app.db")` a `Tracer` (instrumentation scope).
- `Span`: trace id (16 bytes), span id (8), parent span id, name, kind (internal, server, client, producer, consumer), start and end
  (`wall-ns`), attributes `(Vec (Pair str Datum))`, events (name, time, attributes), links (span context, attributes), status (unset,
  ok, error with description). A live span is an owned record held by the code that started it; `end-span!` consumes it and hands the
  finished `SpanData` to the processors, so a span ends at most once **by ownership**, not by a runtime flag: ending twice is a use after
  move, rejected at compile time.
- `MeterProvider`, `Meter`, instruments `Counter` (monotonic), `UpDownCounter`, `Histogram` (explicit bucket bounds, sum, count,
  min, max), `Gauge` (observable callback). Hot counters are `fib.adder` stripes (one CAS, no shared cache line); a histogram is an
  adder per bucket. Attribute sets are interned per instrument (a map from the attribute vector to the cell), so recording with an
  already seen attribute set is a map lookup and an atomic add.
- Resource: `service.name`, `service.version`, `host.name`, `process.pid`, `telemetry.sdk.*`, from `OTEL_SERVICE_NAME` and
  `OTEL_RESOURCE_ATTRIBUTES` (the OTel environment variables, kept as named).

### 3.2 Context propagation

As for logs (2.4), the context is a value: `(with-span tracer ctx "name" (fn (ctx) ..))` starts a child of the span in `ctx`, calls the
body with a context that holds the new span, and ends the span when the body returns (status from a `Result` body: `Err` sets error).
Tasks get the context by capture. **HTTP:** `fib.http.client` gains `(wrap-trace client ctx)`, a request function that writes
`traceparent` (`00-<trace-id>-<span-id>-<flags>`) and `tracestate` and records a client span; `fib.http.server` gains `(wrap-trace
handler provider)` middleware that reads them, starts a server span as the remote parent's child, and passes the context in the
request map (an extra field of `Request`, or a `(Pair Context Request)` handler; to be settled in O3 with the http owner). Both are
ordinary Ring-style middleware, the shape `fib.http` already has (`server/wrap-header`).

### 3.3 Sampling

`always-on`, `always-off`, `trace-id-ratio` (the trace id's low 8 bytes against `ratio * 2^63`), `parent-based` (the default, as OTel).
An unsampled span is a `NonRecordingSpan` value: the same API, no attributes kept, no processor call; the cost of an unsampled span is
creating ids (a 16-byte random from a per-task `fib.rng` state seeded from `fib.os` entropy, not a system call per span).

### 3.4 Export

- **Processors:** `simple` (export at end, for tests) and `batch` (the default): a bounded queue (2048 spans), a background task that
  exports when 512 are waiting or every 5 s, drop-newest when full with a dropped counter (exported as a metric), `force-flush!` and
  `shutdown!` (which the program calls at exit; there is no finalizer to do it).
- **OTLP/HTTP with JSON** (`/v1/traces`, `/v1/metrics`, `/v1/logs`) through `fib.http.client` (libcurl: HTTPS for free). JSON first
  because it needs no protobuf encoder and the collector accepts it; protobuf later as `fib.proto` (a hand-written encoder for the
  three OTLP messages is ~300 lines). Retry with backoff on 429/503 honouring `Retry-After`; never block the application.
- **Console exporter:** one JSON line per span, for development.
- **Exporter contract:** whatever an exporter is, the spans it was given arrive (a fake in-process OTLP receiver, `fib.http.server` on
  an ephemeral port, decodes the JSON and the contract compares ids, parents, attributes and status).

### 3.5 Semantic conventions

HTTP server and client spans (`http.request.method`, `url.full` or `url.path`, `http.response.status_code`, `server.address`,
`server.port`, span name `GET /route`), DB spans from `fib.db` (`db.system.name` sqlite/postgresql, `db.query.text` with parameters
NOT included, `db.operation.name`, `db.collection.name` when fib.sql knows it, `server.address`). `fib.db` takes an optional tracer in
its `DataSource` value; no tracer, no cost.

### 3.6 Cost

Not prototyped, so not measured. Expected from the measured parts: a sampled span start and end is an id generation (~10 ns), a record
(~80 ns, the order of an enabled log record, 2.5) and an enqueue on the batch queue (~20 ns uncontended): ~100 to 150 ns, against OTel
Java's ~300 ns to 1 us. O1 measures it before anything is built on it.

## 4. `fib.db`

### 4.1 Use (next.jdbc's shape)

```clojure
(ns app.store (:use fib.db fib.sql))
(match (open-sqlite "/var/app/data.db")
  ((Err e) (Err e))
  ((Ok conn)
   (with-transaction [tx conn]
     (try-let [_ (execute! tx ["insert into people (name, age) values (?, ?)" name age])
               r (execute-one! tx ["select id from people where name = ?" name])]
       (Ok r)))))
(execute! conn (format (sql {:select [:id :name] :from [:people] :where [:> :age min-age]})))
```

`execute!` returns `(Result (Vec (Map str Datum)) DbError)`; a statement without columns returns `[{"update-count" n}]`
(next.jdbc's `:next.jdbc/update-count`); `execute-one!` the first row or `nil`. A literal vector `["sql ?" a b]` is turned by the macro
into `(Query "sql ?" [(->datum a) (->datum b)])`: Clojure's text, fibber's homogeneous vector. A `Query` (what `fib.sql/format` returns)
is accepted as is.

### 4.2 Protocols (JDBC re-expressed)

| JDBC | fib.db | Note |
|---|---|---|
| `Driver` | `(defprotocol Driver (connect (self url: str props: (Map str str)) -> (Result (dyn Connection :send) DbError)))` | a driver library registers nothing globally: the program names the driver value (`sqlite/driver`, `pg/driver`) |
| `DataSource` | a value: a driver and its url, or a `Pool` | `(get-connection ds)`, `(with-connection [c ds] ..)` |
| `Connection` | `(defprotocol Connection (conn-execute ..) (conn-close ..))` (prototype) plus `conn-prepare`, `conn-reduce` in D1 | used by one task at a time; may move between tasks |
| `PreparedStatement` | `(prepare c sql)` returns a `Statement` value owned by the caller; `(execute-prepared! st params)` | D1: a per-connection LRU statement cache makes `execute!` reuse them (the prototype prepares per call: 4.8) |
| `ResultSet` | `(plan c q)` (next.jdbc's reducible): `(reduce f init (plan c q))` steps the cursor and calls `f` per row without building a vector | a `Reducible` impl whose `reduce` owns the statement for the walk and finalizes it when the walk ends |

Names: `plan` is also a private function of `compiler/expand/expr.fib`; a library function in a module is not the self-hosting trap
(that is core forms and builtins), but the compiler must never `:use` fib.db. Type and variant names were checked against `compiler/`
and `lib/` (`grep -rnw`); the SQL enum's variants are `QKw QVec QMap ..` because `SKw SVec SMap` are the compiler's `Stx` variants and
variant names are global across linked modules.

### 4.3 Parameters and types

`Datum` both ways: i64 (INTEGER, int8), f64 (REAL, float8), str (TEXT, text/varchar), bool (SQLite 0/1, PostgreSQL boolean), nil (NULL),
bytes (BLOB, bytea). Dates: fibber has no date type yet (`fib.os.time` is nanoseconds); D2 maps timestamps to `DInt` epoch microseconds
and a `fib.time` package adds `Instant` and `LocalDate` with `ToDatum` impls. Decimal (numeric) arrives as `DStr` until a decimal type
exists (BigDecimal is listed in stdlib, not built), so no precision is lost silently.

### 4.4 Errors

`(defstruct DbError (sqlstate: str code: i64 message: str))` with the SQL standard's SQLSTATE; PostgreSQL sends it; SQLite has none, so
`sqlstate-of` maps the extended result codes (unique and primary key 23505, not null 23502, foreign key 23503, check 23514, other
constraint 23000, busy/locked 40001, cannot open 08001, generic error 42000, rest HY000). `(db-error? e "23")` tests a class.

### 4.5 Resource safety

A connection is an owned value; `conn-close` takes its handle out by compare-and-set, so a second close (or a racing one) is a no-op
and a closed connection answers `08003` instead of passing a freed pointer to C (scenario "a closed connection refuses work"; planted
fault "closed flag never set" fails it). Statements in the prototype never outlive one `conn-execute` (prepare, bind, step, finalize on
every path). D1's `with-connection` closes on every ordinary exit; when L12's scope-exit hook lands, `Connection` gets a drop impl and
the macro becomes a convenience. What is not covered and cannot be without a catch: a TRAP between open and close abandons the handle
(the OS closes the file at exit; a PostgreSQL socket is closed by the OS when the process ends, and the server rolls back).

### 4.6 Transactions, honestly

`(with-transaction [tx conn] body)`: BEGIN; the body returns a `Result`; `Ok` commits (a failed COMMIT rolls back and returns its
error), `Err` rolls back. Two scenarios check both (planted fault "rollback commits" fails one). A trap in the body: there is no
unwinding, so `transact` cannot run ROLLBACK. Options:

- a. **`try-transact`** (D3): runs the body in a task with the connection moved in, `try-join`s it; on `Err (Trap ..)` the parent,
  which kept the driver's raw handle (an i64), issues ROLLBACK and closes it. The task's fibber objects are abandoned (the stage 1
  rule), the database handle is not. Sound because the task has finished before the parent touches the handle. Costs one thread start
  (~20 us) per transaction, so it is opt-in.
- b. Do nothing: PostgreSQL rolls back a transaction whose connection closes; SQLite's journal rolls back on the next open. The process
  ends on a trap in `main` anyway.
- c. Wait for stage 2 of exceptions (catch) and make `transact` unwind-safe then.

Proposal: b by default (documented), a as `try-transact` in D3, c when it exists.

### 4.7 Drivers

- **SQLite** (D1, prototyped): the system `libsqlite3.so.0` through 21 externs (`lib/fib/db/sqlite/ffi.fib`); handles are i64 as
  `fib.http.client.ffi` does for libcurl; text and blobs bound with `SQLITE_TRANSIENT` so SQLite copies them before the C string is
  freed. All parameters fit registers, so no `:varargs :fixed` is needed (only `sqlite3_config` and `sqlite3_mprintf` are variadic,
  and neither is used). Darwin: the system libsqlite3 is present; the same externs.
- **PostgreSQL** (D2), the wire protocol in pure fibber over `fib.os.net` (`connect-tcp`, `send-all`, `read-fd`): StartupMessage,
  authentication (SCRAM-SHA-256 and md5 refused by default), the simple query protocol for BEGIN/COMMIT and DDL, the extended protocol
  (Parse/Bind/Describe/Execute/Sync) with parameters in binary format for int8/float8/bool/bytea and text for the rest, RowDescription to
  column names and type oids, DataRow decoding by oid, ErrorResponse fields to `DbError` (C = SQLSTATE), ReadyForQuery transaction
  status, Terminate on close. ~900 lines. TLS: deferred; it comes as an SSLRequest followed by a TLS session over the socket from a
  libssl binding (`fib.tls`, externs to OpenSSL's `SSL_*` with memory BIOs so fibber still owns the socket); until then `sslmode=disable`
  only, refused loudly otherwise.
- **`fib.crypto`** (C1, for SCRAM): `sha256`, `hmac-sha256`, `pbkdf2-hmac-sha256`, constant-time `bytes=`, `random-bytes` from
  `fib.os.random`, base64; pure fibber, tested against the published vectors (FIPS 180-4 examples, RFC 4231 for HMAC, RFC 7914 section 11
  for PBKDF2-SHA256, RFC 7677's SCRAM-SHA-256 exchange). Not a general cryptography library: no ciphers (TLS brings its own).
- **MySQL** and others later, each a separate git library (docs/design/packages.md) implementing `Driver` and `Connection`, run against
  the same Driver contract.

### 4.8 Pool (D3)

`(pool driver url {:size 10 :min-idle 2 :acquire-timeout-ms 5000 :max-lifetime-ms 1800000 :idle-timeout-ms 600000 :validate "select 1"})`:
a bounded set of connections behind an atom of free slots and a condition (P-chan's channel when it lands); `with-connection` borrows
one and returns it, rolling back an open transaction first; a connection that failed validation or exceeded its lifetime is closed and
replaced; acquire times out with SQLSTATE `08004`. Like HikariCP and next.jdbc's pooled datasources, minus the reflection.

### 4.9 Round trip, measured

`probe/dbbench.fib` against `dbbench.c` (the same statements through the same calls), SQLite in memory, 200,000 operations, three runs:

| Operation | fib.db (us/op) | C (us/op) | fib.db overhead |
|---|---|---|---|
| insert, autocommit | 1.98, 1.92, 1.91 | 1.53, 1.59, 1.44 | ~0.4 us |
| insert, one transaction | 1.57, 1.49, 1.35 | 1.05, 0.94, 0.97 | ~0.5 us |
| select one row by id | 2.93, 3.23, 2.64 | 1.66, 1.55, 1.58 | ~1.3 us |

The overhead is the prototype's: C strings built and read one byte at a time (`fib.os.native/bytes-new`, `c-string`), an 8-byte heap
slot per out-parameter, column names read per statement, a `Map` per row. D1 replaces them with a `memcpy`-based copy, a statement
cache (no prepare per call) and names read once per statement; target: within 20% of C. Against a server (PostgreSQL over TCP, ~50 to
100 us per round trip) the overhead is noise, as the brief expects.

## 5. `fib.sql` (HoneySQL 2 in fibber)

### 5.1 Plain data or typed records

Clojure's answer is plain maps, and HoneySQL's value is that a query is data a program builds, merges and inspects. Typed records
(`(defstruct Select (columns ..) (from ..) (where ..))`) would catch a misspelled clause at compile time but lose the data shape: every
clause a library adds would be a new field, and `merge` of two queries would be a function per pair of types. Plain fibber maps are
not possible either: `{:select [:a [:b :bee]] :limit 10}` has values of three types, and fibber's maps and vectors are homogeneous.
The middle path chosen: **one sum type** for every query value,

```clojure
(defenum SqlV (QNil) (QKw name: str) (QParam value: Datum) (QRaw text: str) (QVec items: (Vec SqlV)) (QMap entries: (Vec (Pair str SqlV))))
```

which is what the spec's `Val` would be for this domain (`Val` is a `dyn`-free sum type, stdlib §5 T4). Cost: one heap object per node
(a 10-clause query is ~40 small objects) and a match per node when formatting; nothing dynamic at run time beyond the tag. The static
typing that is kept: a parameter is a `Datum`, so only bindable values reach the database; a clause name is a keyword, checked by the
macro; a query is a `SqlV`, so `execute!` cannot be handed a string built by concatenation unless the program writes `[:raw ..]`.

### 5.2 The `sql` macro

`(sql {:select [:a] :from [:t] :where [:= :id id]})` is expanded at compile time: a keyword becomes `(QKw "a")`, a number or string a
`(QParam ..)`, `true`/`false` a parameter that `format` writes inline as TRUE/FALSE (HoneySQL does), `nil` `QNil`, a vector `QVec`, a
map `QMap` (clause order does not matter: `format` writes HoneySQL's order), and a symbol or a call `(->sqlv x)`: a `SqlV` (so queries
compose: a subquery is a variable holding a query) or any `ToDatum` value as a parameter. HoneySQL's text works unchanged; the 26-query
corpus is pasted into the fibber program as is by `scripts/tests/sql/gen.py`.

### 5.3 Helpers and the macro-resolution limit

`(-> (select :a [:b :bee]) (from :t) (where [:= :id 7]) (order-by [:a :desc]) (limit 3))` builds the same value as the literal
(scenario "the helpers build what the literal builds"); `where` twice ANDs the conditions as `honey.sql.helpers/where` does (found by the
oracle: the first version replaced). The helpers are macros because their arguments are HoneySQL data. Because a template's `fib.sql/..`
names resolve only in a module that `:use`s `fib.sql` (section 1), the library is `:use`d, and its `format` shadows the implicit
`fib.print/format` in that module (`fib.print/format` still names it). Clojure would write `(:require [honey.sql :as sql])` and
`(sql/format ..)`; that needs syntax-quote to qualify names to the defining module (open question 1).

### 5.4 Dialects and extensibility

`(format q)` writes identifiers unquoted with `-` as `_`; `(format-with q :ansi)` quotes each dotted part with `"` and keeps `-`
(PostgreSQL and SQLite), `:mysql` with backticks: HoneySQL's own rules, checked on the corpus. **Extensibility** (S2): HoneySQL's
`register-clause!` and `register-op!` mutate a global registry. Fibber's version is a `Formatter` value: the clause order and two maps,
clause name to a formatting function and operator name to one (`(fn (Dialect (Vec SqlV) &ps) str)`), with `default-formatter` holding the
built-ins; `(format-using fmt q)` uses it, and `(register-clause fmt :upsert f :after "values")` returns a new formatter. PostgreSQL's
`:on-conflict`/`:do-update-set` (upsert), `:returning` (done), `:for update`, `:lateral`, window functions and `:filter` come as
registered extensions in a `fib.sql.pg` module, which is also how a third-party dialect plugs in.

### 5.5 Into fib.db

`(execute! conn (format q))` works today (scenario "a fib.sql query runs through execute!, its values bound", SQLite). D1 adds
`execute!` on a `SqlV` directly with the connection's dialect (SQLite and PostgreSQL both `:ansi`), so the program never formats by
hand.

### 5.6 Differential testing against HoneySQL

`scripts/test-sql.sh`: `gen.py` writes one fibber program from `scripts/tests/sql/corpus.edn`; `oracle.clj` formats the same corpus with
HoneySQL 2.7.1479 (Clojars `com.github.seancorfield/honeysql`, sha256 `b9c0c6c582fc1c78011afc519257d1c7e2a23911b618ab57e1b23494d0050290`,
in `~/.cache/fibber-scratch/tools/honeysql/`) on Clojure 1.12.0; both print `SQL | kind:param ..` per query and dialect; `diff` must
be empty. Result: `test-sql: 78 lines identical (26 queries x 3 dialects)`. Planted faults, each makes it fail: operands of AND/OR not
parenthesised (30 lines differ), booleans bound as parameters (12), quoted identifiers losing their dashes (4), `= nil` not written
`IS NULL` (6). Growing the corpus is the way S1 proceeds: every clause and operator added comes with corpus lines, and a random query
generator over the corpus grammar (fib.test properties) is S2.

## 6. Shared concerns

### 6.1 No global state: why, and what it costs

Java's `LogManager.getLogger`, OTel's `GlobalOpenTelemetry` and JDBC's `DriverManager` are process-wide registries. In fibber they would
need a runtime global (a new builtin), they would be shared mutable state that every task touches, and they hide the data flow that the
language otherwise makes explicit. The decision: values (`LogSystem`, `TracerProvider`, `MeterProvider`, `DataSource`) built in `main`
and passed. What it costs: a parameter on functions that log or trace. What it saves: no initialisation order problems (a library
cannot log before configuration exists, since it has no logger until it is given one), tests that run in parallel with different
configurations (the spec runs 18 scenarios with as many systems), and no `reset!` between tests. Libraries take an optional logger or
tracer in their own configuration value (`fib.db` DataSource, `fib.http` server options), and none by default costs nothing.

### 6.2 Performance budget

| Operation | Budget | Measured (section) |
|---|---|---|
| log call, level off | ~1 ns | 0.33 ns/iter vs 0.27 to 0.31 loop alone (2.5) |
| log call, level on, null sink | 100 ns | 83 to 107 ns (2.5) |
| task-local context read (option B) | n/a | 1.1 to 1.2 ns (2.4) |
| span start and end, sampled | 150 ns | not measured (O1) |
| counter add (adder) | 5 ns uncontended | `fib.adder`, docs/shootout/atoms.md |
| SQLite insert, in memory | within 20% of C (D1) | 1.35 to 1.98 us vs 0.94 to 1.59 us in C (4.9) |
| SQLite select by key | within 20% of C (D1) | 2.64 to 3.23 us vs 1.55 to 1.66 us in C (4.9) |
| PostgreSQL round trip | server-bound | D2 |
| `fib.sql/format` of a 10-clause query | 2 us | not measured (S1) |

### 6.3 Testing strategy

- **fib.test specs and contracts.** An **Appender contract** (`specs/log-appender-contract.fib`: every record once and whole, 100
  records from 4 tasks arrive whole, fields in order) run against the memory sink and JSON lines through a pipe; a **Driver contract**
  (`scripts/tests/db/driver-contract.fib`, 9 scenarios: types round trip, binding not splicing, execute-one!, update counts, commit,
  rollback, SQLSTATE, close, use from another task) run against SQLite now and PostgreSQL in D2; an **Exporter contract** (O3) against
  the console exporter and OTLP to the fake receiver.
- **Where they run.** Specs that need only libc are under `specs/` and run in the gate (`specs/log-spec.fib`, `specs/sql-spec.fib`).
  Specs that need a C library run from a script (`scripts/test-db.sh`: `LD_PRELOAD=libsqlite3.so.0 fibc test scripts/tests/db`, then the
  same spec built with `-l:libsqlite3.so.0`), as `scripts/test-http.sh` does for libcurl. A gate stage for them is open question 3.
- **Real services.** PostgreSQL 17 and the OpenTelemetry Collector (contrib) in docker for D2 and O3, started by the test script with a
  random port and removed by it (`docker run --rm`), or downloaded binaries under `~/.cache/fibber-scratch/tools` where docker is absent
  (the Mac). The fake OTLP receiver (an in-process `fib.http.server`) is the unit-test path; the collector is the integration path
  (its file exporter writes what it received, which the test compares).
- **Differential oracles.** HoneySQL for `fib.sql` (5.6); `psql` against the PostgreSQL driver for result decoding (D2); the published
  test vectors for `fib.crypto` (C1).
- **Planted faults** for every slice (section 8), kept as scripts so a reviewer reruns them.

## 7. Package plan

| Package | Contents | Size (lines) | Depends on | Parallel with |
|---|---|---|---|---|
| **L1** fib.log core (prototyped) | `Datum`, levels, records, Sink, LogSystem/Logger, macros, env and data config, text/JSON/memory sinks, spec + Appender contract | 450 (done: 380 + specs) | - | everything |
| **L2** appenders | FileSink with rotation, AsyncSink (bounded queue, drop counter, flush/shutdown), FanoutSink, guarded sink, run-time level atom | 400 | L1 (P-chan helps) | O1, C1, D1 |
| **O1** tracing core + context | ids, Span/SpanData, Tracer/Provider values, with-span, samplers, simple and batch processors, console exporter; measures span cost | 600 | L1 (Datum) | L2, C1, D1 |
| **O2** metrics | Meter/instruments over fib.adder, interned attribute sets, periodic reader | 450 | O1 | D2 |
| **O3** OTLP + propagation | OTLP/HTTP JSON exporter (traces, metrics, logs) via fib.http, traceparent in client and server middleware, the log bridge, Exporter contract, fake receiver, collector integration test | 700 | O1, O2, L1 | D2, D3 |
| **C1** fib.crypto | sha256, hmac, pbkdf2, base64, constant-time compare, random; published vectors | 350 | fib.os | anything |
| **D1** fib.db + SQLite (prototyped) | Driver/DataSource, prepare, statement cache, `plan` (reducible), with-connection, `execute!` on a SqlV, memcpy-based C strings, Driver contract | 600 (done: 230 + specs) | L1 (Datum) | O1, C1 |
| **S1** fib.sql format (prototyped) | the corpus grown to every HoneySQL clause and operator the docs list, Formatter value, upsert and PostgreSQL extensions, `:inline`, `:raw`, `:cast`, `:case` | 700 (done: 320 + specs) | D1 (Query) | D2 |
| **D2** PostgreSQL wire driver | startup, SCRAM-SHA-256, simple and extended protocol, types by oid, errors, cancel; Driver contract in docker | 1000 | C1, D1 | O2, S1 |
| **D3** pool + transactions | Pool, validation, timeouts, `try-transact` (task isolation), savepoints, `db.*` spans | 500 | D1, (O1 for spans) | O3 |
| **S2** fib.sql extension and properties | register clause/op on a Formatter, a random query generator differentially tested against HoneySQL | 400 | S1 | - |

Order: L1 and D1 first (users can log and use SQLite: this prototype is most of both), S1 next (small, independent, oracle ready),
then O1 and C1 in parallel, then D2, O2, L2, O3, D3. Four agents can run L2, O1, C1 and S1 at once. MySQL, protobuf OTLP, TLS
(`fib.tls`) and `fib.time` are later packages of their own.

## 8. The prototype

Files (each under 500 lines, functions under 50):

| File | Lines | What |
|---|---|---|
| `lib/fib/datum.fib` | 84 | `Datum`, `ToDatum`, text and JSON renderings |
| `lib/fib/log.fib`, `lib/fib/log/core.fib`, `lib/fib/log/sinks.fib` | 103, 117, 75 | fib.log |
| `lib/fib/db.fib`, `lib/fib/db/core.fib`, `lib/fib/db/sqlite.fib`, `lib/fib/db/sqlite/ffi.fib` | 37, 57, 104, 33 | fib.db and the SQLite driver |
| `lib/fib/sql.fib`, `lib/fib/sql/data.fib`, `lib/fib/sql/format.fib` | 61, 54, 209 | fib.sql |
| `specs/log-spec.fib`, `specs/log-appender-contract.fib`, `specs/sql-spec.fib` | | run by the gate |
| `scripts/tests/db/`, `scripts/test-db.sh` | | the Driver contract and SQLite, preloaded and linked |
| `scripts/tests/sql/`, `scripts/test-sql.sh` | | the HoneySQL differential test |

Checks as run (stage 2 at `~/.cache/fibber-scratch/obsdb/F`):

```
$ F test specs/log-spec.fib
total: 18 scenarios: 18 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)
$ F test specs/sql-spec.fib
total: 6 scenarios: 6 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)
$ FIBC=F scripts/test-db.sh
== fibc test (LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libsqlite3.so.0)
total: 12 scenarios: 12 pass, 0 fail, 0 trap, 0 timeout in 1 spec files (seed 1)
== built and linked with -l:libsqlite3.so.0
12 scenarios: 12 pass, 0 fail, 0 trap, 0 timeout (seed 1)
$ FIBC=F CLJ_CP=.. scripts/test-sql.sh
test-sql: 78 lines identical (26 queries x 3 dialects)
```

Planted faults (each applied alone with `sed`, run, restored; scripts in `~/.cache/fibber-scratch/obsdb/plant-*.sh`):

| Fault | Result |
|---|---|
| `enabled?` uses `>` | log spec: 12 pass, 4 fail, 1 trap |
| level macro not lazy | 15 pass, 2 fail |
| context dropped in `emit` | 12 pass, 5 fail |
| `"` not escaped in JSON | 16 pass, 1 fail |
| level prefix without the dot rule | 16 pass, 1 fail |
| sink failure not counted | 16 pass, 1 fail |
| ROLLBACK replaced by COMMIT | db spec (11 scenarios then): 10 pass, 1 fail |
| parameters bound one place off | 3 pass, 8 fail |
| update count always 0 | 9 pass, 2 fail |
| not-null mapped to 23000 | 9 pass, 2 fail |
| blobs not read | 10 pass, 1 fail |
| closed flag never checked | 10 pass, 1 fail |
| `execute-one!` returns the last row | 10 pass, 1 fail (the first version of the scenario had one row and passed: strengthened) |
| fib.sql: the four faults of 5.6 | 30, 12, 4 and 6 differing lines |

## 9. Open questions for the owner

1. **Macro name resolution.** Library macros can name their own module's functions only if the user `:use`s the module. Clojure's
   syntax-quote qualifies symbols to the defining namespace, so `(:require [fib.log :as log])` + `(log/info ..)` would work. Change the
   expander so a template's `m/x` resolves to module `m` whether or not the user named it (a language change in `compiler/expand`), or
   keep `:use` for macro libraries?
2. **`Datum` vs `Val`.** Ship `fib.datum` now as the scalar value of these libraries, and make `Val` (when designed) a superset that
   embeds it? Or design `Val` first?
3. **C-library specs in the gate.** A gate stage that runs `scripts/test-db.sh` (needs libsqlite3.so.0 on CI runners), or keep it a
   separate script like `test-http.sh`? Same question for docker-based PostgreSQL and collector tests.
4. **A user sink that traps.** Accept "a sink must not trap" as a documented rule (the library's sinks do not), or pay a task per
   write for isolation until catch exists?
5. **Context as a value** (2.4 option A) rather than a task-local slot: confirm, knowing that every function that logs takes a logger.
6. **`format` shadowing** `fib.print/format` in modules that `:use` fib.sql: acceptable, or rename to `sql-format` (HoneySQL's name is
   `format`)?

## 10. Not done

- No `fib.otel` code: sections 3 and 6.2's span cost are design and estimate only.
- No PostgreSQL driver, no `fib.crypto`, no pool, no `plan`, no prepared-statement cache, no `try-transact`; no containers were started
  by this work (docker works; the host runs other, unrelated containers, which were not touched).
- No file or async sink, no OTel log bridge.
- fib.sql: only the clauses in the corpus; no `:inline`, `:cast`, `:case`, upsert, `Formatter` registry, or format timing.
- No stdlib rows in `spec/stdlib.md` for the new modules (the libraries are explicit, not implicit; the rows come with L1/D1 proper).
