# 0017. An idle process gives its cached large blocks back

Status: accepted
Date: 2026-10-06
Source: docs/design/allocator.md (ALLOC-1: "Decay first and then scope arena"; section 3, decay and the cap); rt/alloc.lir; cases 7791 and 7792.

## Context

The runtime caches freed blocks of 128 KiB and more, by size class, so a tensor loop that makes and drops 800 KB results does not
`mmap` and `munmap` each time. A cache that keeps what it was given would make a server that spiked once look like a server that leaks:
its resident set would stay at the peak for ever. The owner's line was decay first: a block kept longer than the decay time goes back
to the system, so an idle process's resident memory returns to where it started. "Returns to baseline" is a measured property, not a
syntactic one.

## Decision

The cache **decays**: the default decay time is one second (`FIB_ALLOC_DECAY_MS`, default 1000), a decay of 0 turns the cache off, and the
property is measured by a deterministic case that the gate runs, not by this rule:

- case 7791 makes 32 blocks of 2 MB, drops them, and requires VmRSS to rise by at least 48 MB while the cache holds them and to come back
  within 16 MB of the start after a 700 ms sleep with a 200 ms decay; a runtime that never decays fails it (it returns 3);
- case 7792 requires a decay of 0 to turn the cache off, from the call and from the environment.

This ADR keeps those two cases in the tree with their thresholds, keeps them out of the expected failures, and keeps the default in the
runtime and the design in agreement.

## Consequences

- The measurement is a case, so it runs in every gate (`F cases cases/stdlib`), on Linux for the VmRSS part and on every platform for
  the cache's own counts.
- Loosening a threshold of 7791 (a bigger slack, a smaller rise) is an edit this rule sees.
- A change of the default decay is a change of the runtime, the design and this ADR together.

## Governance

```fibber fitness
(defun case-7791 () -> str "cases/stdlib/7791-alloc-cache-decay-gives-blocks-back-after-the-decay-time.fib")
(defun case-7792 () -> str "cases/stdlib/7792-alloc-cache-off-with-a-decay-of-zero-and-from-the-environment.fib")

(defun listed-in (repo: Repo path: str needle: str) -> (Vec Finding)
  (match (file-text repo path)
    (nil [(Finding path 0 "is not in the tree")])
    ((some t) (if (some? (str-find t needle 0)) [(Finding path 0 (str "names " needle ": the case is listed among those that do not pass"))] []))))

(rule "the runtime's default decay is one second, in the initial value and in the environment's default"
  (into (must-contain repo "rt/alloc.lir" "(global internal fib.lc-decay i64 (i64 1000000000))")
        (into (must-contain repo "rt/alloc.lir" "(call @fib.lc-env-int (string \"FIB_ALLOC_DECAY_MS\") (i64 1000))")
              (must-contain repo "docs/design/allocator.md" "(default **1000**)")))
  (plant-file "rt/alloc.lir" ";; empty\n"))

(rule "the decay case is in the tree and keeps its thresholds (48 MB up, back within 16 MB, a 700 ms sleep, a 200 ms decay)"
  (into (must-contain repo (case-7791) "(>= (- b a) 50331648)")
        (into (must-contain repo (case-7791) "(<= (- c a) 16777216)")
              (into (must-contain repo (case-7791) "rss-bytes")
                    (must-contain repo (case-7792) "FIB_ALLOC_DECAY_MS"))))
  (plant-file "cases/stdlib/7791-alloc-cache-decay-gives-blocks-back-after-the-decay-time.fib" "(ns main)\n")
  (plant-remove "cases/stdlib/7792-alloc-cache-off-with-a-decay-of-zero-and-from-the-environment.fib"))

(rule "neither case is listed among the cases stage 2 does not pass"
  (into (listed-in repo "scripts/ci-stage2.expected" "7791-alloc-cache-decay") (listed-in repo "scripts/ci-stage2.expected" "7792-alloc-cache-off"))
  (plant "scripts/ci-stage2.expected" "\ncases/stdlib/7791-alloc-cache-decay-gives-blocks-back-after-the-decay-time.fib FAIL\n"))
```

### What this does not check

That the case passes: `F cases cases/stdlib` runs it in every gate and a failure fails the gate (scripts/gate.sh); that an idle **server's**
RSS returns to its baseline (the case allocates arrays in one process; a server's own growth is outside the cache); the behaviour on
macOS (no `/proc/self/status`: only the counts are checked there); performance of the cache (docs/shootout, the allocator design
tables).
