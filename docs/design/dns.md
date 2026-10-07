# A native DNS resolver (`fib.dns`)

Status: built (HTTP-2). The client resolves with it by default; `getaddrinfo` is the opt-in fallback.

## Why

`fib.os.net/resolve-host` calls `getaddrinfo`. In a static musl binary in a `FROM scratch` image, on Lambda or on a raw VM that call needs `/etc/hosts`, `/etc/resolv.conf` and, under
glibc, NSS (`/etc/nsswitch.conf`, shared libraries loaded at run time, which a static binary cannot do). The owner deploys static binaries there (docs/design/static-linking.md), so the
name lookup had to stop depending on libc: `fib.dns` reads the two files itself and speaks DNS over UDP and TCP. It needs only the kernel's socket calls.

## Layers

```
fib.dns                  facade: types, config, cache, resolve
  fib.dns.resolve        the resolver: hosts, search rules, A and AAAA at once (one task each), servers x attempts, the cache
  fib.dns.answer         reading a response: CNAME chain, off-chain records ignored, TTLs, negative TTL from the SOA
  fib.dns.exchange       one question to one server: UDP, the ID and question check, TC -> TCP, EDNS0 and its FORMERR fallback
  fib.dns.cache          TTL cache with negative entries, bounded, the clock an argument
  fib.dns.config         /etc/resolv.conf and /etc/hosts parsed from text, the fallbacks
  fib.dns.decode / .encode / .types / .ip    the wire format, no network
fib.os.net.udp           the platform layer's addition: connected UDP sockets, bind, recvfrom, sendto (the only externs)
```

No `extern` outside `fib.os` (ADR 0011); no global mutable state (ADR 0013): a `Resolver` is a value (configuration and an `Atom` cache) that is shared on purpose. `fib.os.net.udp` is the whole platform
addition: `socket-new-datagram` in both backends (Linux `SOCK_DGRAM|NONBLOCK|CLOEXEC`; Darwin `SOCK_DGRAM` then `fcntl`), `connect`, `send`, `recv`, `bind`, `recvfrom`, `sendto`, all with deadlines
through the existing `poll`.

## What it does

- **Files.** `/etc/resolv.conf`: `nameserver` (at most 3, literals only), `search` (at most 6) and `domain` (the last of the two wins), `options ndots:N timeout:N attempts:N` (capped at 15, 30 s, 5); unknown
  options and bad lines are skipped. `/etc/hosts`: addresses with names, case-insensitive, a trailing dot ignored; consulted first (the nsswitch default `files dns`). Files over 256 KiB are ignored.
  **Fallbacks when absent** (a scratch image has neither): no nameserver gives `127.0.0.1:53` (what musl and glibc do), no search list gives none (the host's own domain is not guessed), ndots 1, timeout
  5 s, attempts 2. **Lambda** writes a `resolv.conf` (the VPC resolver), so it is read, not assumed; a Docker `scratch` container gets one from Docker.
- **Names.** An IPv4 or IPv6 literal (also in brackets) is returned as it is, no query. `localhost` and `*.localhost` are the loopback addresses (RFC 6761 6.3) with no files. A name that cannot be a DNS name
  (empty label, label over 63 bytes, over 255 bytes, a byte outside printable ASCII) is refused before any query. A trailing dot is absolute. Otherwise `search` and `ndots` as resolv.conf says: at least
  ndots dots, the name first, else the search suffixes first. IDN is the caller's (punycode); 8-bit and escaped labels are not supported.
- **Queries.** A and AAAA at once, one task each (RFC 1035, 3596), each over a fresh connected UDP socket: the kernel drops datagrams from any other address or port. A 16-bit ID from the platform's entropy
  (`getentropy`) for every try (no entropy is a failure, never a predictable ID); the source port is the kernel's random ephemeral port (Linux and Darwin randomise it; the library does not bind one).
  EDNS0 advertises 1232 bytes (the DNS flag day size); a server that answers FORMERR or NOTIMP gets one retry without it. A truncated answer (TC) is asked again over TCP to the same server (RFC 7766, two-byte
  length frame, at most 65535). Servers are tried in order for `attempts` rounds, `timeout` each, under an overall deadline (`total-ms`, 15 s); a refused port (ICMP) moves to the next server at once.
- **Validation.** An answer counts only if it is a response (QR), has the query's ID, opcode 0, exactly one question equal to the one asked (case-insensitive name), and decodes without error. Anything else is
  ignored and the wait goes on until the deadline: a spoofer cannot end the exchange, only a real answer can. The decoder checks every read, every section count against the bytes left before allocating,
  that compression pointers point strictly before the start of the name and before every earlier pointer's target (a loop is an error, at most 126 jumps), names at most 255 bytes, labels printable ASCII
  without a dot, rdata lengths exact, and that nothing follows the last record (case 8162, 23 malformed shapes in the spec, every prefix of a valid message, 100,000 mutants).
- **Answers.** CNAME chains from the question's name (at most 8 links, a name seen twice is a loop); only records on the chain are used (a response carrying records for other names is not trusted into the
  answer); the smallest TTL of what was used; at most 16 addresses per type. IPv4 first, then IPv6 (a connect that fails moves on; a host with no IPv6 pays one immediate error): there is no happy eyeballs.
- **Cache.** By TTL (capped at a day, TTL 0 is not cached), negative entries from the SOA (the smaller of its TTL and its minimum, capped at an hour, none when the response has no SOA, RFC 2308), at most 512
  entries (default; expired ones go first, then the one that expires soonest). Failures (SERVFAIL, timeouts) are never cached. The cache lives in the `Resolver`: a client builds one (`build-http-client`),
  `request` builds a client per call so it has none; keep a client to keep the cache.
- **Not done.** DNSSEC (the `AD` bit is ignored, nothing is validated), DoT and DoH, 0x20 case randomisation, mDNS and LLMNR, SRV and other types, `options rotate`, `resolv.conf` reload (the files are read when the
  resolver is built), EDNS client subnet, IPv6 zone ids (`fe80::1%eth0` is refused), IDN.

## In `fib.http`

`Options.resolver` is `NativeDns` (the default), `SystemResolver` (`(with-system-resolver o)`: `getaddrinfo`, for hosts whose names live in NSS only: mDNS, LDAP) or `(CustomResolver f)`. `resolve-host`'s shape
is kept: `fib.dns/resolve-host` returns `(Result (Vec str) OSError)` (NotFound, TimedOut, Other); the client maps it to `DnsError`. The client's `Target.dial` is how a factory reaches the network, so TLS
and proxies use the same resolver.

## Tests

| What | Where | Result (2026-10-07) |
|---|---|---|
| message tables, malformed packets, prefixes, round trips (3,000 generated), IP and config tables, cache with a driven clock, search rules, answer reading, 100,000 mutated responses (no trap; every success re-encodes) | `specs/dns-wire-spec.fib` | 29 scenarios pass |
| end to end against a fake server in the process over loopback UDP and TCP: answers, CNAME chains, NXDOMAIN, NODATA, SERVFAIL, TC then TCP, lost and slow answers with retry, a refused first server, forged answers (wrong ID, wrong question, another source port), 48 queries with at least 40 distinct IDs and ports, the cache (hit, TTL, negative, not for failures), search and ndots as the queries seen, literals, hosts, localhost | `specs/dns-resolver-spec.fib`, `specs/dns-fake.fib` | 28 scenarios pass |
| cases | 8160-8165, 8171 | pass |
| planted faults, each of which a spec must catch: accept a wrong ID, no question check, no response bit, TC ignored, pointers may point forward, no pointer bound and no jump cap (a loop; the timeout kills it), trailing bytes accepted, no count check, TTL top bit, the cache ignores TTL, no negative cache, unbounded cache, no CNAME chain limit, no loop check, off-chain records accepted, a constant ID, ndots the wrong way round | `scripts/mutant-dns.sh` | 17 of 17 killed |
| the box's tools on live names (`example.com`, `www.iana.org`, `localhost`, an unknown name): the same address sets as `dig +short A` plus `AAAA` (and `getent ahosts` for localhost) | `scripts/dns-diff.sh` | same for all 4 |
| a static binary alone in a `FROM scratch` image with only a bind-mounted `resolv.conf` resolves and fetches | `scripts/static-fetch-demo.sh` | see docs/shootout/http.md |
