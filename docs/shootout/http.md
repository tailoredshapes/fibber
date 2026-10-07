# HTTP: the native client and server

Two measurements: HTTP-2 on the quiet Ryzen box (first), HTTP-1's original on the busy development box (below, kept as it was).

## HTTP-2 (2026-10-07): the Ryzen box, before and after

Box: ssh `tsmar@192.168.7.83`, WSL2 Ubuntu (kernel 6.6.87.2-microsoft-standard-WSL2), 16 cpus, load average 1.4 to 3.6 while running (a Windows host under WSL2 is not a quiet Linux:
loopback is slower than on the development box, which is why absolute numbers are lower), python 3.14.4, curl 8.18.0, the v0.1.11 release `fibc` (`fibc build` at -O2). Scratch directory only
(`~/fibber-scratch/http2`). Two trees of `lib` + `scripts/bench`, **before** = commit 68e4cf6 (HTTP-1) and **after** = this package (HTTP-2) at f970860 plus the read-size change below,
each built and run by the same `scripts/bench/http-bench.py --runs 5` (medians of 5), interleaved before, after, before, after in one session; the reference server is python's `http.server`
and the load generator is fibber's own native client. The load generator is unchanged between trees, so the client side of each row is the tree's client.

| server | body | conns | before rps (2 runs) | after rps (2 runs) | before p50 us | after p50 us | python rps |
|---|---|---:|---:|---:|---:|---:|---:|
| native | 100 B | 1 | 7550, 7737 | 7442, 7505 | 128, 126 | 131, 130 | 6331, 6474 |
| native | 100 B | 64 | 120791, 130578 | 124454, 122158 | 462, 436 | 465, 466 | 8270, 8283 |
| native | 1 MiB | 1 | 477, 487 | 697, 696 | 2041, 2022 | 1406, 1411 | 668, 689 |
| native | 1 MiB | 64 | 2132, 2196 | 3373, 3331 | 19401, 18354 | 6412, 6776 | 3552, 3430 |

(A first run of the intermediate tree, which read 64 KiB for every head as well, gave 96857 rps for 100 B over 64 connections against 123594 before: a bigger buffer per request costs more than it
saves, so heads read 16 KiB again and only body reads ask for 64 KiB; the table is the final tree.)

What changed for the 1 MiB rows: **660 to 700 rps with one connection, 2.1k to 3.4k with 64: from 70 percent of python's `http.server` to its level** (python read 668 to 689 and 3265 to 3552 in
the same sessions). The three changes: `fib.os.net/send-all` hands the array's own data to `send` (it copied the body into a malloc buffer one byte at a time, about a millisecond a MiB); the server writes
the head and a body of 16 KiB or more as two writes instead of joining them with `concat-bytes` (another millisecond a MiB); `concat-bytes` of one part returns it. What is **not** fixed: the client
joins a body read in pieces with the same byte loop (about a millisecond a MiB; `array-copy` is 94 microseconds a MiB, so a blit builtin would make it nearly free: CRYPTO-3 is adding one and had not
landed, so the loop stays and the follow-up is named in docs/design/http.md). Small bodies are unchanged within the spread (7.4k to 7.7k rps with one connection, 121k to 131k with 64).

10,000 sequential GETs on one keep-alive connection (first session; seconds, median of 5): curl CLI against native 1.62 before and 1.59 after; the native client against native 1.35 before and 1.38 after;
against python's server curl 1.76 and 1.79, the native client 1.65 and 1.65. The probe of pooled connections adds one `poll` per reused connection and is inside the noise.
Memory: 13312 bytes per idle keep-alive connection before, 12800 after (RSS growth over 256 connections).

DNS: `scripts/dns-diff.sh` against `dig` and `getent` on four live names gave identical address sets (docs/design/dns.md). A static binary alone in a `FROM scratch` image (`scripts/static-fetch-demo.sh`, development box,
host network, the host's `resolv.conf` bind-mounted read-only):

```
binary: 793080 bytes
/tmp/static-fetch.3iCZ6h/fetch: ELF 64-bit LSB executable, x86-64
image: fibber-static-fetch:demo 793kB
--- the image holds only /fetch (no libc, no /etc/nsswitch.conf; docker itself adds /etc/resolv.conf and /etc/hosts to a running container), an unknown name:
resolve failed: no-such-host.invalid: no such host
GET failed: no-such-host.invalid: no-such-host.invalid: no such host
--- with the host's resolv.conf bind-mounted:
resolved example.com -> 172.66.147.243 104.20.23.154 2606:4700:10::ac42:93f3 2606:4700:10::6814:179a
GET http://example.com/ -> 200, 577 bytes
```

`ab`, `wrk` and `hey` are still not installed. TLS was not measured (no crypto driver in the gate).

# HTTP-1, as first measured (development box)

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
