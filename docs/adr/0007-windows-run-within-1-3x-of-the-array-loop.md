# 0007. Windows run within 1.3x of the array loop

Status: accepted
Date: 2026-10-05
Source: docs/design/exclusive-views.md; `scripts/bench/windows.sh` (package EV2, which already checks these claims when someone runs it).

## Context

Exclusive views (`with-view`, `with-tiles`, windows over an `Array`, `MArray` or `Tensor`) exist so that numeric code reads like NumPy
indexing and still compiles to the loop one would have written over the array: that is the pitch against Python, and it is why the checker
sees every write. If a window costs a multiple of the array loop, the abstraction has lost its reason. The claim was checked by a benchmark
script that nobody runs by default and that recorded its result nowhere in the repository.

## Decision

A loop over an exclusive window runs within **1.3 times** the time of the same loop over the array, for read-modify-write, fill and saxpy;
saxpy through windows runs within 1.3 times the library's SIMD kernel; a blocked write through tiles runs within 2.5 times (every tile is
lent once, so there is a per-tile cost). The measured ratios are recorded in `docs/design/exclusive-views.md` (section "Recorded
measures") as `<!-- measure KEY VALUE ratio DATE | cmd: .. -->` lines, newest last, and the decision holds while the latest record of each is
under its threshold.

## Consequences

- A change to the window lowering (`compiler/own/walk/call.fib`, `compiler/emit/lower/window.fib`) that slows a loop must either keep the
  ratios or change this decision on purpose.
- The ratios are the *recorded* ones: a new record is a person's act (`scripts/bench/windows.sh`, or `adr --rerun` which runs it and compares the
  fresh numbers). Until someone re-measures, the gate checks the record, not the machine.
- The numbers are for cache-resident kernels of 1,000 elements on one machine; they say nothing about memory-bound sizes.
- A ratio is noisy on a loaded machine (see the first record's history in the document).

## Governance

```fibber measure
(measure "read-modify-write through a window within 1.3x of the array loop"
  :recorded "docs/design/exclusive-views.md" :key "rmw-window/array" :under 1.3)
(measure "fill through a window within 1.3x of the array loop"
  :recorded "docs/design/exclusive-views.md" :key "fill-window/array" :under 1.3)
(measure "saxpy through a window within 1.3x of the array loop"
  :recorded "docs/design/exclusive-views.md" :key "saxpy-window/array" :under 1.3)
(measure "saxpy through a window within 1.3x of the SIMD kernel"
  :recorded "docs/design/exclusive-views.md" :key "saxpy-window/simd" :under 1.3)
(measure "a blocked write through tiles within 2.5x of the array loop"
  :recorded "docs/design/exclusive-views.md" :key "tile-window/array" :under 2.5)
```

The benchmark the records come from still enforces the same limits, and still prints the line `--rerun` reads:

```fibber fitness
(rule "windows.sh limits are the ADR's: 1.3 for the three loops and the SIMD kernel, 2.5 for tiles"
  (into (must-contain repo "scripts/bench/windows.sh" "ratio \"rmw window/array\" rmw-window rmw-array 1.3")
        (into (must-contain repo "scripts/bench/windows.sh" "ratio \"saxpy window/simd\" saxpy-window saxpy-simd 1.3")
              (must-contain repo "scripts/bench/windows.sh" "ratio \"tile window/array\" tile-window tile-array 2.5")))
  (plant-file "scripts/bench/windows.sh" "ratio \"rmw window/array\" rmw-window rmw-array 3.0\n"))

(rule "windows.sh prints the `measure KEY VALUE` line that --rerun reads"
  (must-contain repo "scripts/bench/windows.sh" "echo \"measure ${1// /-} $r\"")
  (plant-file "scripts/bench/windows.sh" "#!/bin/bash\n"))
```

### What this does not check

That the recorded numbers were measured, or when (run `adr --rerun`); other machines or compilers' flags (`-O 2`, the benchmark's own); sizes
beyond the cache; that the *source* of the benchmark (`scripts/bench/windows.fib`) still measures what its name says (a window loop and an array
loop doing the same work: the script checks their checksums, this ADR does not read them); parallel tiles (`par-4 / par-1 < 0.7` is the script's
claim, not this decision's).
