# Stability and the 1.0 proposal

Status: pre-1.0 policy clarification; the freeze below is a **proposal**.
VERSION and SEED currently name 0.1.13. Nothing here publishes a 1.0 promise.

The owner decides release timing and the promised surface. Executable checks
establish whether that surface meets its acceptance criteria; approval and
checks are both needed. Passing tests cannot choose the promise themselves.

The audit proposes S (stable candidate), E (experimental) and I (internal).
These are proposed classifications, not guarantees already attached to exports.
S would preserve compatible source/API behaviour in 1.x, with breaking changes
reserved for a major release. E would be labelled at the API and could change
in a minor release. I would carry no external compatibility promise. Diagnostic
wording, compiler implementation details and tuning knobs should remain I;
lIR, C, PTX/WGSL and server protocols need explicit format/version decisions.

A proposed deprecation process announces the replacement, documents migration
and retains the old stable name through 1.x unless a separately documented
security correction requires otherwise. The owner must approve its duration
and exceptions before it becomes a promise.

Proposed 1.0 acceptance evidence:

- Full Linux gate and native Mac gate on the release commit; skipped platform
  checks are listed explicitly, not counted as passes.
- The release tarball's relocation, dependency and hello build/run checks.
- Documentation examples, links and generated references match the release.
- ADR 0023's unbound planning rows are implemented or explicitly omitted from
  the promised reference; known failure 1707 is fixed or explicitly excluded
  from the promise with rationale.
- Stable/experimental/internal exports and supported platforms are approved.
- A newcomer runs the tutorial against both published platform tarballs.

See [the audit proposal](../design/docs-audit.md#2-the-surface-a-10-would-freeze)
for candidate classifications and [limits](limits.md) for current boundaries.
