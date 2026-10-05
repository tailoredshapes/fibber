# Finite domains and the Sudoku port

`fib.logic.fd` extends the relational core with finite integer domains. It is
the first constraint layer built on the typed terms and immutable FIFO search
in [`fib.logic`](../../lib/fib/logic/README.md). The implementation is small
enough to serve as a reference for later stronger propagators while already
supporting a complete Sudoku solver.

## API and representation

The public entry point is `fib.logic.fd`:

```clojure
(let ((d (fd/interval 1 9)))
  (l/run 1 (fn (q)
    (l/all [(fd/in q d)
            (fd/!= q (l/integer 4))]))))
```

The current API is:

| Function | Purpose |
|---|---|
| `interval lo hi` | Inclusive integer interval |
| `domain values` | Sorted, deduplicated explicit values |
| `size d` | Number of values |
| `in term d`, `in-all terms d` | Domain membership constraints |
| `batch goals` | Post equality and FD goals together, propagate once |
| `distinct terms` | Pairwise all-different constraint |
| `!=`, `<`, `<=` | Disequality and ordering constraints |
| `+ left right result` | Integer addition relation |

Domains are canonical values, rather than mutable solver objects. An interval
or explicit set whose values fit in a 63-value window is stored as `(base,
mask)` and tested with integer bit operations. A large interval stays as lazy
`(lo, hi)` bounds; an irregular explicit set uses sorted unique values.
Intersections preserve compact or interval forms when possible, and sparse
intersections use binary membership checks. Removing an interior value from a
large interval currently materializes the remaining values; split intervals are
an obvious next representation improvement.

Each search work item carries a `State` and a finite-domain `Store`. Domain
entries and variable watches use indexed persistent vectors for a bounded dense
prefix of variable IDs, with maps for negative or large IDs. Branch updates copy
only the affected vector paths when another branch shares them.

Posting a constraint registers its variable watches and queues it once. Domain
changes wake only watched constraints, with duplicate queue entries suppressed.
Tracked unification reports the roots it extended, so aliasing intersects those
domains and transfers their watches without rebuilding the entire domain store.
Singleton events bind integers into the term state; general unification retains
its occurs check. Entailed constraints retire under stable IDs, so queued entries
and watches can safely refer to them without renumbering.

Propagation intersects membership constraints, removes fixed all-different
values, and narrows ordering and sum supports. All-different accumulates its
fixed values in a domain; compact domains remove all fixed values together with
an aligned mask. Sparse fixed values retain a general fallback.
An empty forbidden domain and removal of an interval endpoint keep wide domains
lazy. Propagation finishes when the event queues are empty, rather than comparing
whole maps for equality. Search then chooses the smallest domain and
enqueues one equality goal for each remaining value. This is a first-fail
heuristic with immutable branches, so an abandoned branch cannot mutate a
sibling.

`fd/in-all` posts its domains as one batch. `fd/batch` also accepts equality
goals, FD constraints, and nested `all` or batch goals, applying setup in order
and propagating once at the boundary. Fresh variables must be built outside the
batch; disjunctions and delayed builders are rejected. A batch occupies one
ordinary logic scheduler step and can change interleaving with other branches.
The batch's solution set agrees with posting the same conjunction separately.

The store's `runs` and `updates` counters record actual constraint executions and
domain changes along that branch's ancestry. They support scheduling regressions
and setup diagnostics; they do not total work in other or failed branches.

Ordering and disequality use bound filtering. Addition keeps exact support
enumeration for small domains and switches to sound interval bounds for wide
domains, avoiding a Cartesian product when the candidate sets are large.
The bounds are intentionally weaker than full arc consistency, so later search
still verifies every concrete assignment.

The domain representation and bound-propagation shape are adapted from
`clojure.core.logic.fd` 1.0.1 under EPL-1.0; the Fibber implementation uses
its own typed terms, immutable stores, and ownership rules.

## Sudoku port

[`examples/sudoku/solver.fib`](../../examples/sudoku/solver.fib) ports the
solver from [tsmarsh/sudoku](https://github.com/tsmarsh/sudoku), at commit
[`743e4640`](https://github.com/tsmarsh/sudoku/tree/743e4640a567462c7a1fd45b0f96c2ef0ddcecb5).
The original project is an EPL-1.0 Clojure program that adapts David Nolen's
core.logic example. The port retains the same row, column, and 3-by-3 box
model, while replacing core.logic finite-domain calls with `fd/in-all` and
`fd/distinct`, and batching domains, clues, and units with `fd/batch`.
The original license is included in
[`examples/sudoku/LICENSE`](../../examples/sudoku/LICENSE), and the two input
fixtures are preserved in that directory.

Run the port with a rebuilt stage-2 compiler:

```sh
./F build examples/sudoku.fib -I examples -I lib -o /tmp/fibber-sudoku
/tmp/fibber-sudoku examples/sudoku/test.txt
/tmp/fibber-sudoku examples/sudoku/hard.txt
```

Both fixtures produce one valid solution. The executable prints the solve time
in nanoseconds and the solved grid. The executable case
`cases/stdlib/7204-sudoku-port-solves-both-original-fixtures.fib` checks both
solutions against the preserved answers; the domain and constraint cases are
7200–7212. These include watch transfer through aliases, skipping unrelated
constraints, branch isolation, structured unification, wide interval and sparse
ID behavior, and agreement with independent enumeration across 32 models.

## Performance comparison

The comparison below uses the same two 81-cell fixtures and asks for one
solution. Both harnesses discard 100 warm-up solves, then report the median of
31 measured solves in the same process. Each implementation runs in three fresh
processes; the summary below is the median of the three process medians.
Clojure's lazy result stream and each solution grid are fully realized with
`mapv vec` before stopping the clock.
Both harnesses check answer count, all clues, and all row/column/box constraints
outside the timed region. Compilation, JVM startup, file I/O, validation, and
printing are excluded.

These measurements were taken on 2026-10-05 on an Intel Core i7-14700KF, with
Fibber native code at the default build optimization level (`-O 2`), Clojure
1.11.1, core.logic 1.0.1, and Temurin OpenJDK 27+35. The runs were serialized
through `/tmp/fibsuite.lock`, without CPU affinity. Each round ran Fibber before
the change, Fibber after the change, then Clojure. The before binary was built
from `4b14367`; the after binary includes batched setup, watched propagation,
indexed persistent domain storage, and mask-based all-different filtering.

**Correction:** the previous Clojure harness stopped the clock before forcing
its lazy answer stream. It measured stream construction while Fibber measured
the completed solve. The previously reported 62–85× ratios are invalid and are
superseded by the completed-solve measurements below.

| puzzle | Fibber before | Fibber after | Clojure/core.logic | Clojure / Fibber after |
|---|---:|---:|---:|---:|
| test | 4,930,063 ns | 699,000 ns | 4,348,139 ns | 6.22× |
| hard | 5,149,557 ns | 644,872 ns | 1,850,673 ns | 2.87× |

Fibber improves by 7.05× on `test` and 7.99× on `hard` relative to the before
binary. It completes both fixtures faster than the warmed Clojure solver in this
comparison. This measures the combined effect of the library changes, rather
than attributing a speedup to each change separately.

The individual process medians, in nanoseconds, were:

| implementation | test, rounds 1 / 2 / 3 | hard, rounds 1 / 2 / 3 |
|---|---|---|
| Fibber before | 4,930,063 / 4,748,527 / 5,018,356 | 5,149,557 / 5,160,084 / 5,006,032 |
| Fibber after | 687,197 / 715,243 / 699,000 | 644,872 / 649,735 / 621,888 |
| Clojure | 4,271,317 / 4,871,732 / 4,348,139 | 1,850,673 / 1,930,235 / 1,820,728 |

[The measurement record](finite-domains-benchmark-20261005.json) contains every
sample and the execution policy.

These results compare two particular puzzle fixtures and solver implementations.
They do not establish a general language performance ratio. Clojure runs after
repeated JVM warm-up; Fibber uses generic immutable FD stores and constraints.
Both are sequential. A broader puzzle corpus and counts including failed
branches would help assess behavior on other search trees.

Reproduce the Fibber side with:

```sh
./F build scripts/bench/sudoku.fib -I examples -I lib -o /tmp/sudoku-fib-bench
/tmp/sudoku-fib-bench
```

Repeat the complete process invocation three times for the policy above. Run
each invocation and its Clojure counterpart serially under `/tmp/fibsuite.lock`.

For the original comparison harness, check out the cited commit and run
[`scripts/bench/sudoku.clj`](../../scripts/bench/sudoku.clj) with Clojure and
the core.logic dependency on the classpath. Include Clojure's spec.alpha and
core.specs.alpha dependencies as well. For example, with the jars in the current
directory and the original repository checked out at `sudoku/`:

```sh
java -cp clojure-1.11.1.jar:core.logic-1.0.1.jar:spec.alpha-0.3.218.jar:core.specs.alpha-0.2.62.jar:sudoku/src clojure.main scripts/bench/sudoku.clj
```

Record the compiler, CPU, optimization flags, JDK, and warm-up policy with new
measurements. A longer warm-up may change JIT behavior; timings also vary with
CPU scheduling and frequency.

Initial propagation can be inspected separately from completed-solve timings:

```sh
./F build scripts/bench/sudoku-work.fib -I examples -I lib -o /tmp/sudoku-work
/tmp/sudoku-work
```

| puzzle | initial constraint executions | initial domain updates | needs labeling |
|---|---:|---:|---|
| test | 90 | 254 | true |
| hard | 89 | 277 | true |

These counters exclude later search. The timed region includes search and answer
realization; validation remains outside the clock.

## Validation of the incremental engine

The 30 targeted logic and FD cases pass, including the six new cases 7207–7212.
The full gate reports `GATE PASS (full)`: 305 ownership cases and 27 module cases
pass; the 1,165 stdlib cases contain 1,143 passes, the existing expected atom leak
case 1707, and 21 existing open cases. Stage 2 and stage 3 emit identical lIR.

The general quick benchmark's ten checksums match the stored baseline. It flags
`set-conj` at 0.33 s versus the stored 0.23 s; a five-run recheck still flags
0.30 s. This program imports no logic modules. Before and after compilers emit
identical lIR for it (SHA-256
`ed0e8ad94426ed86b791f741db566d2689bf8e05e5f91d20e1df971b00381b15`).
Five interleaved control runs measure a 0.25 s median for both binaries:
before `[0.30, 0.27, 0.24, 0.25, 0.24]`, after
`[0.33, 0.26, 0.24, 0.24, 0.25]`. The stored-baseline flag remains recorded;
the control does not show a regression from the FD changes.

An affinity-controlled follow-up runs the historical cached binary, the before
binary, and the after binary on the same cores, rotating their order across nine
measured runs after one warm-up per binary and core. Outputs agree throughout.
The Intel Core i7-14700KF has performance and efficiency cores:

| core | historical cached binary | before FD changes | after FD changes |
|---|---:|---:|---:|
| performance core, CPU 8 | 227.606 ms | 231.103 ms | 228.332 ms |
| efficiency core, CPU 16 | 332.811 ms | 332.831 ms | 335.839 ms |

Core selection reproduces the magnitude of the unpinned timing flag. Pinning the
existing quick benchmark to CPU 8 with nine samples reports 0.24 s versus the
0.23 s baseline, matching the checksum and ending with `BENCH ok`:

```sh
taskset -c 8 scripts/bench/quick.sh -n 9 set-conj
```

Use the same CPU-affinity policy when recording and checking benchmarks on this
host. The follow-up changes no set or map implementation and retains the stored
baseline. Making affinity explicit in the benchmark runner remains separate
work, as does optimizing trie insertion.

## Further performance work

The watched queue, batched setup, indexed copy-on-write domains, and compact
all-different filtering address the first sources of excess bookkeeping. Further
changes should be guided by profiles and search counts:

1. Cache each all-different constraint's processed singleton values, so it visits
   only newly fixed peers rather than collecting all fixed values again.
2. Generate FD alternatives lazily rather than allocating every sibling equality
   goal before the first branch runs; preserve ordinary relational search fairness.
3. Extend wide-domain sum bounds with residue-aware filtering and cached support;
   exact support remains useful for small domains.
4. Strengthen `distinct` with matching or Hall-set filtering when the reduction in
   search pays for the propagator. core.logic 1.0.1 also primarily uses singleton
   elimination here; its incremental scheduling is the relevant implementation
   model for this pass.
5. Consider a reversible trail if profiles still show branch copying dominates,
   while preserving replayable search values and independent sibling branches.
6. Parallelize only after a configurable search cut-off, with deterministic
   answer ordering as an explicit option. Threads should operate on independent
   branches and never share mutable domains.

SIMD can help dense batches of arithmetic constraints after these changes, but
it is not a direct optimization for the current irregular Sudoku search tree.
