# Sequential relational search

2026-10-04. Implemented in the explicit `fib.logic` library as a small
miniKanren-style reference engine. This milestone is sequential; parallel
search and SIMD constraints are subsequent work.

## Terms and queries

`Term` is a typed tagged union of logic variables, i64 integers, named symbols,
the empty list, and pairs. Integer and symbol atoms are distinct even when their
printed text is similar. `list-term` constructs proper lists; `pair` also permits
improper lists. There is no implicit conversion from arbitrary Fibber objects.
Floating terms, arithmetic constraints, disequality constraints, rational trees,
and relation compilation are outside this initial core.

`run limit builder` allocates one query variable, calls the builder once, and
returns up to `limit` reified terms. `run-many limit variables builder` passes a
vector of query variables and returns vectors of reified terms. A zero-variable
query can return empty answer tuples. Limits and variable counts must be
nonnegative, including when the answer limit is zero.

A zero answer limit does not invoke the builder or force any goal. A positive
limit stops as soon as enough answers have been found and does not inspect the
next pending goal. Duplicate answers are retained. A search with fewer finite
answers returns those answers; a search with no answers and infinitely many
steps can continue forever.

Use `run`, `run-many`, and `call-fresh`/`fresh` to allocate variables. IDs are
local to each branch's immutable state, with query IDs reserved before fresh
allocation. Branches never merge their substitutions. `Var` in an answer is a
canonical display name, not a freshly allocated variable for another query.

Unbound variables reify to `Var 0`, `Var 1`, and so on, shown as `_.0`, `_.1`.
Renaming restarts per answer and is shared across every term in an answer tuple,
so aliases remain aliases. Ground terms retain their values and structure.

## Unification

Substitutions are persistent `Map i64 Term` values. `walk` follows variable
aliases; `unify` returns an optional new substitution. Inputs remain unchanged,
including when unifying the head of a pair succeeds and its tail fails.

Occurs checking follows existing substitutions and rejects both direct and
indirect cyclic bindings. Unifying a variable with itself succeeds without
extending the map. This engine implements finite terms, rather than matching
every historical miniKanren implementation's occurs-check policy.

There are no raw pointers, mutable global substitutions, or shared variable
counters in the implementation. Fresh ID increments retain checked i64 overflow.

## Goals and Lisp syntax

The functional API is suitable for a qualified import:

```clojure
(ns main (:require [fib.logic :as l]))

(l/run 2 (fn (q)
  (l/any [(l/== q (l/integer 1))
          (l/== q (l/integer 2))])))
```

`==` constructs a unification goal. `all` conjoins a vector of goals;
`any` disjoins one. Empty conjunction succeeds once; empty disjunction fails.
`call-fresh` delays a variable-binding builder until that goal is evaluated.
`delay-goal` suspends a zero-argument goal builder.

The macros use qualified facade references in their generated code. Under the
current module visibility rules, import the facade with `:use fib.logic` for
macro syntax; a `:require` alias alone suffices for the functional API.

```clojure
(ns main (:require [fib.logic :as l]) (:use fib.logic))

(defun appendo (x: l/Term y: l/Term z: l/Term) -> l/Goal
  (conde [(l/== x (l/nil-term)) (l/== y z)]
         [(fresh [head tail rest]
            (l/== x (l/pair head tail))
            (l/== z (l/pair head rest))
            (zzz (appendo tail y rest)))]))
```

`conde` accepts list or vector clauses, conjoining each clause before disjoining
them. `fresh` accepts a list or vector of variable names and conjoins its body.
`zzz` wraps one goal expression in `delay-goal`. Qualified generated helpers
avoid capture by caller functions with the same names; fresh's named variables
are intentional lexical bindings.

Recursive relations must delay their recursive construction, as `appendo` does.
Ordinary function calls evaluate while constructing a goal; the scheduler cannot
rescue a function that never returns a goal. Builders should be pure. The engine
is a replayable recipe, not a memoized stream: replaying the same search can run
its builders again, and effects are not rolled back by failed unification.

## Fair, explicit search

A work item contains an immutable state and a linked list of remaining goals.
A search contains an immutable FIFO queue represented by a front vector,
a position, and a back vector. Exhausting the front rotates the back into place;
no recursive stream flattening or queue reversal is required.

`step` returns `SearchDone`, `SearchProgress rest`, or `SearchFound state rest`.
One transition evaluates at most one goal:

- Unification either enqueues the remaining conjunction or drops the branch.
- Conjunction prepends its goals to that branch's remaining work.
- Disjunction enqueues each alternative with the same state and continuation.
- Fresh allocation or delay invokes one builder and enqueues its returned goal.
- A completed conjunction produces an answer immediately, without another
  queue turn that could force an unrelated branch.

New work joins the back of the queue. This makes both disjunction and the
continuations of conjunction fair when each transition terminates and each
disjunction has finitely many alternatives. An infinitely delayed branch cannot
starve another queued branch with a finite answer. Answer order is deterministic
for pure builders and this FIFO scheduler; it is not promised to match the
ordering of other miniKanren implementations. Extra conjunction wrappers can
change relative answer order by adding transitions.

Bounded stepping means bounded forcing, not a constant CPU-time guarantee:
unification traverses finite terms, a large disjunction enqueues its alternatives,
and user builders can do arbitrary computation. Very deeply nested terms still
use recursive unification, occurs checking, reification, and formatting.

`next-answer` advances to the next result; `take-states` collects bounded states.
`start` begins a closed goal with an empty state, while `start-in` supports an
explicit initial state. Search values can be replayed independently. Dropping a
search releases its remaining work through ordinary reference counting; no
background workers need cancellation in this sequential implementation.

## Red, green, refactor evidence

The five initial cases first failed because the library did not exist, then
reported `5 cases: 5 pass, 0 fail, 0 pending`. Replacing the recursive interleaved
stream prototype with the explicit FIFO engine retained those five passes.
The syntax case failed on missing `conde`/`fresh` before their implementation.

The final targeted run reported `13 cases: 13 pass, 0 fail, 0 pending`:

- 7100–7101: aliasing, finite-term occurs checks, conjunction/disjunction, shared
  reification, and unbound variables.
- 7102: diverging disjunctions, two infinite producers, and conjunction fairness.
- 7103: zero demand, builder invocation counts, and immutable search snapshots.
- 7104–7105: bidirectional list append and the Lisp macros.
- 7106: 512 nested conjunctions with no premature delayed-body invocation,
  bounded demand, and clean release of abandoned work.
- 7107: every list split for lengths 0–12 against an independent slicing model,
  forward append for all 91 splits, empty tuples, duplicates, and term formatting.
- 7108: persistent substitutions, rollback on tail failure, and indirect cycles.
- 7109: an answer limit stops before forcing a later suspended branch. This
  case first failed with `expected 0, got 2`; immediately emitting a completed
  conjunction made it pass.
- 7140–7142: negative answer limits and query-variable counts trap.

Accept cases require clean native memory audits. Fault injection uses isolated
copies of the library, never the working tree: disabling occurs checking makes
7108 fail, and replacing FIFO enqueue with depth-first insertion makes the
fairness case exceed a 10-second watchdog instead of producing its bounded
answers. Neither fault is permitted to pass as an open case.

## Parallel work after this reference

A parallel engine should schedule explicit work items in bounded chunks, preserve
substitution isolation, and stop outstanding work after enough answers. Decide
whether it preserves deterministic ordering or offers a separately named mode
with unspecified ordering before exposing it as an API.

The current builders have local function types and may capture local values.
Work containing them is not automatically transferable to threads. Parallel
execution needs sendable goal builders or defunctionalized relation descriptions,
plus a tested ownership and cancellation protocol. A FIFO work representation
is a useful starting point, not evidence that those requirements are already met.
SIMD belongs first in measured numeric constraint kernels or suitably uniform
batches, rather than assuming arbitrary symbolic unification vectorizes.

## Repository checks

The final quick gate reported `GATE PASS (quick)` after rebuilding stage 2,
checking all ownership and module cases, and checking the standard-library
sample against its expected non-passing set. The separate 13-case logic run
covers every new case, including those outside that sample. The append example
also ran with the rebuilt compiler and printed all three splits of `(tea cup)`
and the missing prefix `(tea)`.

The repository quick benchmark preserved all ten answer checksums but reported
baseline timing flags: `num-f64` +13.6%, `num-nbody` +10.1%, `vec-sort` +20.9%,
`map-assoc-get` +25.0%, and `lazy-fused` +14.7%. The rebuilt compiler binary is
byte-identical to the compiler from the preceding numerical milestone
(SHA-256 `4f67327eb4caf7428d42ed9b75e924819671d7b58c37923a1c44e486da61e30e`).
The timing differences remain unresolved; the baseline was not rewritten.

Session records: `/tmp/fibber-logic-red.log`,
`/tmp/fibber-logic-demand-red.log`, `/tmp/fibber-logic-final-cases.log`,
`/tmp/fibber-logic-gate-final.log`, and `/tmp/fibber-logic-quick-bench.log`.
