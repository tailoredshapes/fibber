# Logging, traces and metrics

Fibber's observability libraries are explicit values. A program owns its logger,
tracer provider, metric reader and exporters, passes context to the work that
needs it, and shuts those resources down. Importing a module starts no workers.

- `fib.log` provides structured fields, logger levels/context, console and rotating
  file sinks, routing and bounded asynchronous delivery. Disabled level macros
  do not evaluate messages or field expressions.
- `fib.otel` provides spans, sampling, W3C trace context, metrics, processors,
  periodic readers and trace-correlated logging.
- `fib.otel.http` adds opt-in HTTP tracing without changing HTTP request types.
- `fib.otel.otlp` exports traces, metrics and logs using OTLP/HTTP JSON.

The [library reference](../reference/library/INDEX.md) links declarations to their
full source signatures. These modules are available in the source library;
the previously published 0.1.21 bundle predates the OTLP and HTTP-wrapper additions.

## Export traces

Build an exporter, give it to a batch processor, and give that processor to a
provider. The provider owns the processor's lifetime:

```fib check
(ns main (:use fib.core fib.seq fib.coll)
  (:require [fib.otel :as otel] [fib.otel.otlp :as otlp] [fib.log :as log]))

(defun main () -> i64
  (let [exporter (otlp/trace-exporter
                   (otlp/options "http://127.0.0.1:4318/v1/traces"))
        processor (otel/batch-processor (dyn otel/Exporter :send exporter)
                    2048 256 1000000000 log/DropNewest)
        provider (otel/tracer-provider (otel/resource-from-env)
                   (otel/parent-based (otel/always-on))
                   [(dyn otel/SpanProcessor :send processor)])]
    (otel/with-provider provider
      (fn (p: otel/TracerProvider)
        (otel/in-span span ctx (otel/get-tracer p "billing")
          (otel/empty-context) "invoice" [] 0)))))
```

`with-provider` drains and shuts down on return or a caught failure. Export
failures are counted and isolated by the processor. A synchronous exporter used
directly, or through `simple-processor`, performs I/O in its caller; use the batch
processor for application request paths.

For metrics, pass `otlp/metric-exporter` to `otel/periodic-reader`. For logs,
`otlp/log-sink options resource capacity overflow` returns a `fib.log/Sink` with
its own background batch worker. Logs are sent in batches of at most 256 records,
with a 100 ms collection interval and explicit flush/close triggers. Queue
capacity is separate from the in-flight bounded drain. `stats`, `log-dropped` and
`log-failed` expose delivery diagnostics.

## Configuration and transport

`otlp/options` takes an exact signal URL. `otlp/options-from-env "traces"`
(`"metrics"` or `"logs"`) returns a `Result` and follows these environment settings:

| Generic setting | Signal override example |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` |
| `OTEL_EXPORTER_OTLP_HEADERS` | `OTEL_EXPORTER_OTLP_METRICS_HEADERS` |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | `OTEL_EXPORTER_OTLP_LOGS_TIMEOUT` |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `OTEL_EXPORTER_OTLP_TRACES_PROTOCOL` |
| `OTEL_EXPORTER_OTLP_COMPRESSION` | `OTEL_EXPORTER_OTLP_LOGS_COMPRESSION` |

The generic endpoint defaults to `http://localhost:4318` and gets a `/v1/SIGNAL`
suffix; a signal endpoint is used as-is. Empty values behave as unset. Headers
use comma-separated keys and percent-decoded values. The default overall export
deadline is 10,000 ms. This implementation accepts `http/json` and no compression;
unsupported protocols/compression are errors rather than silent fallbacks.

Traces preserve hex IDs, parents, flags, status, events, links, resources and
scopes. Integer/timestamp JSON fields preserve 64-bit precision as strings;
byte values use base64. Metrics use cumulative temporality. Log correlation IDs
are promoted into OTLP fields only when valid.

The transport uses a raw, uninstrumented HTTP client. Collector requests cannot
recursively generate their own HTTP spans or application logs. HTTP 429, 502,
503 and 504 and transient transport failures get bounded retries/backoff;
`Retry-After` is never shortened to retry early. A delay beyond the remaining
budget terminates the export. Partial acceptance is counted and never retried.
An empty/default-only `partialSuccess` is a full success.

Native DNS and dial budgets are constrained by the remaining export deadline.
Unbounded system/custom resolvers are refused. HTTPS requires an explicitly
configured TLS factory in the options; the library does not install a provider
or weaken verification. MeshQL's deployment adapter supplies its existing TLS
factory and certificate configuration.

## Instrument HTTP with explicit context

`fib.otel.http/wrap-server` adapts a handler taking `(Context Request)` into an
ordinary HTTP handler. `wrap-server-route` also takes a declared route template.
`request` wraps a client send function, starts a client span and injects that
span's context into a copy of its request.

The server validates incoming `traceparent`/`tracestate`; malformed context starts
a new trace. The client replaces stale propagation headers. Tasks capture their
parent context explicitly. No global or task-local current-request slot exists.
Sampling decisions propagate even when a span is not recorded.

Default attributes contain method, response status and an explicitly supplied
route template. URLs, query strings, bodies and arbitrary headers are not
automatically copied into telemetry. Application-provided messages, exception
text and attributes remain application data and need appropriate care.

## Flush and stop

Stop accepting application work before shutting down providers, metric readers
and log sinks. Flush is a delivery barrier, not a durability guarantee from a
collector. Log flush captures a fixed accepted-work snapshot and does not hold
the producer lock while waiting for HTTP. Drop-newest producers stay independent
of collector I/O and flush waits.

Shutdown drains queued work and joins workers. Multiple queued batches can each
consume an export budget, so a drain can take longer than one request timeout.
Forced process termination cannot guarantee delivery of buffered telemetry.

## Executable checks

```sh
make observability
# Or use an already-built compiler and a reusable result store:
OTLP_FIBC=/path/to/fibc OTLP_TEST_OUT=/path/to/results scripts/test-otlp.sh
```

Make describes compiled fixtures and their dependent result files. Rebuilding a
fixture removes its old result first. Unchanged passing results are reused.
The full gate already runs the unit specs and adds the TCP/collector integration
results, avoiding a second unit-spec pass.

The official collector image is pinned by digest in
[`scripts/otel-collector-image`](../../scripts/otel-collector-image). Pull that
image to enable the real collector target. An unavailable image produces an
explicit skipped result, never a claim of collector acceptance. The collector
fixture verifies traces, metrics and logs, including typed integer/byte values;
its wrong-route control must fail without producing telemetry. TCP fault tests
exercise retries, deadlines, partial success and malformed responses. Scratch
mutations check that propagation/status/producer-lock assertions can fail.
