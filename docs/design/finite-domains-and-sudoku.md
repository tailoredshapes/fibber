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
mask)` and tested with integer bit operations. A wide or irregular set is stored
as sorted unique values. Intersections preserve the compact form when both
inputs fit it; sparse intersections use binary membership checks.

Each search work item carries a `State` and a finite-domain `Store`. The store
contains variable-id to domain entries and posted constraints. Propagation
normalizes terms through the current unification state, intersects membership
constraints, removes values that violate fixed all-different terms, narrows
ordering and sum supports, and binds singleton domains back into the term state.
When propagation reaches a fixed point, search chooses the smallest domain and
enqueues one equality goal for each remaining value. This is a first-fail
heuristic with immutable branches, so an abandoned branch cannot mutate a
sibling.

The arithmetic propagators currently use value enumeration for pairwise sums
and interval bounds. That is deliberately straightforward and correct for the
reference implementation; it is also the largest source of avoidable work on
larger arithmetic models.

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
7200–7203.

## Performance comparison

The comparison below uses the same two 81-cell fixtures and asks for one
solution. Each implementation is run repeatedly in one process and the first
sample is discarded. The Fibber benchmark reports seven samples and takes the
median of the remaining six; the original Clojure harness reports six samples
and takes the median of its remaining five. The Fibber numbers came from
`scripts/bench/sudoku.fib`; the Clojure numbers came from the original source
with Clojure 1.11.1 and core.logic 1.0.1.

| puzzle | Fibber median | Clojure/core.logic median | Fibber / Clojure |
|---|---:|---:|---:|
| test | 6,127,672 ns | 81,476 ns | 75.2× |
| hard | 6,257,579 ns | 60,235 ns | 103.9× |

The Fibber samples were:

```text
test: 8579142 8785095 7669252 6036126 5792330 6086034 6169309
hard: 6098106 6212691 6296737 5947995 6331919 6353612 6228420
```

The Clojure samples were:

```text
test: 681297 76727 87202 81476 101830 53840
hard: 64250 52073 63884 67055 58056 60235
```

These are development measurements, not a claim of language-level parity.
Clojure benefits from a mature core.logic finite-domain implementation and a
warmed JVM JIT. Fibber's solver is native code with immutable maps and vectors,
simple propagation, and no specialized Sudoku representation. The current
solver does not use SIMD or threads: the work is branch-heavy and irregular,
and SIMD is useful only after a constraint has a dense numeric kernel.

Reproduce the Fibber side with:

```sh
./F build scripts/bench/sudoku.fib -I examples -I lib -o /tmp/sudoku-fib-bench
/tmp/sudoku-fib-bench
```

For the original comparison harness, check out the cited commit and run
[`scripts/bench/sudoku.clj`](../../scripts/bench/sudoku.clj) with Clojure and
the core.logic dependency on the classpath. Keep the compiler,
CPU, optimization flags, and process warm-up policy with any new measurements;
single executable runs include startup and allocation noise.

## Next performance steps

The measured gap points to solver work rather than SIMD throughput. The useful
sequence of improvements is:

1. Replace the pairwise sum enumeration with interval and residue-aware bounds,
   then cache each constraint's last domains.
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
