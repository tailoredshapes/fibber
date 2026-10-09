# fib.dns

Generated declaration inventory of [fib.dns](../../../lib/fib/dns.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `Cache` | defstruct | [lib/fib/dns/cache.fib:11](../../../lib/fib/dns/cache.fib#L11) |
| `DnsConfig` | defstruct | [lib/fib/dns/resolve.fib:7](../../../lib/fib/dns/resolve.fib#L7) |
| `DnsError` | defenum | [lib/fib/dns/resolve.fib:9](../../../lib/fib/dns/resolve.fib#L9) |
| `Entry` | defstruct | [lib/fib/dns/cache.fib:8](../../../lib/fib/dns/cache.fib#L8) |
| `HostEntry` | defstruct | [lib/fib/dns/config.fib:8](../../../lib/fib/dns/config.fib#L8) |
| `Message` | defstruct | [lib/fib/dns/types.fib:18](../../../lib/fib/dns/types.fib#L18) |
| `Question` | defstruct | [lib/fib/dns/types.fib:15](../../../lib/fib/dns/types.fib#L15) |
| `Rdata` | defenum | [lib/fib/dns/types.fib:16](../../../lib/fib/dns/types.fib#L16) |
| `ResolvConf` | defstruct | [lib/fib/dns/config.fib:7](../../../lib/fib/dns/config.fib#L7) |
| `Resolver` | defstruct | [lib/fib/dns/resolve.fib:8](../../../lib/fib/dns/resolve.fib#L8) |
| `Rr` | defstruct | [lib/fib/dns/types.fib:17](../../../lib/fib/dns/types.fib#L17) |
| `Server` | defstruct | [lib/fib/dns/config.fib:6](../../../lib/fib/dns/config.fib#L6) |
| `cache-get` | defun | [lib/fib/dns/cache.fib:15](../../../lib/fib/dns/cache.fib#L15) |
| `cache-put` | defun | [lib/fib/dns/cache.fib:36](../../../lib/fib/dns/cache.fib#L36) |
| `cache-put-negative` | defun | [lib/fib/dns/cache.fib:38](../../../lib/fib/dns/cache.fib#L38) |
| `cache-size` | defun | [lib/fib/dns/cache.fib:13](../../../lib/fib/dns/cache.fib#L13) |
| `candidates` | defun | [lib/fib/dns/resolve.fib:18](../../../lib/fib/dns/resolve.fib#L18) |
| `class-in` | def | [lib/fib/dns/types.fib:10](../../../lib/fib/dns/types.fib#L10) |
| `default-conf` | defun | [lib/fib/dns/config.fib:16](../../../lib/fib/dns/config.fib#L16) |
| `default-dns-config` | defun | [lib/fib/dns/resolve.fib:12](../../../lib/fib/dns/resolve.fib#L12) |
| `default-servers` | def | [lib/fib/dns/config.fib:15](../../../lib/fib/dns/config.fib#L15) |
| `fields` | defun | [lib/fib/dns/config.fib:22](../../../lib/fib/dns/config.fib#L22) |
| `file-limit` | def | [lib/fib/dns/config.fib:62](../../../lib/fib/dns/config.fib#L62) |
| `hosts-lookup` | defun | [lib/fib/dns/config.fib:58](../../../lib/fib/dns/config.fib#L58) |
| `load-hosts` | defun | [lib/fib/dns/config.fib:73](../../../lib/fib/dns/config.fib#L73) |
| `load-resolv-conf` | defun | [lib/fib/dns/config.fib:72](../../../lib/fib/dns/config.fib#L72) |
| `lookup-type` | defun | [lib/fib/dns/resolve.fib:25](../../../lib/fib/dns/resolve.fib#L25) |
| `lower` | defun | [lib/fib/dns/config.fib:17](../../../lib/fib/dns/config.fib#L17) |
| `make-flags` | defun | [lib/fib/dns/types.fib:31](../../../lib/fib/dns/types.fib#L31) |
| `max-negative-ttl-s` | def | [lib/fib/dns/cache.fib:7](../../../lib/fib/dns/cache.fib#L7) |
| `max-ttl-s` | def | [lib/fib/dns/cache.fib:6](../../../lib/fib/dns/cache.fib#L6) |
| `new-cache` | defun | [lib/fib/dns/cache.fib:12](../../../lib/fib/dns/cache.fib#L12) |
| `new-resolver` | defun | [lib/fib/dns/resolve.fib:13](../../../lib/fib/dns/resolve.fib#L13) |
| `opcode` | defun | [lib/fib/dns/types.fib:30](../../../lib/fib/dns/types.fib#L30) |
| `parse-hosts` | defun | [lib/fib/dns/config.fib:55](../../../lib/fib/dns/config.fib#L55) |
| `parse-resolv-conf` | defun | [lib/fib/dns/config.fib:50](../../../lib/fib/dns/config.fib#L50) |
| `rcode` | defun | [lib/fib/dns/types.fib:29](../../../lib/fib/dns/types.fib#L29) |
| `rcode-formerr` | def | [lib/fib/dns/types.fib:12](../../../lib/fib/dns/types.fib#L12) |
| `rcode-nxdomain` | def | [lib/fib/dns/types.fib:14](../../../lib/fib/dns/types.fib#L14) |
| `rcode-ok` | def | [lib/fib/dns/types.fib:11](../../../lib/fib/dns/types.fib#L11) |
| `rcode-servfail` | def | [lib/fib/dns/types.fib:13](../../../lib/fib/dns/types.fib#L13) |
| `resolve-host` | defun | [lib/fib/dns/resolve.fib:84](../../../lib/fib/dns/resolve.fib#L84) |
| `resolve-name` | defun | [lib/fib/dns/resolve.fib:75](../../../lib/fib/dns/resolve.fib#L75) |
| `response?` | defun | [lib/fib/dns/types.fib:27](../../../lib/fib/dns/types.fib#L27) |
| `system-resolver` | defun | [lib/fib/dns/resolve.fib:15](../../../lib/fib/dns/resolve.fib#L15) |
| `truncated?` | defun | [lib/fib/dns/types.fib:28](../../../lib/fib/dns/types.fib#L28) |
| `type-a` | def | [lib/fib/dns/types.fib:4](../../../lib/fib/dns/types.fib#L4) |
| `type-aaaa` | def | [lib/fib/dns/types.fib:8](../../../lib/fib/dns/types.fib#L8) |
| `type-cname` | def | [lib/fib/dns/types.fib:6](../../../lib/fib/dns/types.fib#L6) |
| `type-ns` | def | [lib/fib/dns/types.fib:5](../../../lib/fib/dns/types.fib#L5) |
| `type-opt` | def | [lib/fib/dns/types.fib:9](../../../lib/fib/dns/types.fib#L9) |
| `type-soa` | def | [lib/fib/dns/types.fib:7](../../../lib/fib/dns/types.fib#L7) |
