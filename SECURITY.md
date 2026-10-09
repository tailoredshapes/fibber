# Security

Fibber is pre-1.0. Its TLS client is **UNAUDITED** and client-only; read
[security boundaries and known limits](docs/policy/limits.md) before using it.

For sensitive reports, use the repository host's private vulnerability reporting
feature if enabled. If it is unavailable, open an issue asking for a private
contact without including exploit details or secrets. No private-report mailbox,
response SLA or audited-release claim is established by this document.
Include the affected commit/version, platform and a minimal reproducer once a
private channel is available. Report dependency/driver faults to that project's
maintainer as well; provider contract tests do not establish security assurance.
