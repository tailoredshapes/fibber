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

Each search work item carries a `State` and a finite-domain `Store`. The store
contains variable-id to domain entries and posted constraints. Propagation
normalizes terms through the current unification state, intersects membership
constraints, removes values that violate fixed all-different terms, narrows
ordering and sum supports, and binds singleton domains back into the term state.
When propagation reaches a fixed point, search chooses the smallest domain and
enqueues one equality goal for each remaining value. This is a first-fail
heuristic with immutable branches, so an abandoned branch cannot mutate a
sibling.

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
`fd/distinct`. The original license is included in
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
7200–7206.

## Performance comparison

The comparison below uses the same two 81-cell fixtures and asks for one
solution. Both harnesses discard 100 warm-up solves, then report the median of
31 measured solves in the same process. Clojure's lazy result stream and each
solution grid are fully realized with `mapv vec` before stopping the clock.
Both harnesses check answer count, all clues, and all row/column/box constraints
outside the timed region. Compilation, JVM startup, file I/O, validation, and
printing are excluded.

These measurements were taken on 2026-10-05 on an Intel Core i7-14700KF, with
Fibber native code at the default build optimization level (`-O 2`), Clojure
1.11.1, core.logic 1.0.1, and Temurin OpenJDK 27+35. The runs were serialized
through `/tmp/fibsuite.lock`.

**Correction:** the previous Clojure harness stopped the clock before forcing
its lazy answer stream. It measured stream construction while Fibber measured
the completed solve. The previously reported 62–85× ratios are invalid and are
superseded by the completed-solve measurements below.

| puzzle | Fibber median | Clojure/core.logic median | Fibber / Clojure |
|---|---:|---:|---:|
| test | 4,953,288 ns | 4,386,633 ns | 1.13× |
| hard | 4,976,612 ns | 1,808,488 ns | 2.75× |

The Fibber samples were:

```text
test: 4972059 5236678 5078854 4894657 5142293 4974061 5014874 5231808 5057992 5204466 4966094 5210570 5259382 4903832 4838445 4953288 4864087 4837361 4957312 4691772 4482733 5074920 4775218 4783549 4830599 5026174 4621484 4850433 4830275 4794615 4804881
hard: 4974351 4979687 4980572 4953833 5112635 4951673 4754056 4956499 4976612 4900576 4961018 5158577 5183690 5033048 5300706 5273961 5114450 4976887 5039082 4911419 4867872 5079784 4950129 4894270 5051437 4957414 4971481 4943766 4971347 4988203 5022749
```

The Clojure samples were:

```text
test: 4408320 4409764 4231522 4320286 4288162 4307365 4254569 4256812 11445538 4813257 4590775 4640558 4329382 4189268 4582921 4470034 4160046 4462256 4359601 4386633 4457148 4384032 4529761 4303141 4293948 4736299 4063923 4234886 4392855 4534702 4423388
hard: 1863407 1824703 1795361 1806113 1867477 1781691 1779427 1796556 1796433 1793574 1876641 1977956 1808488 1792745 1741171 1795007 1906721 1882023 1839315 1823679 1823806 1797232 1762587 1824957 1935352 1998688 1805896 1792750 1823318 1860870 1762358
```

These results compare two particular puzzle fixtures and solver implementations.
They do not establish a general language performance ratio or identify which
component causes the remaining gap. Clojure runs after repeated JVM warm-up;
Fibber's native solver uses immutable maps and vectors with no specialized
Sudoku representation. Both are sequential. Profiling and counts of propagation
and search work are needed to distinguish solver overhead from differing amounts
of search.

Reproduce the Fibber side with:

```sh
./F build scripts/bench/sudoku.fib -I examples -I lib -o /tmp/sudoku-fib-bench
/tmp/sudoku-fib-bench
```

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

## Next performance steps

The implementation contains several potential sources of excess solver work.
Profiling should establish their priority before the next optimization pass:

1. Extend the current wide-domain sum bounds with residue-aware filtering and
   cache each constraint's last domains; exact support remains useful for small
   domains.
2. Add a propagation queue with watched variables so a changed domain revisits
   only affected constraints instead of rescanning the whole store.
3. Use a trail or compact copy-on-write store for branch updates, reducing map
   and vector allocation at every Sudoku choice.
4. Strengthen `distinct` with matching or Hall-set filtering. The current pass
   removes fixed singleton values; it does not detect every hidden subset.
5. Add a Sudoku-specific packed 9-bit candidate representation as an optional
   specialization, keeping the generic domain API as the reference path.
6. Parallelize only after a configurable search cut-off, with deterministic
   answer ordering as an explicit option. Threads should operate on independent
   branches and never share mutable domains.

SIMD can help dense batches of arithmetic constraints after these changes, but
it is not a direct optimization for the current irregular Sudoku search tree.
