# Explicit DataLoader

A `fib.dataloader/Loader k v e` belongs to one request/coordinator and holds a
batch callback, positive maximum batch size and a Cell-backed cache. It is not
sendable. No worker, timer, task or pending promise is created by the loader.
Call `loader` with a positive size and a callback from keys to ordered per-key
results. `load-many` deduplicates missing keys in encounter order, calls bounded
chunks, then restores original order and duplicates. Empty/cached calls do no I/O.
A value can itself be an Option: a missing record is distinct from a per-key error.

Per-key errors cache along with successes. A batch-level error or a result-count
mismatch returns Err and publishes no new cache entries from that load-many call;
previous cache entries survive. Callback side effects are not rolled back. An
exception propagates without publishing the unfinished call's cache. `clear-key!`
and `clear!` invalidate cached entries. Drop the loader at request end. Reentry
into the same loader from its callback and concurrent use are unsupported.

This explicit dispatch model fits a GraphQL executor that already gathers sibling
parents. It does not block one resolver task waiting for another to enqueue work.
Caller context, argument and selection grouping remain the embedding's job; never
reuse a loader across authorization identities or requests.

Executable evidence: cases/stdlib/9800 and 9801 cover stable deduplication, chunk
bounds, duplicate restoration, empty/cache-only calls, cached per-key errors,
invalidation, separate loaders, invalid sizes, malformed callbacks and retry.

Case 9802 also pins rejection of moving a loader into another task.
