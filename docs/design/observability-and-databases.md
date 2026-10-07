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

### 2.6.1 L2 as built (2026-10-06)

`Sink` gained `sink-flush` and `sink-close`; `LogSystem` gained `flush-system`, `close-system` and `with-log-system` (closes on any exit).
Files: `lib/fib/log/file.fib`, `async.fib`, `route.fib`, `config.fib`; spec `specs/log-sinks-spec.fib` (30 scenarios, the Appender contract
against the file sink, the async sink over a file and a fanout); planted faults `scripts/mutant-obs.sh all '^(file-|async-|fanout-|filter-|env-)'`
(14 modes, each killed); measurements `scripts/bench/obs-log.fib`.

- **File sink** (`file-sink path max-bytes keep json?`): appends under a spin lock (a scalar atom; the section is a few system calls), rotates
  before a line that would pass `max-bytes` (`path.(K-1)` over `path.K` ... `path` over `path.1`, each `rename(2)`; a file never holds a partial line;
  a line longer than the limit goes whole into a file of its own). A write or rotation failure is an `Err` (counted and reported once by the
  logger); a trap inside the section is caught (`try`) and the lock released, so the caller never traps (`/dev/full` is the test).
- **Async sink** (`async-sink inner capacity overflow`): a chain of nodes in an atom under a spin lock (an enqueue is one node; the first version
  held a persistent vector in the atom and cost 2 us per enqueue at 4,000 queued, because every enqueue retained every record of a path) and one
  worker task. Policies `DropNewest`, `DropOldest` (O(capacity) per drop), `BlockProducer`; drops are counted. `sink-flush` waits for
  `written == accepted`; `sink-close` drains, joins the worker and closes the inner sink; a write after that is an `Err`. The inner sink runs under
  `try`: an `Err` or a trap is counted (`async-failed`), reported once, and the worker goes on (spec: a sink that traps).
- **Fanout, level filter**: `fanout` writes to every sink and isolates each with `try` (the first failure is the answer, the others still got the
  record); `level-filter` gives a sink its own level.
- **Configuration**: `FIB_LOG=info,my.mod=debug` (a bare level is the root, `k=level` a rule; FIB_LOG_LEVEL(S) still work), `FIB_LOG_FORMAT`,
  `FIB_LOG_FILE`, `FIB_LOG_FILE_MAX`, `FIB_LOG_FILE_KEEP`, `FIB_LOG_ASYNC`, `FIB_LOG_QUEUE`, `FIB_LOG_OVERFLOW`; the data form is
  `(system-from-map {"level" "info" "file" ".." "async" "true" ..})` with an `Err` that names the bad key.
- **The runtime waits for tasks.** A program whose `main` returns while an async sink's worker is running does not exit (measured: a batch
  processor left running, `timeout 30` killed it). There is no finalizer, so `with-log-system` (and `with-provider`, `with-periodic-reader`)
  close on every exit; an async sink must be closed.

Measured (`scripts/bench/obs-log.fib`, 300,000 calls x 3 rounds, `info` with two fields, this shared machine, `ns/op`):

| Sink | ns/op |
|---|---|
| level off (loop alone) | 0 (hoisted) |
| no-op sink | 121 to 130 (design 2.5: 82 to 107; this loop has two fields, and the program now catches, see below) |
| file sink, JSON lines, one `write(2)` each | 1,558 to 1,578 |
| async over the file, queue 1M, `BlockProducer` | 484 to 820 (the queue grows to the length of the run: the worker, at ~1.5 us per line, is slower than the producer) |
| async over the file, queue 4096, `DropNewest` | 251 to 276 (87 % of 900,000 records dropped, the rest written; `written + dropped == 900000` is checked) |
| async over the no-op sink, queue 8192, `BlockProducer` | 1,746 to 1,779 |

The last row is worse than the synchronous file sink, and `strace -c` says why: 18,630 `futex` calls for 60,000 records. A record is allocated by the
producer and freed by the worker, and glibc's arena lock is contended by the two threads (`MALLOC_ARENA_MAX=2` helps by a quarter, a larger
`tcache_count` does not). So the async sink buys isolation from a slow or blocking disk (the caller's cost no longer depends on the write) and
not a lower cost per call; where the file is the bottleneck, `DropNewest` bounds the caller's cost at ~255 ns. A cross-thread free is the
runtime's, not the sink's; a per-thread allocator or a record pool is the lever, not measured here.

A program that contains `try` (every user of the file sink, the async sink, a fanout or fib.otel) is compiled with the catching convention
(docs/design/exceptions.md 9: -5 to +14 % on the happy path, measured there), which is part of the no-op sink's 121 to 130 ns; a program that
only uses the console sinks of L1 is unchanged.

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
- **OTLP/HTTP with JSON** (`/v1/traces`, `/v1/metrics`, `/v1/logs`) through `fib.http.client` (native; plain http until the TLS package, then https). JSON first
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

Measured in O1, see 3.7: a sampled span start and end is 290 to 420 ns with an exporter that only counts and 560 ns to the in-memory exporter,
against the estimate of 100 to 150 ns; a sampled-out span is 72 ns and a no-op tracer 54 ns.

### 3.7 O1 and O2 as built (2026-10-06)

Files `lib/fib/otel.fib` (the facade and the `in-span` macro) and `lib/fib/otel/*.fib`:

| Module | What |
|---|---|
| `ids` | `TraceId` (two i64), span ids (i64), `IdGen`: one atom with a SplitMix64 state seeded from `getentropy` when the generator is made; an id is the mixed output of an atomic add, so tasks never draw one state twice and a draw is ~10 ns. Not secret (invertible from one output). A draw of 0 is skipped (a test seeds the state that mixes to 0) |
| `propagation` | `SpanContext`; `parse-traceparent` and `parse-tracestate` (W3C rules: lower-case hex only, version `ff` invalid, `00` exactly 55 characters, a future version read from the front, 32 members, key and value grammar, no duplicates) and `format-*`; a malformed header is `nil`, a malformed tracestate drops only itself |
| `trace` | `SpanKind` (`KindServer` ..: variant names are global, so prefixed), `SpanStatus`, `SpanData`, `Resource`, `Context`; the O3 hooks `inject-headers` and `extract-context` |
| `sampler` | `Sampler` protocol; `always-on`, `always-off`, `trace-id-ratio` (low 63 bits of the id's low half against ratio x 2^63), `parent-based` |
| `export` | `Exporter` protocol, console text and JSON, in-memory; `span-json` |
| `queue`, `processor` | the generic bounded queue (L2's design); `simple-processor`; `batch-processor exporter capacity max-batch interval-ns overflow` |
| `tracer` | `TracerProvider` (resource, sampler, processors, ids, a wall and monotonic clock pair), `Tracer`, `Span`, `with-span` / `in-span` / `with-span-kind` / `with-span-result`, `start-span-full`, `end-span!` (exactly once, a compare-and-set), `record-exception!`, `with-provider` |
| `semconv`, `log` | attribute-name constants and helpers (`http-status`, `db-attributes`: the query text, never the values); `logger-with-trace` |
| `metrics`, `reader` | O2: `MeterProvider`, `Meter`, `Counter`, `UpDownCounter`, `Histogram`, `observable-gauge`, `collect-metrics`, `periodic-reader`, the metric exporters |

**The idiom for tasks.** The Context is a value; a task gets it by capture, exactly as a logger does:

```clojure
(otel/in-span sp ctx tracer (otel/empty-context) "request" []
  (with-tasks [s]
    (do (dotimes (t 8) (fork s (fn () (otel/in-span sp2 ctx2 tracer ctx (str "task-" t) [] (work ctx2)))))
        ())))
```

`specs/otel-trace-spec.fib` checks it for `with-tasks` (8 children and their children: the right parents, one trace, 17 distinct span ids), for
`pmap`, and that a task that does not capture the context starts a new trace.

**Spans that fail.** `with-span` runs the body under `try`: a trap or a throw is recorded (status error, an `exception` event with
`exception.type` from `ex-kind` and `exception.message`), the span is ended, and the failure goes on (spec: the caller's `catch` receives it; a
nested span marks every span it passes through). Processor and exporter calls are isolated the same way; an exporter's `Err` or trap costs the
batch.

**Facts that surprised.** `subs` counts characters while `str-find`, `str-len` and `str-slice` count bytes: three places mixed them and
trapped on a non-ASCII input (found by the tracestate and the log-rules specs; fixed with `str-slice`). `filter` returns a lazy sequence, not a
`Vec`. A variant name used in a `match` through an alias needs the alias (`otel/SampleDrop`). A spec scenario that leaves a worker task running
hangs the runner, for the reason in 2.6.1.

**Cost.** `scripts/bench/obs-trace.fib`, 300,000 spans x 3 rounds, `ns/op` for start plus end:

| Case | ns/op | Notes |
|---|---|---|
| no-op tracer | 53 to 59 | no id, no clock |
| sampled out (`always-off`) | 71 to 77 | ids drawn, no state |
| sampled in, simple processor, counting exporter | 287 to 423 | the cost of the span itself |
| sampled in, in-memory exporter | 562 to 563 | the exporter conj's onto a vector held by an atom |
| sampled in, batch processor (queue 4096, drop-newest) | 336 to 353 | 226,022 exported, 673,978 dropped: the worker is slower than the producer |
| root and child per iteration | 420 to 567 | per span about half |
| 2 attributes at start, one `set-attribute!`, one event | 575 to 584 | |

Allocations per sampled span with the counting exporter: **18** (`F run --trace`: 54,666 allocations for 3 x 1,000 spans and 109,180 for
3 x 2,000, so 18.17 per span). The span costs 2 to 3 times the estimate of 3.6. Where it goes, from the allocation list: the two contexts, the
17-field `SpanData` and its copy at `end-span!` (the data lives in an atom), two atoms, the span state, the `Span`, the closure of `try` per
processor call and the one-span vector the simple processor exports. The levers, none pulled: the end time in a scalar atom and one `SpanData`
built at the end, an `on-end` that borrows the span, and no per-call `try` closure on the simple processor. The budget of 150 ns (6.2) is
therefore **not met** for a sampled span; it is met for a no-op (54 ns) and a sampled-out (72 ns) span.

O2 (`scripts/bench/obs-metrics.fib`; `ns/op` is wall time / (tasks x records), 3 rounds, 300,000 records per task):

| Case | 1 task | 8 tasks |
|---|---|---|
| `fib.adder` `inc!` alone | 7 to 8 | 2 |
| plain shared atom, `swap!` | 7 | 27 to 32 |
| Counter add, no attributes | 12 to 17 | 11 |
| Counter add, bound series (`bind-counter`) | 10 | not run |
| Counter add, two attributes (canonicalise, key, look up) | 971 to 1,079 | 760 to 1,013 |
| Histogram record, no attributes (15 bounds) | 37 to 54 | 56 to 64 |
| Histogram record, one attribute | 306 to 314 | not run |

A counter without attributes is within 5 ns of the adder alone and 2.8 times faster than a shared atom at 8 tasks. An attribute set costs
about 1 us (the canonical sort, the key string, the map lookup): a hot path binds the series once. Found while measuring: the first version
returned the series from a helper, which retains a shared object with an atomic increment per add, and 8 tasks cost 81 ns each; reading the
fields by borrow all the way gave 11 ns. The same trap awaits any hot path that returns a shared struct.

### 3.8 O3 (not built): where the exporter plugs in, and the OTLP JSON mapping

**Hooks that exist.**

| For O3 | Hook |
|---|---|
| a span exporter | implement `Exporter` (`export-spans`, `exporter-flush`, `exporter-shutdown`); the batch processor calls it from its worker task, so it may block on the network, and an `Err` or trap is counted and never reaches the program. It holds a `fib.http.client` value, an endpoint and headers (`OTEL_EXPORTER_OTLP_ENDPOINT`, `_HEADERS`) |
| a metric exporter | implement `MetricExporter`; `periodic-reader` calls it from its task |
| a log exporter | a `Sink` (fib.log) that maps `LogRecord` to a log record below and hands batches to an async sink; `logger-with-trace` already puts `trace_id` and `span_id` into the fields, and the sink moves them into the record's own fields |
| propagation, client | `(inject-headers ctx)` gives the `traceparent` and `tracestate` header pairs; the middleware starts a `KindClient` span with `start-span-full` (attributes from `http-client-attributes`), injects the headers of `span-in-context`, sets `http-status` from the response, ends the span |
| propagation, server | `(extract-context header-lookup)` takes a function `str -> (Option str)`, so any request type works; the middleware starts a `KindServer` span as its child (`parent-remote` is true) and puts the Context where the handler can read it (a field of the request or a pair, to be settled with the http owner) |
| the Exporter contract | `specs/otel-exporter-contract.fib`: an `ExporterProbe` (the exporter and a function returning the spans it produced, one JSON text each); O3 adds a probe over the fake receiver |

**OTLP/HTTP JSON** (`POST {endpoint}/v1/traces|metrics|logs`, `Content-Type: application/json`; the proto3 JSON mapping with OTLP's two
exceptions: ids are lower-case hex strings, not base64, and enums are integers). 64-bit integers are strings in JSON.

| fibber | OTLP JSON |
|---|---|
| `Datum` `DStr s` / `DBool b` / `DInt i` / `DFloat f` / `DBytes b` / `DNil` | AnyValue `{"stringValue":s}` / `{"boolValue":b}` / `{"intValue":"i"}` / `{"doubleValue":f}` (NaN and infinities as the strings `"NaN"`, `"Infinity"`) / `{"bytesValue":base64(b)}` / the attribute is omitted |
| attribute `(Pair k d)` | `{"key":k,"value":AnyValue}` in an `attributes` array |
| `Resource` | `"resource":{"attributes":[..]}` |
| `SpanData` batch | `{"resourceSpans":[{"resource":R,"scopeSpans":[{"scope":{"name":scope},"spans":[S..]}]}]}`, spans grouped by resource and scope |
| `SpanData` | `{"traceId":hex32,"spanId":hex16,"parentSpanId":hex16 (omitted for a root),"traceState":tracestate,"name":..,"kind":N,"startTimeUnixNano":"ns","endTimeUnixNano":"ns","attributes":[..],"events":[..],"links":[..],"status":{"code":N,"message":..},"flags":N}` |
| `SpanKind` | KindInternal 1, KindServer 2, KindClient 3, KindProducer 4, KindConsumer 5 (0 is unspecified) |
| `SpanStatus` | StatusUnset 0, StatusOk 1, StatusError 2 with `message` (the message only for error) |
| `SpanEvent` | `{"timeUnixNano":"ns","name":..,"attributes":[..]}` |
| `SpanLink` | `{"traceId":..,"spanId":..,"traceState":..,"attributes":[..]}` |
| `parent-remote` | `flags` bit 8 (has-is-remote, 0x100) and bit 9 (is-remote, 0x200) on spans and links |
| `Metric` | `{"name":..,"description":..,"unit":..}` plus one of `sum`, `histogram`, `gauge`; grouped `{"resourceMetrics":[{"resource":R,"scopeMetrics":[{"scope":{"name":..},"metrics":[..]}]}]}` |
| `SumData` | `"sum":{"dataPoints":[{"attributes":[..],"startTimeUnixNano":"ns","timeUnixNano":"ns","asInt":"v"}],"aggregationTemporality":2,"isMonotonic":b}` (2 = cumulative, what the readers produce) |
| `HistogramData` | `"histogram":{"dataPoints":[{"attributes":[..],"startTimeUnixNano":..,"timeUnixNano":..,"count":"n","sum":f,"bucketCounts":["c0",..],"explicitBounds":[b0,..],"min":f,"max":f}],"aggregationTemporality":2}` (`bucketCounts` has one more entry than `explicitBounds`, as `counts` and `bounds` do) |
| `GaugeData` | `"gauge":{"dataPoints":[{"attributes":[..],"timeUnixNano":..,"asDouble":f}]}` |
| `LogRecord` | `{"resourceLogs":[{"resource":R,"scopeLogs":[{"scope":{"name":logger},"logRecords":[{"timeUnixNano":"ns","observedTimeUnixNano":"ns","severityNumber":N,"severityText":"INFO","body":{"stringValue":msg},"attributes":[..],"traceId":hex32,"spanId":hex16}]}]}]}`; severity TRACE 1, DEBUG 5, INFO 9, WARN 13, ERROR 17, FATAL 21 |

**Behaviour to implement**: a 200 with `partialSuccess` is logged and counted, not retried; 429, 502, 503 and 504 are retried with exponential
backoff and `Retry-After`; other 4xx drop the batch; a timeout is 10 s; the worker task is the only caller, so nothing blocks the application. The
collector integration test starts the collector with a file exporter (6.3) and compares ids, parents, attributes, status and metric totals with the
in-process spans.

## 4. `fib.db`

### 4.1 Use (next.jdbc's shape)

```clojure
(ns app.store (:require [fib.db :as db] [fib.sql :as sql] [sqlite :as sqlite]))   ; sqlite: the library fib-db-sqlite (4.10)
(db/with-connection [conn (sqlite/open-sqlite "/var/app/data.db")]
  (db/with-transaction [tx conn]
    (try-let [_ (db/execute! tx ["insert into people (name, age) values (?, ?)" name age])
              r (db/execute-one! tx ["select id from people where name = ?" name])]
      (Ok r))))
(db/execute! conn (sql/format (sql/sql {:select [:id :name] :from [:people] :where [:> :age min-age]})))
(reduce f init (db/plan conn ["select id, name from people"]))      ; rows one by one, nothing materialised
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
the macro becomes a convenience. A trap between open and close no longer abandons the handle: see 4.10 (`with-connection`).

### 4.6 Transactions, honestly

`(with-transaction [tx conn] body)`: BEGIN; the body returns a `Result`; `Ok` commits (a failed COMMIT rolls back and returns its
error), `Err` rolls back. Two scenarios check both (planted fault "rollback commits" fails one). A trap in the body: when this section
was written there was no unwinding, so `transact` could not run ROLLBACK; since exceptions stage 2 (fibber v0.1.8) it does, see 4.10. The
options considered then:

- a. **`try-transact`** (D3): runs the body in a task with the connection moved in, `try-join`s it; on `Err (Trap ..)` the parent,
  which kept the driver's raw handle (an i64), issues ROLLBACK and closes it. The task's fibber objects are abandoned (the stage 1
  rule), the database handle is not. Sound because the task has finished before the parent touches the handle. Costs one thread start
  (~20 us) per transaction, so it is opt-in.
- b. Do nothing: PostgreSQL rolls back a transaction whose connection closes; SQLite's journal rolls back on the next open. The process
  ends on a trap in `main` anyway.
- c. Wait for stage 2 of exceptions (catch) and make `transact` unwind-safe then.

Proposal then: b by default (documented), a as `try-transact` in D3, c when it exists. c exists; a is not needed.

### 4.7 Drivers

- **SQLite** (D1; since 2026-10-07 the library fib-db-sqlite, 4.10): the system `libsqlite3.so.0` through 23 externs (`sqlite.ffi`); handles are i64 as
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

### 4.10 D1 as built, and the split of the SQLite driver (2026-10-07)

- **Where things are.** The core (`fib.db`, `fib.db.core`), the Driver contract (`fib.db.contract`) and an in-memory fake driver
  (`fib.db.memory`) are in fibber's library; their specs (`specs/db-*-spec.fib`) need no C library and run in the gate. The SQLite driver
  is the library `fib-db-sqlite` in its own repository, `ssh://git@localhost:2222/tailoredshapes/fib-db-sqlite.git` (modules `sqlite`,
  `sqlite.ffi`, `sqlite.stmt`; not `fib.db.sqlite`: a project's module may not have the name of a module of the bundled library, and the
  v0.1.8 release bundles `fib.db.sqlite`), pulled in with `fibc deps add`. It runs the contract from `fib.db.contract` against itself.
  Open question 3 is answered by this: C-library specs run in the driver's repository, not in fibber's gate.
- **Traps.** `with-transaction` rolls back on a trap and re-raises it (contract scenario; case 7891). `with-connection` closes exactly
  once on Ok, Err and trap (`specs/db-core-spec.fib`, planted: no close in the catch, a double close).
- **`plan`.** `Connection` has a fourth method `conn-each` (rows to a callback, stop on false; the default walks `conn-execute`'s vector).
  `(plan conn q)` is a `Reducible` of `(Result row DbError)`; reduce, run!, take, first and count work on it. SQLite overrides `conn-each`
  with a cursor and releases the statement on every exit, a trap in the consumer included. Measured in the driver's spec: 100,000 rows
  through `plan` leave the peak resident set (VmHWM) unchanged; `execute!` of the same rows raises it by 48 MB.
- **Statement cache.** A bounded (32) LRU per connection, keyed by SQL text; a statement in use is busy, so the same text inside a walk
  gets its own; DDL is not kept and drops the free ones. Measured as 4.9 measured it (200,000 operations, `:memory:`, three runs):

| Operation | before (us/op) | after (us/op) | C, prepare per call | C, statement kept |
|---|---|---|---|---|
| insert, autocommit | 2.11, 1.79, 1.81 | 1.08, 1.01, 1.01 | 1.43 to 1.57 | 0.71 |
| insert, one transaction | 1.44, 1.39, 1.39 | 0.64, 0.61, 0.60 | 1.00 to 1.04 | 0.30 |
| select one row by id | 2.72, 2.77, 2.69 | 0.88, 0.85, 0.81 | 1.59 to 1.73 | 0.40 |
| select 1 | 1.15, 1.15, 1.34 | 0.22, 0.23, 0.23 | | |

  The target (within 20% of the C of 4.9, which prepares per call) is met with room: 40 to 50% below it. Against a C that also keeps its
  statement the driver is about twice: what remains is a `Map` and a `Datum` vector per call and text copied byte by byte (fibber has no
  pointer to an array's data, so no `memcpy`).

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
oracle: the first version replaced). The helpers are macros because their arguments are HoneySQL data. Since MACRO-NS a template's names resolve in the module that defines
the macro, so the library is used as HoneySQL is, `(:require [fib.sql :as sql])` and `(sql/format ..)`, `(sql/sql {..})`,
`(-> (sql/select ..) (sql/from ..))`; nothing needs `:use`, and `format` no longer shadows `fib.print/format` anywhere (a module that
wants both writes `sql/format`).

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
| log call, level on, null sink | 100 ns | 83 to 107 ns (2.5); 121 to 130 ns in a catching program; file sink 1.6 us; async 250 ns (drop) to 1.8 us (2.6.1) |
| task-local context read (option B) | n/a | 1.1 to 1.2 ns (2.4) |
| span start and end, sampled | 150 ns | **290 to 420 ns** with a counting exporter, 560 ns in memory (3.7): not met; no-op 54 ns, sampled out 72 ns |
| counter add (adder) | 5 ns uncontended | 12 to 17 ns one task, 11 ns at 8 tasks (a shared atom: 27 to 32 ns); with attributes ~1 us, bound 10 ns (3.7) |
| SQLite insert, in memory | within 20% of C (D1) | 1.35 to 1.98 us vs 0.94 to 1.59 us in C (4.9) |
| SQLite select by key | within 20% of C (D1) | 2.64 to 3.23 us vs 1.55 to 1.66 us in C (4.9) |
| PostgreSQL round trip | server-bound | D2 |
| `fib.sql/format` of a 10-clause query | 2 us | not measured (S1) |

### 6.3 Testing strategy

- **fib.test specs and contracts.** An **Appender contract** (`specs/log-appender-contract.fib`: every record once and whole, 100
  records from 4 tasks arrive whole, fields in order) run against the memory sink and JSON lines through a pipe; a **Driver contract**
  (`lib/fib/db/contract.fib`, 11 scenarios: types round trip, binding not splicing, execute-one!, plan, update counts, commit,
  rollback, rollback on a trap, SQLSTATE, close, use from another task) run against the memory fake in fibber's gate, against SQLite
  in its repository and against PostgreSQL in D2; `specs/db-contract-spec.fib` shows it can fail (4.10); an **Exporter contract** (O3) against
  the console exporter and OTLP to the fake receiver.
- **Where they run.** Specs that need only libc are under `specs/` and run in the gate (`specs/log-spec.fib`, `specs/sql-spec.fib`).
  Specs that need a C library run from a script (`scripts/test-http.sh` for libcurl; the SQLite driver's `scripts/test.sh` in its own
  repository: `LD_PRELOAD=libsqlite3.so.0 fibc test`, then the same spec built with `-l:libsqlite3.so.0`). Open question 3 is answered: no C
  library in fibber's gate.
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

**Status (2026-10-06):** L1, L2, O1 and O2 are built (2.6.1, 3.7); O3's hooks are listed in 3.8. Order: L1 and D1 first (users can log and use SQLite: this prototype is most of both), S1 next (small, independent, oracle ready),
then O1 and C1 in parallel, then D2, O2, L2, O3, D3. Four agents can run L2, O1, C1 and S1 at once. MySQL, protobuf OTLP, TLS
(`fib.tls`) and `fib.time` are later packages of their own.

## 8. The prototype

(As of 2026-10-07 the SQLite files and `scripts/test-db.sh` listed below live in the fib-db-sqlite repository, 4.10; this section is the record of the prototype.)

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
   **Answered (owner, 2026-10-06): change the expander** (MACRO-NS, docs/design/macro-names.md, spec/syntax.md 3.16 "Names in a
   template"). fib.log, fib.db and fib.sql now name their own functions bare in their templates and work through `:require` alone.
2. **`Datum` vs `Val`.** Ship `fib.datum` now as the scalar value of these libraries, and make `Val` (when designed) a superset that
   embeds it? Or design `Val` first?
3. **C-library specs in the gate.** **Answered: no, they run in the driver repository (4.10).** A gate stage that runs `scripts/test-db.sh` (needs libsqlite3.so.0 on CI runners), or keep it a
   separate script like `test-http.sh`? Same question for docker-based PostgreSQL and collector tests.
4. **A user sink that traps.** Accept "a sink must not trap" as a documented rule (the library's sinks do not), or pay a task per
   write for isolation until catch exists?
5. **Context as a value** (2.4 option A) rather than a task-local slot: confirm, knowing that every function that logs takes a logger.
6. **`format` shadowing** `fib.print/format` in modules that `:use` fib.sql: acceptable, or rename to `sql-format` (HoneySQL's name is
   `format`)?
   **Resolved by MACRO-NS: not forced.** `(:require [fib.sql :as sql])` and `sql/format`; `fib.sql` keeps HoneySQL's name.

## 10. Not done

- `fib.otel` O3 is not built: no OTLP exporter, no `traceparent` middleware in `fib.http` (3.8 lists the hooks and the JSON mapping). No
  baggage in the Context, no delta temporality, no views, no exemplars, no synchronous gauge, no instrument registry by name (a name
  registered twice is two instruments).
- A sampled span costs 2 to 3 times its 150 ns budget, and an async sink does not lower the caller's cost per call (3.7, 2.6.1).
- No PostgreSQL driver, no `fib.crypto` in this library (the protocol is in `fib.crypto`, drivers in their own repos), no pool, no `try-transact` (not needed since catch exists); `plan`, the statement cache and `with-connection` are done (4.10); no containers were started
  by this work (docker works; the host runs other, unrelated containers, which were not touched).
- No OTLP log exporter; log-to-trace correlation is `logger-with-trace` only. No daily rotation (size only), no compression of archives.
- fib.sql: only the clauses in the corpus; no `:inline`, `:cast`, `:case`, upsert, `Formatter` registry, or format timing.
- No stdlib rows in `spec/stdlib.md` for the new modules (the libraries are explicit, not implicit; the rows come with L1/D1 proper).
