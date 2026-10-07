# HTTP (HTTP-1): the native client and server

Measured 2026-10-07 on the development box (28 cpus, **not quiet**: load average 18 to 25 from other agents' jobs during the run), seed fibc 0.1.10,
`fibc build` at -O2, loopback, medians of 5 runs of `scripts/bench/http-bench.py` (`FIBC=fibc scripts/bench/http-bench.py`). The Ryzen box was not used.
The load generator is fibber's own (`scripts/bench/http-load.fib`: one task and one client per connection, the native client, every latency kept); the
reference server is python3's `http.server` (ThreadingHTTPServer, HTTP/1.1; its handler does one buffered write per response, because unbuffered
headers then body cost it a delayed ACK of 40 ms per request, which measured Nagle and not Python). `ab`, `wrk` and `hey` are not installed, and a C
reference was not written. Nothing was tuned: these are the numbers of the first correct implementation. They are noisy (the native 100 B single
connection row read 24.6k, 24.0k and 26.8k requests/s in three earlier partial runs and 15.6k in the final one); read them as an order of magnitude.

## Keep-alive GET, fibber client as load

| server | body | conns | requests | rps | p50 us | p90 us | p99 us |
|---|---|---:|---:|---:|---:|---:|---:|
| native | 100 B | 1 | 4000 | 15586 | 51 | 106 | 167 |
| native | 100 B | 64 | 32000 | 321608 | 140 | 264 | 439 |
| native | 1 MiB | 1 | 100 | 660 | 1379 | 2112 | 3359 |
| native | 1 MiB | 64 | 768 | 5049 | 8401 | 16007 | 58231 |
| python http.server | 100 B | 1 | 4000 | 22961 | 40 | 43 | 91 |
| python http.server | 100 B | 64 | 32000 | 22168 | 1594 | 5205 | 12082 |
| python http.server | 1 MiB | 1 | 100 | 1125 | 820 | 984 | 2462 |
| python http.server | 1 MiB | 64 | 768 | 6997 | 3047 | 6000 | 72115 |

## 10,000 sequential GETs on one keep-alive connection

| 10,000 sequential GETs, one keep-alive connection | server | seconds (median) |
|---|---|---:|
| curl CLI (URL globbing) | native | 0.40 |
| native client | native | 0.38 |
| curl CLI (URL globbing) | python http.server | 0.45 |
| native client | python http.server | 0.53 |

curl does its 10,000 requests in one process with URL globbing (`/small?n=[1-10000]`); the native client is `http-load URL 1 10000` (the time includes
process start, about 5 ms for both).

## Memory

memory: native server RSS 3268 kB with no connection, 6684 kB with 256 idle keep-alive connections: 13664 bytes per connection

(one task and its stack and buffers per connection; the figure is RSS growth, not a measured peak).

## Reading the numbers

- Small bodies: the native server serves 150k to 320k requests/s over 64 connections where python3's http.server serves about 22k; one connection is
  about 16k to 25k requests/s for both with one connection (the loopback round trip dominates; python's 23k there is above this run's native 15.6k and
  within the spread of the native runs).
- **1 MiB bodies are slower on the native server: 660 against 1125 requests/s with one connection, 5.0k against 7.0k with 64.** The cause is known
  and untouched: a response body is copied several times (`concat-bytes` for the head and the body, then `fib.os.native/bytes-new` stores it into a malloc
  buffer one byte at a time before `send`), and the client reads through a 16 KiB buffer and joins pieces with `concat-bytes`. A `send` from the array
  without the copies is the first lever; it needs a platform-layer primitive that writes a byte array range to a socket.
- The sequential client is on par with the curl CLI (0.38 s against 0.40 s) against the native server, and 0.53 s against 0.45 s against python.
- Latency percentiles are from the client's side of each request (`clock-now` around `request-with`), microseconds.
