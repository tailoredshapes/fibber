# fib.logic

A sequential miniKanren-style library with typed finite terms, persistent
unification, occurs checking, canonical reification, and fair FIFO search.

```clojure
(ns main (:require [fib.logic :as l]) (:use fib.logic))

(defun main () -> i64
  (do (println (l/run 2 (fn (q)
        (conde [(l/== q (l/integer 1))]
               [(fresh [x] (l/== x (l/integer 2)) (l/== q x))]))))
      0))
```

| API | Meaning |
|---|---|
| `integer`, `symbol`, `nil-term`, `pair`, `list-term` | Construct typed terms |
| `== left right` | Unification goal |
| `all goals`, `any goals` | Conjunction/disjunction of goal vectors |
| `call-fresh builder`, `delay-goal builder` | Allocate a variable or suspend construction |
| `fresh [names] goals…`, `conde clauses…`, `zzz goal` | Lisp syntax over those functions |
| `run limit builder` | Reified answers for one query variable |
| `run-many limit variables builder` | Reified answer tuples |
| `start`, `start-in`, `step`, `next-answer`, `take-states` | Explicit incremental search |

Use `:use fib.logic` for macro syntax under current module visibility rules;
a qualified `:require` alone works for the functional API. Recursive relations
must suspend recursive construction with `zzz` or `delay-goal`. Builders should
be pure. A zero answer limit never invokes the builder; duplicates are retained.
Unbound variables print as `_.0`, `_.1`, preserving aliases within each answer.

Run [the append example](../../../examples/logic.fib) with a current stage-2
compiler. It concatenates lists, infers missing prefixes, and enumerates splits
using the same relation. Executable cases 7100–7109 and 7140–7142 cover semantics,
fairness, demand, and memory audits:

```sh
fibc cases cases/stdlib --only 7100 7101 7102 7103 7104 7105 7106 7107 7108 7109 7140 7141 7142 -j 4
```

Finite-domain constraints are available from `fib.logic.fd`:

| API | Meaning |
|---|---|
| `interval lo hi`, `domain values` | Construct a finite integer domain |
| `in term domain`, `in-all terms domain` | Restrict one or many terms |
| `batch goals` | Post equality and FD goals together, then propagate once |
| `distinct terms` | Enforce pairwise all-different values |
| `!= left right`, `< left right`, `<= left right` | Relational arithmetic constraints |
| `+ left right result` | Constrain an integer sum |
| `size domain` | Count the values in a domain |

The solver watches variables and queues only affected constraints when a domain
or binding changes. It propagates before choosing a value, uses a smallest-domain
first branch heuristic, and keeps each branch immutable. Indexed domains use
persistent vectors with sparse storage for unusual variable IDs. Compact domains use a
63-bit window; large intervals stay lazy as bounds, while irregular values use
sorted sparse storage. For example:

```clojure
(let ((d (fd/interval 1 9)))
  (l/run 1 (fn (q)
    (l/all [(fd/in q d) (fd/!= q (l/integer 4))]))))
```

Use `fd/batch` for a model's finite-domain setup:

```clojure
(l/run-many 6 3 (fn (qs)
  (fd/batch [(fd/in-all qs (fd/interval 1 3))
             (fd/distinct qs)
             (fd/< (nth qs 0) (nth qs 1))])))
```

A batch accepts FD constraints, equality goals, and nested `all` or batch goals.
Build fresh variables outside the batch, and put a separate batch inside each
disjunction branch. Delayed goals and disjunctions are rejected. A batch is one
scheduler step; interleaving with other branches can differ from separate goals.
`in-all` also posts its memberships as one batch.

The [finite-domain design and Sudoku port](../../../docs/design/finite-domains-and-sudoku.md)
describes propagation, the port of `tsmarsh/sudoku`, and reproducible
performance measurements. SIMD and parallel search are still separate concerns:
Sudoku's irregular branch tree does not automatically benefit from either.

Run the finite-domain and Sudoku cases with:

```sh
fibc cases cases/stdlib --only 7200 7201 7202 7203 7204 7205 7206 7207 7208 7209 7210 7211 7212 -j 4
```

See [the design and validation record](../../../docs/design/relational-search.md).
Each scheduler step forces at most one goal; it does not bound the CPU time of
unification or arbitrary user code.
